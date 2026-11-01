use std::{
    error::Error,
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const BOOT: &str = "bf51c3ac-164e-4bfa-b56c-7b71df526b7f";
const SYSTEM: &str =
    "/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363";
const MODULE: &str = "/nix/store/3dzvzp3hg82n7gkrmrbac2svplh4xlzb-predator-pm-boundary-compact-20261101/lib/modules/6.18.58/extra/pm_boundary_compact.ko.xz";
const HASH: &str = "39650a6e0654a280a775e3a75ad982539a4ebc60ba184389aad1f4e704be3e00";
const ROOT: &str = "/var/lib/predator-pm-boundary/compact-20261101-a";
const PARAM: &str = "/sys/module/pm_boundary_compact/parameters";
fn text(path: impl AsRef<Path>) -> Result<String> {
    let p = path.as_ref();
    Ok(fs::read_to_string(p)
        .map_err(|e| format!("{}: {e}", p.display()))?
        .trim()
        .into())
}
fn out(program: &str, args: &[&str]) -> Result<String> {
    let v = Command::new("timeout")
        .arg("20s")
        .arg(program)
        .args(args)
        .env("TZ", "Europe/Paris")
        .output()?;
    if !v.status.success() {
        return Err(format!(
            "{program} {args:?}: {} {}",
            v.status,
            String::from_utf8_lossy(&v.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(v.stdout)?.trim().into())
}
fn save(name: &str, data: impl AsRef<[u8]>) -> Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(Path::new(ROOT).join(name))?;
    f.write_all(data.as_ref())?;
    f.sync_all()?;
    Ok(())
}
fn eq(actual: impl AsRef<str>, expected: &str, label: &str) -> Result<()> {
    if actual.as_ref() != expected {
        return Err(format!("{label}: expected {expected:?}, got {:?}", actual.as_ref()).into());
    }
    Ok(())
}
fn guard() -> Result<()> {
    let epoch = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    if !(1793516400..1793570400).contains(&epoch) {
        return Err("outside November1 08:00-23:00 CET".into());
    }
    eq(out("id", &["-u"])?, "0", "root")?;
    eq(text("/proc/sys/kernel/random/boot_id")?, BOOT, "boot")?;
    if !text("/proc/stat")?.lines().any(|l| l == "btime 1793530757") {
        return Err("changed btime".into());
    }
    for (path, expected) in [
        ("/run/current-system", SYSTEM),
        ("/run/booted-system", SYSTEM),
        (
            "/run/current-system/kernel",
            "/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage",
        ),
        (
            "/run/current-system/kernel-modules",
            "/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules",
        ),
    ] {
        if fs::canonicalize(path)? != PathBuf::from(expected) {
            return Err(format!("changed {path}").into());
        }
    }
    eq(
        out("systemctl", &["is-system-running"])?,
        "running",
        "system state",
    )?;
    eq(
        out(
            "systemctl",
            &[
                "show",
                "systemd-suspend.service",
                "-p",
                "ActiveState",
                "--value",
            ],
        )?,
        "inactive",
        "sleep state",
    )?;
    eq(
        out(
            "systemctl",
            &[
                "show",
                "systemd-suspend.service",
                "-p",
                "MainPID",
                "--value",
            ],
        )?,
        "0",
        "sleep PID",
    )?;
    eq(out("systemctl", &["list-jobs", "--no-legend"])?, "", "jobs")?;
    for (path, value) in [
        ("/sys/power/suspend_stats/success", "0"),
        ("/sys/power/suspend_stats/fail", "0"),
        ("/sys/module/pm_stage/parameters/armed", "N"),
        ("/sys/module/pm_stage/parameters/markers", "0"),
        ("/sys/power/pm_trace", "0"),
        ("/sys/power/pm_async", "1"),
        ("/sys/power/mem_sleep", "s2idle [deep]"),
        ("/sys/class/rtc/rtc0/wakealarm", ""),
    ] {
        eq(text(path)?, value, path)?;
    }
    if text("/proc/cmdline")?.contains("efi_no_storage_paranoia") {
        return Err("EFI reserve override present".into());
    }
    for unit in [
        "zfs-scrub.service",
        "restic-backups-predator.service",
        "nextcloud-cron.service",
        "nixos-upgrade.service",
    ] {
        let state = out("systemctl", &["show", unit, "-p", "ActiveState", "--value"])?;
        if state == "active" || state == "activating" {
            return Err(format!("conflicting unit {unit}: {state}").into());
        }
    }
    eq(
        out("timedatectl", &["show", "-p", "NTPSynchronized", "--value"])?,
        "yes",
        "NTP",
    )?;
    let active = out(
        "systemctl",
        &[
            "list-units",
            "--state=running,activating",
            "--no-legend",
            "--no-pager",
        ],
    )?;
    if [
        "restic",
        "sanoid",
        "syncoid",
        "scrub",
        "upgrade",
        "fstrim",
        "zfs-trim",
        "nextcloud-setup",
    ]
    .iter()
    .any(|name| active.contains(name))
    {
        return Err(format!("maintenance active: {active}").into());
    }
    if out("ps", &["-eo", "user=,comm="])?
        .lines()
        .any(|line| line.trim_start().starts_with("nixbld"))
    {
        return Err("active Nix builder".into());
    }
    eq(
        out("sha256sum", &[MODULE])?
            .split_whitespace()
            .next()
            .ok_or("no hash")?,
        HASH,
        "module hash",
    )?;
    Ok(())
}
fn snapshot() -> Result<String> {
    Ok(format!(
        "date={}\nboot={}\ncmdline={}\ntaint={}\n{}\n{}\n{}\n",
        out("date", &["--iso-8601=ns"])?,
        text("/proc/sys/kernel/random/boot_id")?,
        text("/proc/cmdline")?,
        text("/proc/sys/kernel/tainted")?,
        out("df", &["-B1", "/sys/firmware/efi/efivars"])?,
        out("hwclock", &["--show", "--utc"])?,
        out(
            "timedatectl",
            &["show", "-p", "NTPSynchronized", "-p", "LocalRTC"]
        )?
    ))
}
fn run() -> Result<()> {
    guard()?;
    if Path::new("/sys/module/pm_boundary_probe").exists() || Path::new(PARAM).exists() {
        return Err("a probe module is already loaded".into());
    }
    fs::create_dir(ROOT)?;
    fs::set_permissions(ROOT, fs::Permissions::from_mode(0o700))?;
    save(
        "request.txt",
        format!(
            "authorization=owner requested compact breadcrumb and awake write/readback\nboot={BOOT}\nnonce=20261101-a\nmodule={MODULE}\nmodule_sha256={HASH}\nwrite_attempt_limit=1\nrecord_bytes=64\nallocation_budget_bytes=1024\nlinux_reserve_bytes=5120\nextra_margin_bytes=1024\nsuspend_requests=0\nreboot_requests=0\n"
        ),
    )?;
    save("before.txt", snapshot()?)?;
    let loaded = out(
        "insmod",
        &[MODULE, "nonce=20261101-a", &format!("boot_id={BOOT}")],
    );
    save("load-result.txt", format!("{loaded:?}\n"))?;
    loaded?;
    let attempt = (|| -> Result<()> {
        save(
            "status-before.txt",
            fs::read(Path::new(PARAM).join("status"))?,
        )?;
        guard()?;
        let write = fs::write(Path::new(PARAM).join("write_once"), b"1\n");
        save("write-result.txt", format!("{write:?}\n"))?;
        save(
            "status-after.txt",
            fs::read(Path::new(PARAM).join("status"))?,
        )?;
        let readback = fs::read(Path::new(PARAM).join("readback"));
        match &readback {
            Ok(data) => save("readback.txt", data)?,
            Err(e) => save("readback-error.txt", format!("{e:?}\n"))?,
        }
        save("after-write.txt", snapshot()?)?;
        write?;
        readback?;
        Ok(())
    })();
    let unload = (|| -> Result<()> {
        let epoch = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        if !(1793516400..1793570400).contains(&epoch) {
            return Err("cleanup outside date/window".into());
        }
        eq(
            text("/proc/sys/kernel/random/boot_id")?,
            BOOT,
            "cleanup boot",
        )?;
        eq(
            out(
                "systemctl",
                &[
                    "show",
                    "systemd-suspend.service",
                    "-p",
                    "ActiveState",
                    "--value",
                ],
            )?,
            "inactive",
            "cleanup sleep",
        )?;
        eq(
            out(
                "systemctl",
                &[
                    "show",
                    "systemd-suspend.service",
                    "-p",
                    "MainPID",
                    "--value",
                ],
            )?,
            "0",
            "cleanup sleep PID",
        )?;
        let jobs = out("systemctl", &["list-jobs", "--no-legend"])?;
        if jobs.contains("sleep") || jobs.contains("suspend") {
            return Err("queued sleep blocks cleanup".into());
        }
        out("rmmod", &["pm_boundary_compact"])?;
        if Path::new(PARAM).exists() {
            return Err("module remains loaded".into());
        }
        Ok(())
    })();
    save(
        "outcome.txt",
        format!("attempt={attempt:?}\nunload={unload:?}\n"),
    )?;
    save(
        "kernel.txt",
        out(
            "journalctl",
            &[
                "-b",
                &BOOT.replace('-', ""),
                "-k",
                "--since",
                "2026-11-01 12:27:00",
                "--no-pager",
                "-o",
                "short-iso-precise",
            ],
        )?,
    )?;
    save("after.txt", snapshot()?)?;
    attempt?;
    unload?;
    guard()?;
    println!(
        "compact_awake_write_readback_and_unload=PASS; originals={ROOT}; retention_and_suspended_context=UNPROVEN"
    );
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("compact operator stopped: {e}");
        std::process::exit(1);
    }
}
