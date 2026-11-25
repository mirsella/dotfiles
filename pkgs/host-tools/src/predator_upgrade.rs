use crate::{t3_cleanup, t3_settings, util};
use anyhow::{Context, Result, bail, ensure};
use chrono::{Local, Timelike, Utc};
use serde::Deserialize;
use std::{env, path::PathBuf, thread, time::Duration};

const MIN_FREE: u64 = 30 * 1024 * 1024 * 1024;
const RECHECK: Duration = Duration::from_secs(5 * 60);
const MAINTENANCE: &[&str] = &[
    "db-backup.service",
    "ente-db-backup.service",
    "hermes-backup.service",
    "recovery-backup.service",
    "restic-*.service",
    "sanoid.service",
    "zfs-scrub*.service",
    "zpool-trim.service",
    "fstrim.service",
    "nix-gc.service",
];

fn capture(program: &str, args: &[&str]) -> Result<String> {
    util::output("timeout", &[&["30s", program], args].concat())
}

fn available_bytes(output: &str) -> Result<u64> {
    let mut lines = output.lines();
    ensure!(lines.next().is_some(), "missing df header");
    let bytes = lines.next().context("missing root filesystem in df")?;
    ensure!(lines.next().is_none(), "multiple root filesystems in df");
    bytes.trim().parse().context("invalid free space in df")
}

#[derive(Deserialize)]
struct LoginSession {
    class: String,
}

fn has_login(output: &str) -> Result<bool> {
    Ok(serde_json::from_str::<Vec<LoginSession>>(output)?
        .iter()
        .any(|session| !matches!(session.class.as_str(), "manager" | "manager-early")))
}

pub fn check_t3() -> Result<()> {
    let home = PathBuf::from(env::var_os("HOME").context("HOME is not set")?);
    let base = t3_settings::resolve_base_dir(&home, None);
    let Some(session) = t3_settings::Session::open_for_cleanup(&base)? else {
        return Ok(());
    };
    let shell = session.shell()?;
    ensure!(shell["schemaVersion"] == 2, "unknown T3 shell schema");
    let now = Utc::now();
    for key in ["threads", "archivedThreads"] {
        for item in shell[key]
            .as_array()
            .with_context(|| format!("missing T3 {key}"))?
        {
            ensure!(!t3_cleanup::busy(item, now)?, "T3 has active work");
        }
    }
    Ok(())
}

fn check_idle(user: &str, executable: &str) -> Result<()> {
    ensure!(
        !has_login(&capture("loginctl", &["list-sessions", "--json=short"])?)?,
        "a user is logged in"
    );
    // Include direct SSH and public web/proxy connections. Check T3's job
    // state separately: linked clients may keep a transport open while idle.
    let connections = capture(
        "ss",
        &[
            "-Hnt",
            "state",
            "established",
            "( sport = :22 or sport = :80 or sport = :443 or sport = :4096 or sport = :14096 )",
        ],
    )?;
    ensure!(
        connections.trim().is_empty(),
        "SSH or web clients are connected"
    );
    let load: f64 = util::read("/proc/loadavg")?
        .split_whitespace()
        .next()
        .context("missing one-minute load average")?
        .parse()?;
    ensure!((0.0..=1.0).contains(&load), "host load is {load:.2}");
    let units = capture(
        "systemctl",
        &[
            &[
                "list-units",
                "--type=service",
                "--state=active,activating,deactivating",
                "--no-legend",
                "--no-pager",
                "--plain",
            ],
            MAINTENANCE,
        ]
        .concat(),
    )?;
    ensure!(
        units.trim().is_empty(),
        "backup or maintenance services are active"
    );
    ensure!(
        capture("zpool", &["status", "-x"])?.trim() == "all pools are healthy",
        "ZFS pools are not healthy"
    );
    ensure!(
        !capture("zpool", &["status"])?.contains(" in progress"),
        "a ZFS scan or trim is running"
    );
    capture(
        "runuser",
        &["-u", user, "--", executable, "t3-maintenance-idle"],
    )?;
    Ok(())
}

pub fn wait_until_idle(user: &str) -> Result<()> {
    let executable = env::current_exe().context("locate host-tools")?;
    let executable = util::path(&executable)?;
    for attempt in 0..24 {
        ensure!(
            (4..6).contains(&Local::now().hour()),
            "outside the 04:00 to 06:00 upgrade window"
        );
        let bytes = available_bytes(&capture("df", &["-B1", "--output=avail", "/"])?)?;
        ensure!(
            bytes >= MIN_FREE,
            "root has {:.1} GiB free; the daily upgrade requires at least 30 GiB",
            bytes as f64 / (1024.0 * 1024.0 * 1024.0)
        );
        match check_idle(user, executable) {
            Ok(()) if (4..6).contains(&Local::now().hour()) => return Ok(()),
            Ok(()) => break,
            Err(error) => eprintln!("Predator upgrade deferred: {error:#}"),
        }
        if attempt < 23 {
            thread::sleep(RECHECK);
        }
    }
    bail!("no verified idle slot before 06:00")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_user_manager_is_not_a_login() -> Result<()> {
        assert!(!has_login(r#"[{"class":"manager"}]"#)?);
        assert!(has_login(r#"[{"class":"manager"},{"class":"user"}]"#)?);
        assert!(has_login(r#"[{"class":"future-class"}]"#)?);
        assert!(has_login(r#"[{}]"#).is_err());
        Ok(())
    }

    #[test]
    fn free_space_must_be_one_numeric_root_result() -> Result<()> {
        assert_eq!(available_bytes("       Avail\n 32212254720\n")?, MIN_FREE);
        assert!(available_bytes("Avail\n").is_err());
        assert!(available_bytes("Avail\nerror\n").is_err());
        assert!(available_bytes("Avail\n1\n2\n").is_err());
        Ok(())
    }
}
