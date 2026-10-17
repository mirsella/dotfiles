use super::*;
use std::{
    net::{SocketAddr, TcpStream},
    time::Duration,
};

const MIN_AWAKE: u64 = 39600;
const FAST: &str = "/dev/disk/by-id/ata-CT240BX500SSD1_2004E3E6DE68-part1";

fn uuid(value: &str) -> Result<()> {
    check(
        value.len() == 36
            && value.bytes().enumerate().all(|(i, c)| {
                if [8, 13, 18, 23].contains(&i) {
                    c == b'-'
                } else {
                    c.is_ascii_hexdigit()
                }
            }),
        "invalid boot ID",
    )
}

fn schedule(epoch: u64, age: u64) -> Result<(u64, u64)> {
    check(age <= epoch, "uptime exceeds epoch")?;
    let due = (epoch - age + MIN_AWAKE + 10).max(epoch + 120);
    Ok((due, due + 1800))
}

fn plan_path(boot: &str) -> String {
    format!("{STATE}/{boot}-auto-plan.conf")
}
fn armed_path(boot: &str) -> String {
    format!("{STATE}/{boot}-auto-armed.txt")
}

pub fn prepare(previous: &str) -> Result<()> {
    not_held()?;
    uuid(previous)?;
    let boot = text("/proc/sys/kernel/random/boot_id")?;
    if boot == previous {
        println!("STATUS=waiting-for-reboot");
        return Ok(());
    }
    let path = plan_path(&boot);
    if !Path::new(&armed_path(&boot)).exists() {
        capture()?;
        let state = preflight(&boot, TpmState::Absent, EcState::NotRun)?;
        check(
            state.successes == 0,
            "automatic arm requires a fresh boot without prior sleep",
        )?;
        if !Path::new(&path).exists() {
            let (due, wake) = schedule(now()?, state.awake_seconds)?;
            save(
                &format!("{boot}-auto-plan.conf"),
                &format!(
                    "boot={boot}\ndue={due}\nwake={wake}\ntpm=absent\nec=not-run\nmin_awake_seconds={MIN_AWAKE}\n"
                ),
            )?;
        }
    }
    let plan = text(&path)?;
    validate_plan(&plan, &boot)?;
    println!(
        "STATUS={}\nBOOT={boot}\nDUE={}\nWAKE={}",
        if Path::new(&armed_path(&boot)).exists() {
            "armed"
        } else {
            "ready"
        },
        field(&plan, "due")?,
        field(&plan, "wake")?
    );
    Ok(())
}

fn validate_plan(plan: &str, boot: &str) -> Result<(u64, u64)> {
    uuid(boot)?;
    check(field(plan, "boot")? == boot, "automatic plan boot changed")?;
    check(
        TpmState::from_plan(plan)? == TpmState::Absent
            && EcState::from_plan(plan)? == EcState::NotRun,
        "automatic plan arm changed",
    )?;
    check(
        field(plan, "min_awake_seconds")?.parse::<u64>()? == MIN_AWAKE,
        "automatic age guard changed",
    )?;
    let due: u64 = field(plan, "due")?.parse()?;
    let wake: u64 = field(plan, "wake")?.parse()?;
    check(wake == due + 1800, "automatic wake interval changed")?;
    Ok((due, wake))
}

