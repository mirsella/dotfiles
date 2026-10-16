//! Independent Pi notices. Telegram failures must not gate wake/probe jobs.
use super::*;

const EVENTS: [&str; 11] = [
    "setup",
    "start",
    "wake-ok",
    "wake-failed",
    "retry-ok",
    "retry-failed",
    "probe-ok",
    "probe-failed",
    "snapshot-ok",
    "snapshot-failed",
    "canceled",
];

pub(super) fn credentials() -> Result<(String, String)> {
    let token = env::var("TgToken")?;
    let chat = env::var("TgId")?;
    check(
        !token.is_empty()
            && token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b":-_".contains(&b)),
        "invalid Telegram token",
    )?;
    check(
        !chat.is_empty() && chat.bytes().all(|b| b.is_ascii_digit() || b == b'-'),
        "invalid Telegram chat id",
    )?;
    Ok((token, chat))
}

fn unit_arg(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
            .replace('$', "$$")
            .replace('\n', "\\n")
    )
}

fn service(config: &str, text: &str) -> String {
    format!("[Unit]\nDescription=Predator study Telegram notice\n[Service]\nType=oneshot\nTimeoutStartSec=25s\nUMask=0077\nExecStart=/usr/bin/curl --silent --show-error --fail --connect-timeout 5 --max-time 20 --config {} --data-urlencode {}\n", unit_arg(config), unit_arg(&format!("text={text}")))
}

fn notice(trial: &Trial, event: &str) -> String {
    format!("{}-notice-{event}", trial.base())
}

fn install(path: &str, body: &str) -> Result<String> {
    check(
        !body.lines().any(|line| line == "PM_NOTICE_EOF"),
        "invalid unit delimiter",
    )?;
    Ok(format!(
        "cat > {} <<'PM_NOTICE_EOF'\n{body}PM_NOTICE_EOF\n",
        quote(path)
    ))
}

