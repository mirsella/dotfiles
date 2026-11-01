//! Wait for the declared LAN profile to finish activation before clock handoff.
use crate::{Result, check, output};
use std::{
    io::{BufRead, BufReader},
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

const UUID: &str = "c4e6863b-7f76-4a4d-9a48-9937af0b5a36";

fn ready(snapshot: &str) -> Result<bool> {
    let value = |key: &str| snapshot.lines().find_map(|s| s.strip_prefix(key));
    let state: u32 = value("GENERAL.STATE:")
        .ok_or("missing LAN state")?
        .split_whitespace()
        .next()
        .ok_or("empty LAN state")?
        .parse()?;
    check(
        matches!(
            state,
            0 | 10 | 20 | 30 | 40 | 50 | 60 | 70 | 80 | 90 | 100 | 110 | 120
        ),
        "unknown LAN state",
    )?;
    check(
        !matches!(state, 0 | 10 | 120),
        "LAN unknown/unmanaged/failed",
    )?;
    let uuid = value("GENERAL.CON-UUID:").ok_or("missing LAN UUID")?;
    if state != 100 {
        return Ok(false);
    }
    check(
        uuid == UUID,
        format!("connected LAN profile changed: {uuid}"),
    )?;
    let dns: Vec<_> = snapshot
        .lines()
        .filter(|s| s.starts_with("IP4.DNS["))
        .collect();
    Ok(value("IP4.GATEWAY:") == Some("192.168.1.1")
        && dns.len() == 1
        && dns[0].ends_with(":192.168.1.1"))
}

fn receive(
    rx: &mpsc::Receiver<()>,
    duration: Duration,
    mut snapshot: impl FnMut() -> Result<String>,
) -> Result<()> {
    let until = Instant::now() + duration;
    loop {
        let state = snapshot()?;
        println!("LAN readiness snapshot:\n{state}");
        if ready(&state)? {
            return Ok(());
        }
        let remaining = until.saturating_duration_since(Instant::now());
        if remaining.is_zero() || rx.recv_timeout(remaining).is_err() {
            // Covers a signal emitted before busctl completed subscription.
            let final_state = snapshot()?;
            println!("LAN readiness final snapshot:\n{final_state}");
            return check(
                ready(&final_state)?,
                "LAN handoff deadline/monitor ended before readiness",
            );
        }
    }
}

pub fn wait() -> Result<()> {
    let mut child = Command::new("busctl").args([
        "--system", "--no-pager",
        "--match=type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged'",
        "monitor", "org.freedesktop.NetworkManager",
    ]).stdout(Stdio::piped()).spawn()?;
    let pipe = child.stdout.take().ok_or("missing network monitor pipe")?;
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(pipe).lines() {
            match line {
                Ok(line) if line.contains("Member=PropertiesChanged") => {
                    if tx.send(()).is_err() {
                        break;
                    }
                }
                Ok(_) => (),
                Err(_) => break,
            }
        }
    });
    let result = receive(&rx, Duration::from_secs(30), || {
        output(
            "timeout",
            &[
                "5s",
                "nmcli",
                "-t",
                "-f",
                "GENERAL.STATE,GENERAL.CON-UUID,IP4.GATEWAY,IP4.DNS",
                "device",
                "show",
                "enp3s0f1",
            ],
        )
    });
    let stopped = child.kill();
    let reaped = child.wait();
    let joined = reader.join();
    println!(
        "LAN monitor cleanup: stop={stopped:?} reap={reaped:?} joined={}",
        joined.is_ok()
    );
    stopped?;
    reaped?;
    check(joined.is_ok(), "LAN monitor reader panicked")?;
    result?;
    check(
        output(
            "systemctl",
            &[
                "show",
                "NetworkManager.service",
                "-p",
                "ActiveState",
                "--value",
            ],
        )? == "active",
        "NetworkManager stopped during LAN handoff",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state(n: u32) -> String {
        format!(
            "GENERAL.STATE:{n} (state)\nGENERAL.CON-UUID:{UUID}\nIP4.GATEWAY:192.168.1.1\nIP4.DNS[1]:192.168.1.1"
        )
    }
    #[test]
    fn activating_profile_is_not_ready_even_with_address_and_dns() {
        assert!(!ready(&state(70)).unwrap());
        assert!(ready(&state(100)).unwrap());
    }
    #[test]
    fn rejects_wrong_profile_failed_state_and_wrong_routes() {
        assert!(ready(&state(100).replace(UUID, "wrong")).is_err());
        assert!(ready(&state(120)).is_err());
        assert!(!ready(&state(100).replace("IP4.GATEWAY:192.168.1.1", "IP4.GATEWAY:")).unwrap());
        assert!(!ready(&(state(100) + "\nIP4.DNS[2]:8.8.8.8")).unwrap());
    }
    #[test]
    fn activation_event_and_missed_subscription_are_both_handled() {
        let (tx, rx) = mpsc::channel();
        tx.send(()).unwrap();
        let mut snapshots = [state(70), state(100)].into_iter();
        receive(&rx, Duration::from_secs(1), || {
            Ok(snapshots.next().unwrap())
        })
        .unwrap();
        let mut snapshots = [state(70), state(100)].into_iter();
        receive(&rx, Duration::ZERO, || Ok(snapshots.next().unwrap())).unwrap();
    }
    #[test]
    fn deadline_or_wrong_final_profile_cannot_pass() {
        let (_tx, rx) = mpsc::channel();
        assert!(receive(&rx, Duration::ZERO, || Ok(state(70))).is_err());
        let mut snapshots = [state(70), state(100).replace(UUID, "wrong")].into_iter();
        assert!(receive(&rx, Duration::ZERO, || Ok(snapshots.next().unwrap())).is_err());
    }
}