pub fn arm(boot: &str) -> Result<()> {
    not_held()?;
    uuid(boot)?;
    check(
        text("/proc/sys/kernel/random/boot_id")? == boot,
        "boot changed before arming",
    )?;
    let path = plan_path(boot);
    let plan = text(&path)?;
    let (due, wake) = validate_plan(&plan, boot)?;
    if Path::new(&armed_path(boot)).exists() {
        check(
            text(armed_path(boot))? == plan,
            "existing armed state differs from plan",
        )?;
        return Ok(());
    }
    preflight(boot, TpmState::Absent, EcState::NotRun)?;
    check(now()? < due, "automatic setup missed its suspend time")?;
    let unit = format!("predator-pm-auto-{}", boot.replace('-', ""));
    let timer = format!("{unit}.timer");
    let when = output(
        "date",
        &["-d", &format!("@{due}"), "+%Y-%m-%d %H:%M:%S Europe/Paris"],
    )?;
    let definition = output("systemctl", &["show", &timer, "-p", "LoadState", "--value"])?;
    let exe = env::current_exe()?;
    if definition == "not-found" {
        output(
            "systemd-run",
            &[
                &format!("--unit={unit}"),
                &format!("--on-calendar={when}"),
                "--timer-property=AccuracySec=1s",
                "--timer-property=RandomizedDelaySec=0",
                "--timer-property=Persistent=no",
                "--property=Type=oneshot",
                "--property=TimeoutStartSec=40min",
                "--property=Environment=PATH=/run/current-system/sw/bin",
                exe.to_str().ok_or("invalid executable path")?,
                "attempt-plan",
                &path,
            ],
        )?;
    } else {
        check(definition == "loaded", "unexpected automatic timer state")?;
        let content = output("systemctl", &["cat", &timer, &format!("{unit}.service")])?;
        check(
            content.contains(&when)
                && content.contains(&path)
                && content.contains(&exe.display().to_string()),
            "existing automatic timer differs from plan",
        )?;
    }
    check(
        output("systemctl", &["show", &timer, "-p", "SubState", "--value"])? == "waiting",
        "automatic timer is not waiting",
    )?;
    save(&format!("{boot}-auto-armed.txt"), &format!("{plan}\n"))?;
    marker(&format!(
        "AUTO ARMED boot={boot} due={due} wake={wake} TPM=absent raw-EC=not-run min_awake={MIN_AWAKE}"
    ))
}