fn provision(trial: &Trial, config: &str) -> Result<()> {
    let (token, chat) = credentials()?;
    let mut child = Command::new("timeout")
        .args([
            "30s",
            "ssh",
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=5",
            "-p",
            "22",
            "-l",
            "mirsella",
            PI,
        ])
        .arg(format!(
            "sudo -n bash -c {}",
            quote(&format!("set -euC; umask 077; cat > {}", quote(config)))
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    // Send the private config only over SSH stdin. Never put it in argv or captures.
    child.stdin.take().ok_or("missing credential input")?.write_all(
        format!("url = \"https://api.telegram.org/bot{token}/sendMessage\"\ndata-urlencode = \"chat_id={chat}\"\n").as_bytes()
    )?;
    let result = child.wait_with_output()?;
    check(result.status.success(), "Pi Telegram credential provisioning failed; inspect existing per-trial config before retrying")?;
    write_new(
        &trial.out.join("notification-credentials.txt"),
        format!(
            "host={PI}\npath={config}\nmode=0600\ntransport=SSH stdin\ncredentials=not captured\n"
        )
        .as_bytes(),
    )
}

pub(super) fn arm(trial: &Trial) -> Result<()> {
    check(
        epoch()? + 300 < trial.due,
        "need five minutes to prepare Telegram notices",
    )?;
    credentials()?;
    let config = format!("/run/{}-telegram.conf", trial.base());
    let mut guard = format!("set -eu\ntest ! -e {}\ncommand -v curl\n", quote(&config));
    for event in EVENTS {
        guard += &format!(
            "test \"$(systemctl show {}.service -p LoadState --value)\" = not-found\n",
            quote(&notice(trial, event))
        );
    }
    for kind in ["wake", "retry", "probe", "snapshot"] {
        let unit = format!("{}-{kind}", trial.base());
        guard += &format!("test \"$(systemctl show {}.timer -p SubState --value)\" = waiting\ntest ! -e /run/systemd/system/{}.service.d/60-telegram.conf\n", quote(&unit), unit);
    }
    trial.remote("notification-preflight", PI, &guard)?;
    provision(trial, &config)?;
    let due = Command::new("date")
        .env("TZ", "Europe/Paris")
        .args(["-d", &format!("@{}", trial.due), "+%F %T %Z"])
        .output()?;
    check(due.status.success(), "cannot format trial time")?;
    let wake = Command::new("date")
        .env("TZ", "Europe/Paris")
        .args(["-d", &format!("@{}", trial.wake), "+%T %Z"])
        .output()?;
    check(wake.status.success(), "cannot format wake time")?;
    let due = String::from_utf8(due.stdout)?.trim().to_string();
    let wake = String::from_utf8(wake.stdout)?.trim().to_string();
    let prefix = format!("Predator / {} / Pi observer", trial.id);
    let messages = [
        ("setup", format!("Notifications enabled. Test window: {due}; first wake {wake}, retry +30s. Keep the power graph. Schedule notices do not prove sleep entry.")),
        ("start", format!("Test window is starting now. First wake: {wake}. Suspend entry is not yet confirmed; preflight or inhibitors can refuse it. Watch the power graph.")),
        ("wake-ok", "First WoL command completed successfully. Packet receipt and resume are not yet confirmed.".into()),
        ("wake-failed", "First WoL command FAILED. The independent retry is still scheduled.".into()),
        ("retry-ok", "Retry WoL command completed successfully. Reachability check follows.".into()),
        ("retry-failed", "Retry WoL command FAILED. Keep the power graph and inspect the observer logs.".into()),
        ("probe-ok", "SSH responded to the Pi probe. This is reachability only, not a suspend PASS. Kernel logs, timing and power graph still need review.".into()),
        ("probe-failed", "SSH probe FAILED: Predator is unreachable. Possible hang; check power/LEDs. Preserve the graph and allow the snapshot at wake +3 minutes before attended recovery when practical.".into()),
        ("snapshot-ok", "Full netconsole snapshot saved on Pi. Main-side collection follows. Preserve the power graph; final classification is pending.".into()),
        ("snapshot-failed", "Pi netconsole snapshot FAILED. Preserve the original append log and power graph; manual collection is needed.".into()),
        ("canceled", "Test canceled before entry. Primary and wake timers were stopped.".into()),
    ];
    let mut script = String::from("set -euC\numask 077\n");
    for (event, message) in messages {
        script += &install(
            &format!("/run/systemd/system/{}.service", notice(trial, event)),
            &service(&config, &format!("{prefix}\n{message}")),
        )?;
    }
    script += &install(&format!("/run/systemd/system/{}.timer", notice(trial, "start")), &format!("[Unit]\nDescription=Predator study start-window notice\n[Timer]\nOnCalendar=@{}\nAccuracySec=1s\nRandomizedDelaySec=0\nPersistent=no\n", trial.due))?;
    for kind in ["wake", "retry", "probe", "snapshot"] {
        let directory = format!("/run/systemd/system/{}-{kind}.service.d", trial.base());
        script += &format!("mkdir -p {}\n", quote(&directory));
        script += &install(
            &format!("{directory}/60-telegram.conf"),
            &format!(
                "[Unit]\nOnSuccess={}.service\nOnFailure={}.service\n",
                notice(trial, &format!("{kind}-ok")),
                notice(trial, &format!("{kind}-failed"))
            ),
        )?;
    }
    script += &format!("systemctl daemon-reload\nsystemctl start {}.timer\ntest \"$(systemctl show {}.timer -p SubState --value)\" = waiting\nsystemctl start {}.service\njournalctl -u {}.service --no-pager -o cat\n", quote(&notice(trial, "start")), quote(&notice(trial, "start")), quote(&notice(trial, "setup")), quote(&notice(trial, "setup")));
    for kind in ["wake", "retry", "probe", "snapshot"] {
        script += &format!("test \"$(systemctl show {}-{kind}.timer -p SubState --value)\" = waiting\nsystemctl show {}-{kind}.service -p ExecStart -p OnSuccess -p OnFailure\n", quote(&trial.base()), quote(&trial.base()));
    }
    script += &format!(
        "systemctl cat {}.timer\nstat -c '%a %U %n' {}\n",
        quote(&notice(trial, "start")),
        quote(&config)
    );
    trial.remote("notification-armed", PI, &script)?;
    Ok(())
}

pub(super) fn cancel(trial: &Trial) -> Result<()> {
    trial.remote("notification-cancel", PI, &format!("set -eu\nif test \"$(systemctl show {}.timer -p LoadState --value)\" != not-found; then systemctl stop {}.timer; systemctl start {}.service; fi\n", quote(&notice(trial, "start")), quote(&notice(trial, "start")), quote(&notice(trial, "canceled"))))?;
    Ok(())
}

pub(super) fn collect(trial: &Trial) -> Result<()> {
    let mut script = String::from("set -eu\ndate --iso-8601=seconds\n");
    for event in EVENTS {
        let unit = notice(trial, event);
        script += &format!("systemctl show {u}.service -p LoadState -p ActiveState -p Result -p ExecMainStatus\njournalctl -u {u}.service --no-pager -o short-iso-precise\n", u=quote(&unit));
    }
    trial.remote("notification-results", PI, &script)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unit_arguments_preserve_literal_expansions_and_quotes() {
        assert_eq!(unit_arg("a\"%n $HOME\nb"), "\"a\\\"%%n $$HOME\\nb\"");
        let unit = service("/run/private.conf", "SSH responded: not a PASS");
        assert!(unit.contains("--config \"/run/private.conf\""));
        assert!(!unit.contains("api.telegram.org"));
        assert!(!unit.contains("Requires="));
    }
}
