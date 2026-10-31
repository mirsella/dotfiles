//! One fresh-recovery-boot s2idle control. Separate from the immutable S3 runner.
//! A technical return never proves physical low-power duration or an S3 PASS.
use std::{
    collections::BTreeMap,
    env, fs,
    io::{BufRead, BufReader, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const STATE: &str = "/var/lib/predator-pm-s2idle";
const BOOT: &str = "ae29f675-9167-4975-a884-01885468bc38";
const JOURNAL_BOOT: &str = "ae29f67591674975a88401885468bc38";
const BTIME: u64 = 1793440484;
const SYSTEM: &str =
    "/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363";
const KERNEL: &str = "/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage";
const MODULES: &str = "/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules";
const RECORDER: &str = "/nix/store/3jcksrsfmyw8vk1cwp0x5ynb3h1q1pdp-predator-pm-stage-20261009/lib/modules/6.18.58/extra/pm_stage.ko.xz";
const HANDOFF: &str =
    "/nix/store/l423y5sbmkgcl50cjfkxwhqg1im7b31q-predator-pm-study-0.1.0/bin/predator-pm-study";
const RECOVERY: &str =
    "/var/lib/predator-pm-study/ae29f675-9167-4975-a884-01885468bc38-boot-rtc.txt";
const LAN: &str = "c4e6863b-7f76-4a4d-9a48-9937af0b5a36";
fn require(b: bool, why: impl Into<String>) -> Result<()> {
    if b { Ok(()) } else { Err(why.into().into()) }
}
fn text(p: impl AsRef<Path>) -> Result<String> {
    Ok(fs::read_to_string(p)?.trim().into())
}
fn now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
fn out(p: &str, a: &[&str]) -> Result<String> {
    let o = Command::new("timeout")
        .arg("150s")
        .arg(p)
        .args(a)
        .env("TZ", "Europe/Paris")
        .output()?;
    require(
        o.status.success(),
        format!(
            "{p} {a:?}: {} {}",
            o.status,
            String::from_utf8_lossy(&o.stderr)
        ),
    )?;
    Ok(String::from_utf8(o.stdout)?.trim().into())
}
fn hash(p: &str) -> Result<String> {
    Ok(out("sha256sum", &[p])?
        .split_whitespace()
        .next()
        .ok_or("missing hash")?
        .into())
}
fn fields(s: &str) -> Result<BTreeMap<String, String>> {
    let mut m = BTreeMap::new();
    for l in s.lines() {
        let (k, v) = l.split_once('=').ok_or("invalid field")?;
        require(
            !k.is_empty() && !v.is_empty() && m.insert(k.into(), v.into()).is_none(),
            "empty/duplicate field",
        )?;
    }
    Ok(m)
}
fn root_file(p: &str) -> Result<String> {
    let m = fs::symlink_metadata(p)?;
    require(
        m.is_file() && m.uid() == 0 && m.mode() & 0o777 == 0o600,
        "expected root regular0600",
    )?;
    text(p)
}
fn marker(s: &str) -> Result<()> {
    println!("{s}");
    writeln!(
        fs::OpenOptions::new().write(true).open("/dev/kmsg")?,
        "<6>pm-s2idle: {s}"
    )?;
    Ok(())
}
fn journal() -> Result<String> {
    out(
        "journalctl",
        &[
            "-k",
            "-b",
            JOURNAL_BOOT,
            "--no-pager",
            "-o",
            "short-monotonic",
        ],
    )
}
fn restoration_allowed(state: &str, pid: &str, jobs: &str) -> Result<()> {
    require(
        state == "inactive" && pid == "0" && !jobs.contains("sleep") && !jobs.contains("suspend"),
        "sleep active/queued; refusing restoration",
    )
}
fn sleep_inactive() -> Result<()> {
    restoration_allowed(
        &out(
            "systemctl",
            &[
                "show",
                "systemd-suspend.service",
                "-p",
                "ActiveState",
                "--value",
            ],
        )?,
        &out(
            "systemctl",
            &[
                "show",
                "systemd-suspend.service",
                "-p",
                "MainPID",
                "--value",
            ],
        )?,
        &out("systemctl", &["list-jobs", "--no-legend", "--plain"])?,
    )
}
fn quiet() -> Result<()> {
    let s = out(
        "systemctl",
        &[
            "list-units",
            "--no-pager",
            "--no-legend",
            "--plain",
            "--state=active,activating,reloading,deactivating",
            "db-backup.service",
            "recovery-backup.service",
            "hermes-backup.service",
            "restic-backup.service",
            "sanoid.service",
            "syncoid-*.service",
            "zfs-scrub*.service",
            "zpool-trim*.service",
            "nixos-upgrade.service",
            "nextcloud-setup.service",
            "nextcloud-cron.service",
        ],
    )?;
    require(s.is_empty(), format!("active maintenance: {s}"))?;
    require(
        Command::new("pgrep")
            .args(["-G", "nixbld"])
            .status()?
            .code()
            == Some(1),
        "builders active or inspection failed",
    )
}
// Same tested Linux x86_64 ABI and RTC writeback barrier as the immutable R3.
#[repr(C)]
#[derive(Default, Debug)]
struct Timex {
    modes: u32,
    offset: i64,
    freq: i64,
    maxerror: i64,
    esterror: i64,
    status: i32,
    constant: i64,
    precision: i64,
    tolerance: i64,
    time: [i64; 2],
    tick: i64,
    ppsfreq: i64,
    jitter: i64,
    shift: i32,
    stabil: i64,
    jitcnt: i64,
    calcnt: i64,
    errcnt: i64,
    stbcnt: i64,
    tai: i32,
    reserved: [i32; 11],
}
unsafe extern "C" {
    fn clock_adjtime(clock: i32, tx: *mut Timex) -> i32;
}
fn adjust(t: &mut Timex) -> Result<()> {
    require(
        cfg!(all(target_arch = "x86_64", target_os = "linux"))
            && std::mem::size_of::<Timex>() == 208,
        "unsupported timex ABI",
    )?;
    require(
        unsafe { clock_adjtime(0, t) } >= 0,
        format!("clock_adjtime: {}", std::io::Error::last_os_error()),
    )
}
fn clock_status() -> Result<Timex> {
    let mut t = Timex::default();
    adjust(&mut t)?;
    Ok(t)
}
fn quiesce_clock() -> Result<String> {
    out("systemctl", &["stop", "systemd-timesyncd.service"])?;
    let before = clock_status()?;
    let mut t = Timex {
        modes: 0x0010 | 0x0100 | 0x2000,
        status: before.status | 0x0040,
        ..Timex::default()
    };
    adjust(&mut t)?;
    let after = clock_status()?;
    require(after.status & 0x40 != 0, "RTC writer remained synchronized")?;
    require(
        out(
            "systemctl",
            &[
                "show",
                "systemd-timesyncd.service",
                "-p",
                "ActiveState",
                "--value",
            ],
        )? == "inactive",
        "timesyncd active",
    )?;
    Ok(format!(
        "before={before:?}\nafter={after:?}\nzero_offset_cancel=true\nlate_marker_barrier=freeze_kernel_threads_freezable_workqueue\n"
    ))
}

// Empty MemorySleepMode in systemd262 leaves the explicitly selected sysfs mode.
// Pin the entire effective configuration rather than guessing around overrides.
fn validate_sleep_config(s: &str) -> Result<()> {
    let lines: Vec<_> = s
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with(';'))
        .collect();
    require(
        lines == ["[Sleep]", "AllowHibernation=false", "AllowSuspend=true"],
        "sleep configuration changed; s2idle selection not proven",
    )
}
fn sleep_config() -> Result<String> {
    let s = out("systemd-analyze", &["cat-config", "systemd/sleep.conf"])?;
    validate_sleep_config(&s)?;
    require(
        out(
            "systemctl",
            &[
                "show",
                "systemd-suspend.service",
                "-p",
                "DropInPaths",
                "--value",
            ],
        )?
        .is_empty(),
        "suspend unit override",
    )?;
    require(
        fs::canonicalize("/etc/systemd/system/systemd-suspend.service")?
            == Path::new(
                "/nix/store/6qqq5prql4952z0wfm5ymjfr7cx4ab7z-systemd-262/example/systemd/system/systemd-suspend.service",
            ),
        "suspend unit changed",
    )?;
    Ok(s)
}
struct Plan {
    m: BTreeMap<String, String>,
    due: u64,
    wake: u64,
    btime: u64,
}
impl Plan {
    fn get(&self, k: &str) -> &str {
        &self.m[k]
    }
    fn parse(s: &str) -> Result<Self> {
        let m = fields(s)?;
        let keys = [
            "schema",
            "id",
            "boot",
            "runtime",
            "runtime_sha",
            "handoff_sha",
            "source_sha",
            "due",
            "wake",
            "window_end",
            "btime",
            "historical_btime",
            "success_before",
            "markers_before",
            "kind",
            "recovery_sha",
            "pm_async",
            "pm_trace",
            "pm_test",
            "mem_sleep",
            "recorder",
            "tpm",
            "ec",
            "measurement",
            "reset_provenance",
            "nonce",
        ];
        require(
            m.len() == keys.len() && keys.iter().all(|k| m.contains_key(*k)),
            "unknown/missing manifest field",
        )?;
        for (k, v) in [
            ("schema", "rtc-s2idle-control-1"),
            ("boot", BOOT),
            ("historical_btime", "1793440484"),
            ("success_before", "0"),
            ("markers_before", "0"),
            ("kind", "s2idle-control"),
            ("pm_async", "0"),
            ("pm_trace", "0"),
            ("pm_test", "none"),
            ("mem_sleep", "s2idle"),
            ("recorder", "rtc-unsync-zerooffset"),
            ("tpm", "initialized"),
            ("ec", "not-run"),
            ("measurement", "dated-power-graph-required"),
            (
                "reset_provenance",
                "owner-forced-reboot-method-off-duration-unspecified",
            ),
            (
                "recovery_sha",
                "b752bc4051bce5ed70ae71de0360ac566b078e8609dc0a6e8edd8e0b5f807f55",
            ),
        ] {
            require(m[k] == v, format!("unsupported {k}"))?;
        }
        for k in ["id", "nonce"] {
            require(
                !m[k].is_empty()
                    && m[k].len() <= 80
                    && m[k]
                        .bytes()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'),
                "bad id/nonce",
            )?;
        }
        for k in ["runtime_sha", "source_sha", "handoff_sha", "recovery_sha"] {
            require(
                m[k].len() == 64 && m[k].bytes().all(|c| c.is_ascii_hexdigit()),
                "bad digest",
            )?;
        }
        require(
            m["runtime"].starts_with("/nix/store/")
                && m["runtime"].ends_with("/bin/predator-pm-s2idle"),
            "runtime not immutable",
        )?;
        let due = m["due"].parse::<u64>()?;
        let wake = m["wake"].parse::<u64>()?;
        let btime = m["btime"].parse::<u64>()?;
        require(
            btime >= BTIME
                && btime <= BTIME + 2
                && due > btime
                && due >= 1793430000
                && m["window_end"] == "1793484000"
                && due.checked_add(960) == Some(wake)
                && wake.checked_add(660).is_some_and(|t| t < 1793484000),
            "dated window/btime/wake invalid",
        )?;
        Ok(Self {
            m,
            due,
            wake,
            btime,
        })
    }
    fn save(&self, suffix: &str, s: &str) -> Result<()> {
        fs::create_dir_all(STATE)?;
        let mut f = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(format!("{STATE}/{}-{suffix}.txt", self.get("id")))?;
        f.write_all(s.as_bytes())?;
        f.sync_all()?;
        fs::File::open(STATE)?.sync_all()?;
        Ok(())
    }
    fn exists(&self, suffix: &str) -> bool {
        Path::new(&format!("{STATE}/{}-{suffix}.txt", self.get("id"))).exists()
    }
    fn identity(&self) -> Result<()> {
        require(
            text("/proc/sys/kernel/random/boot_id")? == BOOT,
            "boot changed",
        )?;
        for (p, v) in [
            ("/run/current-system", SYSTEM),
            ("/run/booted-system", SYSTEM),
            ("/run/current-system/kernel", KERNEL),
            ("/run/current-system/kernel-modules", MODULES),
            ("/run/booted-system/kernel", KERNEL),
            ("/run/booted-system/kernel-modules", MODULES),
        ] {
            require(
                fs::canonicalize(p)? == Path::new(v),
                format!("frozen identity changed: {p}"),
            )?;
        }
        require(
            env::current_exe()? == Path::new(self.get("runtime"))
                && hash(self.get("runtime"))? == self.get("runtime_sha"),
            "runtime changed",
        )?;
        require(hash(HANDOFF)? == self.get("handoff_sha"), "handoff changed")?;
        require(
            hash(RECORDER)? == "8fd79959fa979cabf2dfac97c4a60dbdab505cca5cf3e3902772faebd3071d9e",
            "recorder changed",
        )?;
        require(
            hash(RECOVERY)? == self.get("recovery_sha")
                && root_file(RECOVERY)?.starts_with(&format!("boot={BOOT}\n")),
            "recovery evidence changed",
        )?;
        for (p, h) in [
            (
                "/var/lib/predator-pm-study/last-completed.txt",
                "280573317fc03a7338ca233c367593ff2fac0163eb8cfe70fabc375f4648fd08",
            ),
            (
                "/var/lib/predator-pm-diagnostic/d-core-03-20261030-completed.txt",
                "7851bcb034541c585c6385dcedd970a0e1f2fa2f3c6b0c0d93d4f1c241370ddc",
            ),
            (
                "/var/lib/predator-pm-instrumented/rtc-cal-core-r3-20261031-completed.txt",
                "2333c0af7c6fa351a53ad5202189df7b469963590245485cb386ae7f6b6227ed",
            ),
            (
                "/var/lib/predator-pm-instrumented/rtc-s3-20261031-plan.conf",
                "aa5be693e84beca952edddf18a7594169c3b08e3885bf61caaf4ca68d1ba5fa9",
            ),
        ] {
            require(hash(p)? == h, format!("historical anchor changed: {p}"))?;
        }
        Ok(())
    }
    fn timing(&self, execute: bool) -> Result<()> {
        let n = now()?;
        require(
            n < 1793484000
                && if execute {
                    (self.due..=self.due + 30).contains(&n)
                } else {
                    n.checked_add(300).is_some_and(|x| x < self.due)
                },
            "arming/execution deadline",
        )?;
        require(out("date", &["+%F"])? == "2026-10-31", "wrong date")
    }
}
fn fresh_history(s: &str) -> Result<()> {
    for x in [
        "PM: suspend entry (",
        "PM: suspend exit",
        "Low-level resume complete",
        "PM: suspend debug:",
        "PM: suspend-to-idle",
        "RTC BEGIN ",
        "S2IDLE BEGIN ",
        "DIAG BEGIN ",
    ] {
        require(!s.contains(x), format!("unexpected prior cycle: {x}"))?;
    }
    Ok(())
}
fn health(p: &Plan, after: bool) -> Result<()> {
    p.identity()?;
    sleep_inactive()?;
    sleep_config()?;
    for dir in [
        "predator-pm-study",
        "predator-pm-diagnostic",
        "predator-pm-instrumented",
        "predator-pm-s2idle",
    ] {
        require(
            !Path::new(&format!("/var/lib/{dir}/HOLD")).exists(),
            "HOLD present",
        )?;
    }
    for (path, v) in [
        ("/sys/power/pm_async", "1"),
        ("/sys/power/pm_trace", "0"),
        ("/sys/module/pm_stage/parameters/armed", "N"),
        ("/sys/power/suspend_stats/fail", "0"),
        ("/sys/power/pm_debug_messages", "1"),
        ("/sys/power/pm_print_times", "1"),
        ("/sys/module/kernel/parameters/initcall_debug", "Y"),
        ("/sys/class/power_supply/ACAD/online", "1"),
        ("/sys/class/graphics/fb0/blank", "4"),
        ("/sys/module/suspend/parameters/pm_test_delay", "5"),
        ("/sys/bus/pci/devices/0000:03:00.1/power/wakeup", "enabled"),
    ] {
        require(text(path)? == v, format!("unexpected {path}"))?;
    }
    require(
        text("/sys/power/suspend_stats/success")?.parse::<u64>()? == u64::from(after),
        "counter mismatch",
    )?;
    require(
        text("/sys/power/pm_test")?.starts_with("[none]")
            && text("/sys/power/mem_sleep")? == "s2idle [deep]",
        "ordinary PM modes changed",
    )?;
    require(
        matches!(text("/sys/class/rtc/rtc0/wakealarm")?.as_str(), "" | "0"),
        "RTC wake alarm present",
    )?;
    require(
        text("/proc/acpi/button/lid/LID0/state")?.ends_with("closed")
            && out("ethtool", &["enp3s0f1"])?
                .lines()
                .any(|l| l.trim() == "Wake-on: g"),
        "lid/WoL changed",
    )?;
    require(
        Path::new("/dev/tpm0").exists()
            && Path::new("/dev/tpmrm0").exists()
            && fs::canonicalize("/sys/bus/acpi/devices/MSFT0101:00/driver")?
                == Path::new("/sys/bus/acpi/drivers/tpm_crb")
            && !text("/proc/cmdline")?.contains("crb_acpi_driver_init"),
        "TPM baseline changed",
    )?;
    for n in ["acer_wmi", "acer_wmi_ph31751"] {
        require(
            !Path::new(&format!("/sys/module/{n}")).exists(),
            "Acer WMI loaded",
        )?;
    }
    let ec = out(
        "journalctl",
        &[
            "-b",
            JOURNAL_BOOT,
            "-u",
            "predator-kbd-backlight-off.service",
            "--no-pager",
            "-o",
            "cat",
        ],
    )?;
    require(
        ec.is_empty()
            || ec == "-- No entries --"
            || (ec.contains("skipped") && !ec.contains("Starting")),
        "raw EC ran",
    )?;
    let audit = out("predator-boot", &["audit"])?;
    for disk in ["root", "fast", "tank1", "tank2", "tank3"] {
        require(
            audit.lines().any(|l| {
                l.starts_with(&format!("{disk}: boot used authenticated slot "))
                    && l.contains("late-phase token 0 rejected")
            }),
            format!("missing authenticated audit {disk}"),
        )?;
    }
    for u in ["night-suspend.timer", "predator-pm-next.timer"] {
        require(
            out("systemctl", &["show", u, "-p", "ActiveState", "--value"])? == "inactive",
            format!("{u} active"),
        )?;
    }
    require(
        out(
            "nmcli",
            &["-g", "GENERAL.CON-UUID", "device", "show", "enp3s0f1"],
        )? == LAN
            && out("nmcli", &["-g", "IP4.DNS", "device", "show", "enp3s0f1"])?
                .lines()
                .any(|l| l == "192.168.1.1"),
        "LAN/DNS changed",
    )?;
    require(
        out("timedatectl", &["show", "-p", "NTPSynchronized", "--value"])? == "yes"
            && clock_status()?.status & 0x40 == 0,
        "NTP unsynchronized",
    )?;
    require(
        out("zpool", &["list", "-H", "-o", "name"])? == "fast\ntank"
            && out("zpool", &["status", "-x"])? == "all pools are healthy"
            && !out("zpool", &["status"])?.contains("in progress"),
        "pools/maintenance",
    )?;
    for (m, d) in [
        ("/srv/data/fast", "fast/data"),
        ("/var/lib/nextcloud/data", "fast/ncdata"),
        ("/srv/backup", "tank/backup"),
        ("/srv/backup/recovery", "tank/backup/recovery"),
        ("/srv/data/archive", "tank/archive"),
    ] {
        require(
            out("findmnt", &["-n", "-o", "SOURCE", "-M", m])? == d,
            format!("mount {m}"),
        )?;
    }
    for u in [
        "nginx",
        "postgresql",
        "redis-nextcloud",
        "phpfpm-nextcloud",
        "podman.socket",
        "podman-immich_server",
        "podman-immich_postgres",
        "podman-immich_redis",
        "podman-immich_ml",
        "hermes-agent",
        "hermes-backend",
        "systemd-timesyncd",
    ] {
        require(
            out("systemctl", &["is-active", u])? == "active",
            format!("inactive {u}"),
        )?;
    }
    let c = out("podman", &["ps", "--format", "{{.Names}} {{.Status}}"])?;
    for n in [
        "immich_server",
        "immich_postgres",
        "immich_redis",
        "immich_ml",
    ] {
        require(
            c.lines()
                .any(|l| l.starts_with(&format!("{n} ")) && l.contains("(healthy)")),
            format!("unhealthy {n}"),
        )?;
    }
    require(
        out(
            "systemctl",
            &["list-units", "--failed", "--no-legend", "--plain"],
        )?
        .is_empty(),
        "failed units",
    )?;
    require(
        out("systemctl", &["list-jobs", "--no-legend", "--plain"])?.is_empty(),
        "jobs active",
    )?;
    quiet()?;
    if !after {
        require(
            text("/sys/module/pm_stage/parameters/markers")? == "0",
            "marker count changed",
        )?;
        require(
            text("/proc/stat")?
                .lines()
                .find_map(|l| l.strip_prefix("btime "))
                .ok_or("missing btime")?
                .parse::<u64>()?
                == p.btime,
            "preflight btime changed",
        )?;
        fresh_history(&journal()?)?;
    }
    Ok(())
}
fn stamp(l: &str) -> Result<f64> {
    let n = l
        .strip_prefix('[')
        .and_then(|l| l.split_once(']'))
        .ok_or("missing monotonic timestamp")?
        .0
        .trim()
        .parse::<f64>()?;
    require(n.is_finite() && n >= 0.0, "invalid monotonic timestamp")?;
    Ok(n)
}
// s2idle does not execute ACPI S3 entry, CPU-offline or syscore/timekeeping suspend.
// Require its distinct loop/return messages, not a generic suspend exit alone.
fn cycle(s: &str) -> Result<f64> {
    let (mut phase, mut start, mut end) = (0, 0.0, 0.0);
    for l in s.lines() {
        for x in [
            "Low-level resume complete",
            "PM: suspend debug:",
            "Timekeeping suspended for",
            "BUG:",
            "Oops:",
            "Call Trace:",
        ] {
            require(!l.contains(x), format!("unexpected {x}"))?;
        }
        if l.contains("PM: suspend entry (") {
            require(phase == 0 && l.contains("(s2idle)"), "not one s2idle entry")?;
            phase = 1;
        } else if l.contains("PM: suspend-to-idle") {
            require(phase == 1, "unexpected s2idle loop")?;
            start = stamp(l)?;
            phase = 2;
        } else if l.contains("PM: resume from suspend-to-idle") {
            require(phase == 2, "unexpected s2idle return")?;
            end = stamp(l)?;
            require(end >= start, "backwards loop timestamps")?;
            phase = 3;
        } else if l.contains("PM: suspend exit") {
            require(phase == 3, "exit without s2idle loop return")?;
            phase = 4;
        }
    }
    require(
        phase == 4
            && s.contains("System returned from sleep operation")
            && (s.contains("Finished System Suspend.")
                || s.contains("Finished systemd-suspend.service - System Suspend.")),
        "incomplete s2idle/service return",
    )?;
    Ok(end - start)
}
fn receive(rx: &mpsc::Receiver<std::io::Result<String>>) -> (String, Result<f64>) {
    let deadline = Instant::now() + Duration::from_secs(2300);
    let mut s = String::new();
    let r = (|| loop {
        let left = deadline
            .checked_duration_since(Instant::now())
            .ok_or("return deadline")?;
        let l = rx.recv_timeout(left)??;
        s.push_str(&l);
        s.push('\n');
        if (s.contains("Finished System Suspend.")
            || s.contains("Finished systemd-suspend.service - System Suspend."))
            && s.contains("PM: suspend exit")
        {
            return cycle(&s);
        }
        require(
            !l.contains("Failed to suspend system")
                && !l.contains("Failed to start System Suspend"),
            "suspend failed",
        )?;
    })();
    (s, r)
}
fn raw_rtc() -> Result<String> {
    Ok(format!(
        "rtc_date={}\nrtc_time={}\nhwclock={}\nlast_action={}\nlast_value={}\nlast_start={}\nmarkers={}\nclock={:?}\nuptime={}\n",
        text("/sys/class/rtc/rtc0/date")?,
        text("/sys/class/rtc/rtc0/time")?,
        out("hwclock", &["--show", "--utc"])?,
        text("/sys/module/pm_stage/parameters/last_action")?,
        text("/sys/module/pm_stage/parameters/last_value")?,
        text("/sys/module/pm_stage/parameters/last_start")?,
        text("/sys/module/pm_stage/parameters/markers")?,
        clock_status()?,
        text("/proc/uptime")?
    ))
}
fn validate_rtc(s: &str) -> Result<u64> {
    let f = fields(s)?;
    for (k, v) in [
        ("rtc_date", "2021-04-10"),
        ("last_action", "17"),
        ("last_value", "1"),
        ("last_start", "N"),
    ] {
        require(
            f.get(k).map(String::as_str) == Some(v),
            format!("unexpected retained {k}"),
        )?;
    }
    require(
        f.get("rtc_time").is_some_and(|s| s.starts_with("00:00:")),
        "unexpected retained RTC time",
    )?;
    let n = f.get("markers").ok_or("no markers")?.parse::<u64>()?;
    require(n > 0, "no new markers")?;
    Ok(n)
}
fn restore(p: &Plan) -> Result<()> {
    p.identity()?;
    sleep_inactive()?;
    if !p.exists("mutating") || p.exists("restored") {
        return Ok(());
    }
    if !p.exists("raw-return") {
        p.save("raw-return", &raw_rtc()?)?;
    }
    require(
        text("/sys/power/pm_test")?.starts_with("[none]"),
        "unexpected cleanup pm_test",
    )?;
    require(
        matches!(
            text("/sys/power/mem_sleep")?.as_str(),
            "s2idle [deep]" | "[s2idle] deep"
        ),
        "unexpected cleanup mem_sleep",
    )?;
    fs::write("/sys/module/pm_stage/parameters/armed", "N")?;
    fs::write("/sys/power/mem_sleep", "deep")?;
    fs::write("/sys/power/pm_async", "1")?;
    require(
        text("/sys/power/mem_sleep")? == "s2idle [deep]" && text("/sys/power/pm_trace")? == "0",
        "ordinary modes not restored",
    )?;
    let log = out(HANDOFF, &["clock-handoff"])?;
    p.save("clock-handoff", &log)?;
    require(
        clock_status()?.status & 0x40 == 0,
        "clock restore still UNSYNC",
    )?;
    p.save(
        "restored",
        &format!(
            "epoch={}\nrtc={}\nclock={:?}\nuptime={}\nbtime_observed={}\nmem_sleep={}\n",
            now()?,
            out("hwclock", &["--show", "--utc"])?,
            clock_status()?,
            text("/proc/uptime")?,
            text("/proc/stat")?
                .lines()
                .find(|l| l.starts_with("btime "))
                .ok_or("missing btime")?,
            text("/sys/power/mem_sleep")?
        ),
    )
}
fn return_class(observed: u64, wake: u64) -> &'static str {
    if observed < wake {
        "S2IDLE_EARLY_RETURN"
    } else if observed > wake + 180 {
        "S2IDLE_LATE_RETURN"
    } else {
        "S2IDLE_RETURN_PENDING_POWER_PROOF"
    }
}
fn run(p: &Plan, raw: &str) -> Result<()> {
    p.save("requested", raw)?;
    p.timing(true)?;
    health(p, false)?;
    p.save("before-kernel", &journal()?)?;
    p.save("sleep-config", &sleep_config()?)?;
    p.save(
        "before",
        &format!(
            "epoch={}\nuptime={}\nclock={:?}\n{}\n",
            now()?,
            text("/proc/uptime")?,
            clock_status()?,
            out("timedatectl", &["timesync-status"])?
        ),
    )?;
    let cursor = out(
        "journalctl",
        &["-b", JOURNAL_BOOT, "-n", "0", "--show-cursor", "--no-pager"],
    )?;
    let cursor = cursor
        .lines()
        .find_map(|l| l.strip_prefix("-- cursor: "))
        .ok_or("no cursor")?;
    p.save("cursor", cursor)?;
    let mut child = Command::new("journalctl")
        .args([
            "-b",
            JOURNAL_BOOT,
            "--after-cursor",
            cursor,
            "-f",
            "--lines=all",
            "--no-pager",
            "-o",
            "short-monotonic",
        ])
        .stdout(Stdio::piped())
        .spawn()?;
    let pipe = child.stdout.take().ok_or("no journal pipe")?;
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for l in BufReader::new(pipe).lines() {
            if tx.send(l).is_err() {
                break;
            }
        }
    });
    let mut captured = String::new();
    let op = (|| {
        p.save("mutating", "clock-and-s2idle-mutations-starting\n")?;
        p.save("clock-quiescence", &quiesce_clock()?)?;
        fs::write("/sys/power/pm_async", "0")?;
        fs::write("/sys/power/mem_sleep", "s2idle")?;
        require(
            text("/sys/power/pm_test")?.starts_with("[none]")
                && text("/sys/power/mem_sleep")? == "[s2idle] deep"
                && text("/sys/power/pm_async")? == "0",
            "s2idle selection failed",
        )?;
        sleep_config()?;
        quiet()?;
        p.identity()?;
        p.timing(true)?;
        sleep_inactive()?;
        marker(&format!(
            "S2IDLE BEGIN id={} boot={BOOT} kind=s2idle-control success_before=0 markers_before=0 nonce={} planned_wake={} age_since_recovery_boot={} duration_source=external-power-graph",
            p.get("id"),
            p.get("nonce"),
            p.wake,
            now()? - BTIME
        ))?;
        out("journalctl", &["--sync"])?;
        require(
            clock_status()?.status & 0x40 != 0,
            "RTC writeback reenabled",
        )?;
        fs::write("/sys/module/pm_stage/parameters/armed", "Y")?;
        require(
            text("/sys/module/pm_stage/parameters/armed")? == "Y"
                && text("/sys/power/mem_sleep")? == "[s2idle] deep",
            "recorder/mode readback failed",
        )?;
        p.save(
            "request-call",
            "SuspendWithFlags(1); inhibitors enforced; mem_sleep=s2idle\n",
        )?;
        out(
            "busctl",
            &[
                "--timeout=30s",
                "call",
                "org.freedesktop.login1",
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
                "SuspendWithFlags",
                "t",
                "1",
            ],
        )?;
        p.save("request-ok", "true\n")?;
        let (s, r) = receive(&rx);
        captured = s;
        r
    })();
    let observed = now()?;
    let stop = child.kill();
    let reap = child.wait();
    let join = reader.join();
    // Attempt safe restoration even when one evidence write fails (ExecStopPost also retries).
    let evidence = (|| {
        p.save("observed-journal", &captured)?;
        p.save("return", &format!("operation={op:?}\nreader_stop={stop:?}\nreader_reap={reap:?}\nreader_joined={}\npre_restore_epoch={observed}\n",join.is_ok()))
    })();
    let cleanup = restore(p);
    p.save(
        "outcome",
        &format!("operation={op:?}\nevidence={evidence:?}\ncleanup={cleanup:?}\n"),
    )?;
    let elapsed = op?;
    evidence?;
    cleanup?;
    stop?;
    reap?;
    require(join.is_ok(), "reader panic")?;
    let markers = validate_rtc(&root_file(&format!(
        "{STATE}/{}-raw-return.txt",
        p.get("id")
    ))?)?;
    health(p, true)?;
    p.save("after-kernel", &journal()?)?;
    let class = return_class(observed, p.wake);
    p.save("completed", &format!("result={class}\nid={}\nboot={BOOT}\nruntime={}\nsource_sha={}\ncompleted_epoch={}\npre_restore_return_epoch={observed}\nmarkers_after={markers}\nsuccess_after=1\nreal_s3=false\ns2idle_loop_monotonic_seconds={elapsed:.6}\nphysical_sleep_seconds=pending-dated-power-graph\nfull_pass=false\n",p.get("id"),p.get("runtime"),p.get("source_sha"),now()?))?;
    marker(&format!(
        "S2IDLE RETURN id={} result={class} full_pass=false",
        p.get("id")
    ))
}
fn main() -> Result<()> {
    let a: Vec<String> = env::args().collect();
    if a.len() == 2 && a[1] == "clock-status" {
        println!("{:?}", clock_status()?);
        return Ok(());
    }
    require(
        a.len() == 4,
        "usage check|run|restore MANIFEST SHA256, or clock-status",
    )?;
    require(hash(&a[2])? == a[3], "manifest hash mismatch")?;
    let raw = root_file(&a[2])?;
    let p = Plan::parse(&raw)?;
    match a[1].as_str() {
        "check" => {
            p.timing(false)?;
            health(&p, false)?;
            marker(&format!(
                "S2IDLE READY id={} nonce={}",
                p.get("id"),
                p.get("nonce")
            ))
        }
        "run" => run(&p, &raw),
        "restore" => restore(&p),
        _ => Err("unknown command".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> String {
        format!(
            "schema=rtc-s2idle-control-1\nid=s2idle-20261031\nboot={BOOT}\nruntime=/nix/store/test/bin/predator-pm-s2idle\nruntime_sha={}\nsource_sha={}\nhandoff_sha={}\ndue=1793444400\nwake=1793445360\nwindow_end=1793484000\nbtime=1793440484\nhistorical_btime=1793440484\nsuccess_before=0\nmarkers_before=0\nkind=s2idle-control\nrecovery_sha=b752bc4051bce5ed70ae71de0360ac566b078e8609dc0a6e8edd8e0b5f807f55\npm_async=0\npm_trace=0\npm_test=none\nmem_sleep=s2idle\nrecorder=rtc-unsync-zerooffset\ntpm=initialized\nec=not-run\nmeasurement=dated-power-graph-required\nreset_provenance=owner-forced-reboot-method-off-duration-unspecified\nnonce=s2idle-nonce\n",
            "a".repeat(64),
            "b".repeat(64),
            "c".repeat(64)
        )
    }
    #[test]
    fn strict_new_boot_protocol() {
        let s = manifest();
        Plan::parse(&s).unwrap();
        for bad in [
            s.replace("mem_sleep=s2idle", "mem_sleep=deep"),
            s.replace("success_before=0", "success_before=10"),
            s.replace("markers_before=0", "markers_before=56"),
            s.replace("kind=s2idle-control", "kind=s3"),
            s.replace("pm_test=none", "pm_test=core"),
            s.replace("wake=1793445360", "wake=1793445300"),
            s.replace("btime=1793440484", "btime=1793381192"),
            s.replace(BOOT, "f8eb6e44-85ae-4e56-bc00-587876699945"),
            s.clone() + "schema=rtc-s2idle-control-1\n",
            s.replace("dated-power-graph-required", "scheduled-delay"),
        ] {
            assert!(Plan::parse(&bad).is_err());
        }
    }
    #[test]
    fn overrides_cannot_silently_select_deep() {
        let s = "# /etc/systemd/sleep.conf\n[Sleep]\nAllowHibernation=false\nAllowSuspend=true\n";
        validate_sleep_config(s).unwrap();
        for extra in [
            "MemorySleepMode=deep",
            "SuspendState=standby",
            "AllowSuspend=false",
            "[Sleep]\nMemorySleepMode=s2idle deep",
        ] {
            assert!(validate_sleep_config(&format!("{s}{extra}\n")).is_err());
        }
    }
    const CYCLE: &str = "[ 100.0] kernel: PM: suspend entry (s2idle)\n[ 101.0] kernel: PM: suspend-to-idle\n[ 1051.0] kernel: PM: resume from suspend-to-idle\n[ 1052.0] kernel: PM: suspend exit\nSystem returned from sleep operation\nFinished System Suspend.\n";
    #[test]
    fn requires_s2idle_loop_and_ordered_return() {
        assert_eq!(cycle(CYCLE).unwrap(), 950.0);
        for bad in [
            CYCLE.replace("(s2idle)", "(deep)"),
            CYCLE.replace("[ 101.0] kernel: PM: suspend-to-idle\n", ""),
            CYCLE.replace("[ 1051.0] kernel: PM: resume from suspend-to-idle\n", ""),
            format!("{CYCLE}PM: suspend entry (s2idle)\n"),
            format!("{CYCLE}Low-level resume complete\n"),
            format!("{CYCLE}PM: suspend debug: Waiting for 5 second(s).\n"),
            CYCLE.replace("1051.0", "99.0"),
            CYCLE.replace("101.0", "NaN"),
        ] {
            assert!(cycle(&bad).is_err());
        }
    }
    #[test]
    fn delayed_kernel_exit_is_not_a_false_failure() {
        let (tx, rx) = mpsc::channel();
        for l in CYCLE.lines().filter(|l| !l.contains("suspend exit")) {
            tx.send(Ok(l.into())).unwrap();
        }
        tx.send(Ok("[ 1052.0] kernel: PM: suspend exit".into()))
            .unwrap();
        assert_eq!(receive(&rx).1.unwrap(), 950.0);
    }
    #[test]
    fn rtc_marker_is_s2idle_not_s3() {
        let s = "rtc_date=2021-04-10\nrtc_time=00:00:02\nlast_action=17\nlast_value=1\nlast_start=N\nmarkers=44";
        assert_eq!(validate_rtc(s).unwrap(), 44);
        for bad in [
            s.replace("2021", "2023"),
            s.replace("last_value=1", "last_value=3"),
            s.replace("markers=44", "markers=0"),
            s.replace("last_start=N", "last_start=Y"),
            s.replace("last_action=17", "last_action=9"),
        ] {
            assert!(validate_rtc(&bad).is_err());
        }
    }
    #[test]
    fn fresh_history_never_relabels_previous_cycles() {
        fresh_history("normal boot\nPM: RTC time: 00:59:40, date: 2099-07-02").unwrap();
        for s in [
            CYCLE,
            "PM: suspend entry (deep)",
            "RTC BEGIN old",
            "DIAG BEGIN old",
            "S2IDLE BEGIN duplicate",
        ] {
            assert!(fresh_history(s).is_err());
        }
    }
    #[test]
    fn active_or_queued_sleep_prevents_cleanup() {
        restoration_allowed("inactive", "0", "").unwrap();
        for (s, p, j) in [
            ("active", "42", ""),
            ("activating", "0", ""),
            ("inactive", "0", "17 suspend.target start waiting"),
            ("inactive", "1", ""),
            ("failed", "0", ""),
        ] {
            assert!(restoration_allowed(s, p, j).is_err());
        }
    }
    #[test]
    fn early_and_late_return_are_never_planned_interval_success() {
        assert_eq!(return_class(100, 1060), "S2IDLE_EARLY_RETURN");
        assert_eq!(
            return_class(1060, 1060),
            "S2IDLE_RETURN_PENDING_POWER_PROOF"
        );
        assert_eq!(return_class(1241, 1060), "S2IDLE_LATE_RETURN");
    }
    #[test]
    fn timex_abi() {
        assert_eq!(std::mem::size_of::<Timex>(), 208);
        assert_eq!(std::mem::offset_of!(Timex, status), 40);
        assert_eq!(std::mem::offset_of!(Timex, time), 72);
        assert_eq!(std::mem::offset_of!(Timex, tai), 160);
    }
}