pub fn unlock_fast() -> Result<()> {
    check(
        text("/proc/cmdline")?
            .split_whitespace()
            .any(|p| p == "initcall_blacklist=crb_acpi_driver_init")
            && !Path::new("/dev/tpm0").exists(),
        "refusing password handoff outside TPM-free boot",
    )?;
    if !Path::new("/dev/mapper/fast-crypt").exists() {
        let mut secret = Vec::new();
        std::io::stdin().read_to_end(&mut secret)?;
        check(!secret.is_empty(), "empty fast unlock credential")?;
        let mut child = Command::new("cryptsetup")
            .args([
                "open",
                "--type",
                "luks",
                "--allow-discards",
                "--key-file",
                "-",
                FAST,
                "fast-crypt",
            ])
            .stdin(Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .ok_or("missing cryptsetup input")?
            .write_all(&secret)?;
        check(child.wait()?.success(), "fast unlock failed")?;
    }
    output(
        "systemctl",
        &[
            "start",
            "systemd-cryptsetup@fast\\x2dcrypt.service",
            "zfs-import-fast.service",
        ],
    )?;
    output(
        "systemctl",
        &[
            "start",
            "srv-data-fast.mount",
            "var-lib-nextcloud-data.mount",
        ],
    )?;
    output(
        "systemctl",
        &[
            "start",
            "nextcloud-setup.service",
            "phpfpm-nextcloud.service",
            "nextcloud-cron.timer",
            "podman-immich_server.service",
            "podman-immich_ml.service",
            "caddy.service",
        ],
    )?;
    Ok(())
}

fn ssh(
    host: &str,
    port: &str,
    user: &str,
    command: &str,
    input: Option<&[u8]>,
    initrd: bool,
) -> Result<String> {
    let mut cmd = Command::new("timeout");
    cmd.args([
        "90s",
        "ssh",
        "-o",
        &format!("HostName={host}"),
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=3",
        "-o",
        "StrictHostKeyChecking=yes",
        "-p",
        port,
        "-l",
        user,
    ]);
    if initrd {
        let home = env::var("HOME")?;
        cmd.args([
            "-o",
            &format!("UserKnownHostsFile={home}/.config/predator-pm-study/initrd-known-hosts"),
        ]);
    }
    let mut child = cmd
        .arg(host)
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(data) = input {
        child
            .stdin
            .take()
            .ok_or("missing SSH input")?
            .write_all(data)?;
    } else {
        drop(child.stdin.take());
    }
    let result = child.wait_with_output()?;
    check(
        result.status.success(),
        format!(
            "SSH {host}:{port}: {}",
            String::from_utf8_lossy(&result.stderr)
        ),
    )?;
    Ok(String::from_utf8(result.stdout)?)
}

fn password() -> Result<Vec<u8>> {
    let result = Command::new("pass-cli")
        .args([
            "item",
            "view",
            "--vault-name",
            "Personal",
            "--item-title",
            "Predator LUKS",
            "--field",
            "password",
        ])
        .output()?;
    check(
        result.status.success(),
        "saved LUKS password unavailable; sign in to Proton Pass CLI",
    )?;
    let mut value = result.stdout;
    if value.ends_with(b"\n") {
        value.pop();
    }
    check(!value.is_empty(), "empty saved LUKS password")?;
    Ok(value)
}

pub fn telegram(message: &str) -> Result<()> {
    let token = env::var("TgToken")?;
    let chat = env::var("TgId")?;
    check(
        token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_')),
        "invalid Telegram token format",
    )?;
    let escaped = |s: &str| {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    };
    let mut curl = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--fail",
            "--max-time",
            "20",
            "--config",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let config = format!(
        "url = \"https://api.telegram.org/bot{token}/sendMessage\"\ndata-urlencode = \"chat_id={}\"\ndata-urlencode = \"text={}\"\n",
        escaped(&chat),
        escaped(message)
    );
    curl.stdin
        .take()
        .ok_or("missing curl input")?
        .write_all(config.as_bytes())?;
    let response = curl.wait_with_output()?;
    check(
        response.status.success(),
        format!(
            "Telegram delivery failed: {}",
            String::from_utf8_lossy(&response.stderr).replace(&token, "[redacted]")
        ),
    )?;
    let mut jq = Command::new("jq")
        .args(["--exit-status", ".ok == true"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    jq.stdin
        .take()
        .ok_or("missing Telegram response input")?
        .write_all(&response.stdout)?;
    check(jq.wait()?.success(), "Telegram rejected notification")?;
    println!("Telegram notification delivered");
    Ok(())
}

fn reachable(port: u16) -> bool {
    TcpStream::connect_timeout(
        &SocketAddr::from(([192, 168, 1, 19], port)),
        Duration::from_secs(1),
    )
    .is_ok()
}

fn pi_wake(boot: &str, wake: u64) -> Result<()> {
    uuid(boot)?;
    let tag = boot.replace('-', "");
    let mut script =
        String::from("set -e\nsudo -n systemctl is-active predator-netconsole-20261006.service\n");
    for (kind, epoch) in [
        ("wake", wake),
        ("wake-retry", wake + 30),
        ("return", wake + 90),
    ] {
        let unit = format!("predator-pm-auto-{kind}-{tag}");
        let when = output(
            "date",
            &[
                "-d",
                &format!("@{epoch}"),
                "+%Y-%m-%d %H:%M:%S Europe/Paris",
            ],
        )?;
        script += &format!(
            "if sudo -n systemctl show '{unit}.timer' -p LoadState --value | /usr/bin/grep -qx loaded; then sudo -n systemctl stop '{unit}.timer'; fi\n"
        );
        let program = if kind == "return" {
            "--property=TimeoutStartSec=20s /usr/bin/timeout 15s /usr/bin/ssh-keyscan -T 5 -t ed25519 192.168.1.19"
        } else {
            "/usr/bin/python3 /home/mirsella/wol_predator.py"
        };
        script += &format!(
            "sudo -n systemd-run --unit='{unit}' --on-calendar='{when}' --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=no --property=Type=oneshot {program}\nsudo -n systemctl show '{unit}.timer' -p SubState --value | /usr/bin/grep -qx waiting\n"
        );
    }
    let evidence = ssh(
        "192.168.1.166",
        "22",
        "mirsella",
        "bash -s",
        Some(script.as_bytes()),
        false,
    )?;
    println!("Pi wake timers verified: {evidence}");
    Ok(())
}

pub fn assist(request: &str) -> Result<()> {
    let data = text(request)?;
    let previous = field(&data, "previous_boot")?;
    uuid(previous)?;
    let runtime = env::current_exe()?;
    let runtime = runtime.to_str().ok_or("invalid runtime path")?;
    check(
        runtime.starts_with("/nix/store/")
            && runtime
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/' | '.')),
        "assistant must run the Nix-built runtime",
    )?;
    if reachable(2223) {
        let cmdline = ssh(
            "192.168.1.19",
            "2223",
            "root",
            "cat /proc/cmdline",
            None,
            true,
        )?;
        check(
            cmdline
                .split_whitespace()
                .any(|p| p == "initcall_blacklist=crb_acpi_driver_init"),
            "initrd is not TPM-free",
        )?;
        let secret = password()?;
        ssh(
            "192.168.1.19",
            "2223",
            "root",
            "predator-pm-study unlock-root",
            Some(&secret),
            true,
        )?;
        println!("Root password handed to the verified diagnostic initrd");
        return Ok(());
    }
    if !reachable(22) {
        return Ok(());
    }
    let boot = ssh(
        "192.168.1.19",
        "22",
        "mirsella",
        "cat /proc/sys/kernel/random/boot_id",
        None,
        false,
    )?;
    let boot = boot.trim();
    uuid(boot)?;
    if boot == previous {
        return Ok(());
    }
    let cmdline = ssh(
        "192.168.1.19",
        "22",
        "mirsella",
        "cat /proc/cmdline",
        None,
        false,
    )?;
    if !cmdline
        .split_whitespace()
        .any(|p| p == "initcall_blacklist=crb_acpi_driver_init")
    {
        let warning = Path::new(request).with_file_name(format!("{boot}-wrong-arm-notified.txt"));
        if !warning.exists() {
            telegram(
                "Predator test was not armed: the new boot has Linux TPM enabled. Select the pm-study boot entry for the pending TPM-off / EC-off test.",
            )?;
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(warning)?
                .write_all(boot.as_bytes())?;
        }
        return Ok(());
    }
    let secret = password()?;
    ssh(
        "192.168.1.19",
        "22",
        "mirsella",
        &format!("sudo -n {runtime} unlock-fast"),
        Some(&secret),
        false,
    )?;
    let response = ssh(
        "192.168.1.19",
        "22",
        "mirsella",
        &format!("sudo -n {runtime} auto-prepare {previous}"),
        None,
        false,
    )?;
    let status = field(&response, "STATUS")?;
    check(
        field(&response, "BOOT")? == boot,
        "boot changed during setup",
    )?;
    let due: u64 = field(&response, "DUE")?.parse()?;
    let wake: u64 = field(&response, "WAKE")?.parse()?;
    check(wake == due + 1800, "unexpected automatic wake time")?;
    if status == "ready" {
        pi_wake(boot, wake)?;
        ssh(
            "192.168.1.19",
            "22",
            "mirsella",
            &format!("sudo -n {runtime} auto-arm {boot}"),
            None,
            false,
        )?;
    } else {
        check(status == "armed", "unexpected automatic setup status")?;
    }
    let civil = |epoch| {
        output(
            "date",
            &["-d", &format!("@{epoch}"), "+%a %d %b %H:%M:%S %Z"],
        )
    };
    telegram(&format!(
        "Predator TPM-off / EC-off test is set up.\nSuspend: {}\nExpected wake: {}\nPi wake retry: +30 seconds; SSH check: +90 seconds.\nAt least 11 hours continuously awake, then 30 minutes of real deep sleep. RTC phase recording is ready. Keep this boot running and retain the power graph.\nBoot: {boot}",
        civil(due)?,
        civil(wake)?
    ))?;
    let completed = Path::new(request).with_file_name(format!("{boot}-setup-completed.txt"));
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(completed)?
        .write_all(format!("{response}\ntelegram=delivered\n").as_bytes())?;
    fs::remove_file(request)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_schedule_preserves_age_and_leaves_setup_time() {
        assert_eq!(schedule(1_000_000, 100).unwrap(), (1_039_510, 1_041_310));
        assert_eq!(schedule(1_000_000, 50_000).unwrap(), (1_000_120, 1_001_920));
        assert!(schedule(100, 101).is_err());
    }
    #[test]
    fn automatic_plan_requires_exact_arm_age_and_boot() {
        let boot = "0e17a3fe-84fd-41d1-9dd8-7431dc1a2d62";
        let plan = format!(
            "boot={boot}\ndue=100000\nwake=101800\ntpm=absent\nec=not-run\nmin_awake_seconds=39600\n"
        );
        assert!(validate_plan(&plan, boot).is_ok());
        assert!(validate_plan(&plan.replace("tpm=absent", "tpm=initialized"), boot).is_err());
        assert!(validate_plan(&plan.replace("39600", "21600"), boot).is_err());
        assert!(validate_plan(&plan, "wrong").is_err());
    }
}
