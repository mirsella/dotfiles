//! November 1 first-slot wake-path experiment after the sealed October 31 cycle.
//! One control: deny NIC D3cold to keep its upstream bridge in D0. Log actual
//! PCI suspend states and PME/ACPI arming through existing, pinned debug sites.
//! RTC phase recording is OFF. Preserve kernel source, receipt and wall-clock domains.
use std::{
    collections::BTreeMap,
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    process::{Command, Stdio},
    sync::mpsc,
    time::{SystemTime, UNIX_EPOCH},
};
mod wake_timing;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const STATE: &str = "/var/lib/predator-pm-wake-path";
const D3COLD: &str = "/sys/bus/pci/devices/0000:03:00.1/d3cold_allowed";
const DEBUG: &str = "/sys/kernel/debug/dynamic_debug/control";
const SITES: [(&str, &str); 6] = [
    ("drivers/pci/pci.c:2513", "pci_pme_active"),
    ("drivers/pci/pci-driver.c:923", "pci_pm_suspend_noirq"),
    ("drivers/pci/pci-driver.c:938", "pci_pm_suspend_noirq"),
    ("drivers/acpi/device_pm.c:956", "acpi_pm_set_device_wakeup"),
    ("drivers/acpi/device_pm.c:965", "acpi_pm_set_device_wakeup"),
    ("drivers/acpi/device_pm.c:971", "acpi_pm_set_device_wakeup"),
];
const DAY_START: u64 = 1793516400;
const LAST_ENTRY: u64 = 1793523600;
const REVIEW: u64 = 1793525400;
const DAY_END: u64 = 1793570400;
const BOOT: &str = "f9b45aa6-b820-4ab5-b18c-9324d8d53087";
const JOURNAL_BOOT: &str = "f9b45aa6b8204ab5b18c9324d8d53087";
const BTIME: u64 = 1793474520;
const ALARM: &str = "/sys/class/rtc/rtc0/wakealarm";
const SYSTEM: &str =
    "/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363";
const KERNEL: &str = "/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage";
const MODULES: &str = "/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules";
const RECORDER: &str = "/nix/store/3jcksrsfmyw8vk1cwp0x5ynb3h1q1pdp-predator-pm-stage-20261009/lib/modules/6.18.58/extra/pm_stage.ko.xz";
const HANDOFF: &str =
    "/nix/store/qzs6qmgvvka19nv4dyihz9rc8mbybkkv-predator-pm-study-0.1.0/bin/predator-pm-study";
const RECOVERY: &str =
    "/var/lib/predator-pm-study/f9b45aa6-b820-4ab5-b18c-9324d8d53087-boot-rtc.txt";
const LAN: &str = "c4e6863b-7f76-4a4d-9a48-9937af0b5a36";
fn require(b: bool, why: impl Into<String>) -> Result<()> {
    if b {
        Ok(())
    } else {
        Err(why.into().into())
    }
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
        "<6>pm-wake-path: {s}"
    )?;
    Ok(())
}
fn journal() -> Result<String> {
    out(
        "journalctl",
        &["-k", "-b", JOURNAL_BOOT, "--no-pager", "-o", "json"],
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
// Read-only clock status. This protocol never sets UNSYNC or abuses the RTC.
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
fn clock_valid() -> Result<()> {
    let rtc = text("/sys/class/rtc/rtc0/since_epoch")?.parse::<u64>()?;
    require(
        rtc.abs_diff(now()?) <= 2,
        "RTC and UTC differ by more than two seconds",
    )?;
    require(
        text("/etc/adjtime")?.lines().last() == Some("UTC"),
        "RTC not configured UTC",
    )?;
    require(clock_status()?.status & 0x40 == 0, "kernel clock UNSYNC")?;
    require(
        text("/sys/module/pm_stage/parameters/armed")? == "N"
            && text("/sys/module/pm_stage/parameters/markers")? == "0"
            && text("/sys/power/pm_trace")? == "0",
        "RTC recorder or trace altered clock",
    )?;
    require(
        text("/sys/class/rtc/rtc0/device/power/wakeup")? == "enabled",
        "RTC wake disabled",
    )
}

fn alarm_owned(current: &str, owned: u64) -> Result<bool> {
    if matches!(current, "" | "0") {
        return Ok(false);
    }
    require(
        current.parse::<u64>()? == owned,
        "foreign RTC alarm; refusing removal",
    )?;
    Ok(true)
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
    fallback: u64,
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
            "timing_sha",
            "due",
            "wake",
            "rtc_fallback",
            "review",
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
            "variable",
            "original",
            "intervention",
            "rollback",
            "hypothesis",
            "prediction_wol",
            "prediction_rtc",
            "instrumentation",
        ];
        require(
            m.len() == keys.len() && keys.iter().all(|k| m.contains_key(*k)),
            "unknown/missing manifest field",
        )?;
        for (k, v) in [
            ("schema", "s2idle-wake-path-20261101-first-1"),
            ("boot", BOOT),
            ("historical_btime", "1793474520"),
            ("success_before", "1"),
            ("markers_before", "0"),
            ("kind", "s2idle-wake-path"),
            ("variable", "nic-d3cold-allowed"),
            ("original", "1"),
            ("intervention", "0"),
            ("rollback", "1"),
            (
                "hypothesis",
                "upstream-bridge-D3-may-break-firmware-owned-PME-wake",
            ),
            (
                "prediction_wol",
                "D0-bridge-and-PME-armed-with-WoL-return-supports-power-path-sensitivity",
            ),
            (
                "prediction_rtc",
                "D0-bridge-and-PME-armed-with-RTC-only-return-shows-this-restriction-insufficient",
            ),
            (
                "instrumentation",
                "six-pinned-existing-PCI-ACPI-debug-sites-original-off",
            ),
            ("pm_async", "0"),
            ("pm_trace", "0"),
            ("pm_test", "none"),
            ("mem_sleep", "s2idle"),
            ("recorder", "off-normal-utc-rtc-alarm"),
            ("tpm", "initialized"),
            ("ec", "not-run"),
            ("measurement", "dated-power-graph-required"),
            (
                "reset_provenance",
                "owner-forced-reboot-method-off-duration-unspecified",
            ),
            (
                "recovery_sha",
                "ec6d9a401a59064512c56c035894715d4123ef69ee301cc83297a373c9a83b7d",
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
        for k in [
            "runtime_sha",
            "source_sha",
            "timing_sha",
            "handoff_sha",
            "recovery_sha",
        ] {
            require(
                m[k].len() == 64 && m[k].bytes().all(|c| c.is_ascii_hexdigit()),
                "bad digest",
            )?;
        }
        require(
            m["runtime"].starts_with("/nix/store/")
                && m["runtime"].ends_with("/bin/predator-pm-wake-path"),
            "runtime not immutable",
        )?;
        let due = m["due"].parse::<u64>()?;
        let wake = m["wake"].parse::<u64>()?;
        let fallback = m["rtc_fallback"].parse::<u64>()?;
        let btime = m["btime"].parse::<u64>()?;
        require(
            btime == BTIME
                && due > btime
                && (DAY_START..=LAST_ENTRY).contains(&due)
                && m["window_end"] == DAY_END.to_string()
                && due.checked_add(960) == Some(wake)
                && due.checked_add(1200) == Some(fallback)
                && m["review"].parse::<u64>()? == REVIEW
                && fallback.checked_add(180).is_some_and(|t| t < REVIEW),
            "dated window/btime/wake invalid",
        )?;
        Ok(Self {
            m,
            due,
            wake,
            fallback,
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
                "/var/lib/predator-pm-wake-validation/wake-20261031-plan.conf",
                "767cd2aff3f22cc1b8691ae63e3435d83279f7952b763870a53e0b680214b942",
            ),
            (
                "/var/lib/predator-pm-wake-validation/wake-20261031-completed.txt",
                "14b4bc808a6056346cc7d2844c8d48b35d16b3970f1312ac5c34a740b7a6fbb9",
            ),
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
            (
                "/var/lib/predator-pm-s2idle/s2idle-20261031-plan.conf",
                "f22e464bb3aae8404d25ec05f7b4ae3002ba0c757a9d5e69f1199195f13a5820",
            ),
            (
                "/var/lib/predator-pm-s2idle/s2idle-20261031-raw-return.txt",
                "da403c1d125532c0d8f9d0935c64a9f868a2ddacbf6bcc031fd75ad936b26269",
            ),
        ] {
            require(hash(p)? == h, format!("historical anchor changed: {p}"))?;
        }
        Ok(())
    }
    fn timing(&self, execute: bool) -> Result<()> {
        let n = now()?;
        require(
            (DAY_START..DAY_END).contains(&n)
                && if execute {
                    (self.due..=self.due + 30).contains(&n)
                } else {
                    n.checked_add(300).is_some_and(|x| x < self.due)
                },
            "arming/execution deadline",
        )?;
        require(out("date", &["+%F"])? == "2026-11-01", "wrong date")
    }
}
fn reviewed_history(s: &str) -> Result<()> {
    let expected = [
        ("PM: suspend entry (s2idle)", 6790650591u64),
        ("PM: suspend-to-idle", 6842844799),
        ("PM: resume from suspend-to-idle", 7979482880),
        ("PM: suspend exit", 7983055184),
    ];
    let mut phase = 0;
    let mut begin = 0;
    let mut returned = 0;
    for line in s.lines() {
        let v: serde_json::Value = serde_json::from_str(line)?;
        require(
            v["_BOOT_ID"].as_str() == Some(JOURNAL_BOOT)
                && v["_TRANSPORT"].as_str() == Some("kernel"),
            "history identity",
        )?;
        let m = v["MESSAGE"].as_str().ok_or("non-text history")?;
        for bad in [
            "Low-level resume complete",
            "PM: suspend debug:",
            "RTC BEGIN ",
            "S2IDLE BEGIN ",
            "DIAG BEGIN ",
            "WAKEPATH BEGIN ",
            "BUG:",
            "Oops:",
            "Call Trace:",
        ] {
            require(!m.contains(bad), format!("unreviewed history: {bad}"))?;
        }
        if m.starts_with("PM: suspend entry (") || expected.iter().any(|(x, _)| m == *x) {
            require(phase < expected.len(), "extra previous cycle")?;
            let (message, timestamp) = expected[phase];
            require(
                m == message
                    && v["_SOURCE_MONOTONIC_TIMESTAMP"].as_str()
                        == Some(timestamp.to_string().as_str())
                    && v["_SOURCE_BOOTTIME_TIMESTAMP"].as_str()
                        == Some(timestamp.to_string().as_str()),
                "previous phase changed",
            )?;
            phase += 1;
        }
        if m.contains("WAKE BEGIN ") {
            require(
                m.contains(&format!("WAKE BEGIN id=wake-20261031 boot={BOOT} "))
                    && m.contains("nonce=wake-20261031-1793481300 "),
                "unknown previous begin",
            )?;
            begin += 1;
        }
        if m.contains("WAKE RETURN ") {
            require(m.ends_with("WAKE RETURN id=wake-20261031 result=RTC_FALLBACK_WINDOW_RETURN full_pass=false"), "unknown previous return")?;
            returned += 1;
        }
    }
    require(
        phase == 4 && begin == 1 && returned == 1,
        "sealed previous cycle missing/duplicated",
    )
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
        "predator-pm-wake-validation",
        "predator-pm-wake-path",
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
        ("/sys/bus/pci/devices/0000:00:1c.3/power/wakeup", "enabled"),
        ("/sys/bus/pci/devices/0000:00:1c.3/d3cold_allowed", "1"),
        ("/sys/bus/pci/devices/0000:03:00.0/d3cold_allowed", "1"),
        ("/sys/class/net/enp3s0f1/address", "98:29:a6:3a:a7:50"),
        ("/sys/bus/pci/devices/0000:03:00.1/vendor", "0x10ec"),
        ("/sys/bus/pci/devices/0000:03:00.1/device", "0x8168"),
        (D3COLD, "1"),
    ] {
        require(text(path)? == v, format!("unexpected {path}"))?;
    }
    require(
        text("/sys/power/suspend_stats/success")?.parse::<u64>()? == 1 + u64::from(after),
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
    clock_valid()?;
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
        reviewed_history(&journal()?)?;
    }
    debug_sites(&text(DEBUG)?, "_")?;
    require(
        fs::canonicalize("/sys/class/net/enp3s0f1/device")?
            == Path::new("/sys/devices/pci0000:00/0000:00:1c.3/0000:03:00.1"),
        "NIC topology changed",
    )?;
    Ok(())
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
fn debug_sites(control: &str, flags: &str) -> Result<String> {
    let mut result = String::new();
    for (site, function) in SITES {
        let lines: Vec<_> = control
            .lines()
            .filter(|l| l.starts_with(&format!("{site} ")))
            .collect();
        require(
            lines.len() == 1,
            format!("debug site absent/duplicated: {site}"),
        )?;
        let parts: Vec<_> = lines[0].split_whitespace().collect();
        require(
            parts.len() >= 4
                && parts[1].ends_with(&format!("]{function}"))
                && parts[2] == format!("={flags}"),
            format!("unexpected debug function/flags: {}", lines[0]),
        )?;
        result.push_str(lines[0]);
        result.push('\n');
    }
    Ok(result)
}
fn debug_write(site: &str, enable: bool) -> Result<()> {
    let (file, line) = site.split_once(':').ok_or("bad pinned site")?;
    fs::write(
        DEBUG,
        format!(
            "file {file} line {line} {}p\n",
            if enable { "+" } else { "-" }
        ),
    )?;
    Ok(())
}
fn debug_restore() -> Result<()> {
    // Only our original empty flags or our enabled print flag are acceptable.
    let control = text(DEBUG)?;
    for (site, function) in SITES {
        let lines: Vec<_> = control
            .lines()
            .filter(|l| l.starts_with(&format!("{site} ")))
            .collect();
        require(lines.len() == 1, "debug restoration site changed")?;
        let parts: Vec<_> = lines[0].split_whitespace().collect();
        require(
            parts.len() >= 4
                && parts[1].ends_with(&format!("]{function}"))
                && matches!(parts[2], "=_" | "=p"),
            "foreign debug flags; refusing overwrite",
        )?;
    }
    for (site, _) in SITES {
        debug_write(site, false)?;
    }
    debug_sites(&text(DEBUG)?, "_")?;
    Ok(())
}
fn path_snapshot() -> Result<String> {
    let mut s = format!(
        "interrupts:\n{}\nacpi_wakeup:\n{}\n",
        text("/proc/interrupts")?,
        text("/proc/acpi/wakeup")?
    );
    for name in [
        "ff_rt_clk",
        "ff_pwr_btn",
        "ff_slp_btn",
        "gpe_14",
        "gpe_66",
        "sci",
        "sci_not",
        "gpe_all",
        "error",
    ] {
        s.push_str(&format!(
            "ACPI {name}={}\n",
            text(format!("/sys/firmware/acpi/interrupts/{name}"))?
        ));
    }
    for dev in ["0000:03:00.1", "0000:03:00.0", "0000:00:1c.3"] {
        let base = format!("/sys/bus/pci/devices/{dev}");
        for name in [
            "power_state",
            "d3cold_allowed",
            "power/wakeup",
            "power/control",
            "power/runtime_status",
        ] {
            s.push_str(&format!(
                "PCI {dev} {name}={}\n",
                text(format!("{base}/{name}"))?
            ));
        }
        let mut b = Vec::new();
        fs::File::open(format!("{base}/config"))?
            .take(256)
            .read_to_end(&mut b)?;
        require(b.len() == 256, "incomplete PCI configuration snapshot")?;
        s.push_str(&format!(
            "PCI {dev} config256={}\n",
            b.iter().map(|x| format!("{x:02x}")).collect::<String>()
        ));
    }
    s.push_str("debug_sites:\n");
    let control = text(DEBUG)?;
    for (site, _) in SITES {
        for line in control
            .lines()
            .filter(|l| l.starts_with(&format!("{site} ")))
        {
            s.push_str(line);
            s.push('\n');
        }
    }
    for entry in fs::read_dir("/sys/class/thermal")? {
        let entry = entry?;
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("thermal_zone")
        {
            s.push_str(&format!(
                "thermal {}={}\n",
                text(entry.path().join("type"))?,
                text(entry.path().join("temp"))?
            ));
        }
    }
    Ok(s)
}
fn experiment_evidence(journal: &str) -> Result<()> {
    let messages: Vec<String> = journal
        .lines()
        .map(|l| -> Result<String> {
            let v: serde_json::Value = serde_json::from_str(l)?;
            require(
                v["_BOOT_ID"].as_str() == Some(JOURNAL_BOOT),
                "experiment journal boot mismatch",
            )?;
            if v["_TRANSPORT"].as_str() != Some("kernel") {
                return Ok(String::new());
            }
            Ok(v["MESSAGE"]
                .as_str()
                .ok_or("non-text experiment message")?
                .into())
        })
        .collect::<Result<_>>()?;
    for (device, event) in [
        ("0000:03:00.1", "PME# enabled"),
        ("0000:03:00.1", "PCI PM: Suspend power state: D3hot"),
        ("0000:00:1c.3", "PCI PM: Suspend power state: D0"),
        ("0000:00:1c.3", "Wakeup enabled by ACPI"),
    ] {
        require(
            messages
                .iter()
                .any(|m| m.contains(device) && m.ends_with(event)),
            format!("experiment not established: {device} {event}"),
        )?;
    }
    Ok(())
}
fn wake_snapshot() -> Result<String> {
    let irq = match fs::read_to_string("/sys/power/pm_wakeup_irq") {
        Ok(s) => s.trim().to_string(),
        Err(e) if e.raw_os_error() == Some(61) => "ENODATA:no-recorded-wake-IRQ".into(),
        Err(e) => return Err(e.into()),
    };
    Ok(format!(
        "epoch={}\nrtc_epoch={}\nalarm={}\nwake_irq={}\n{}rtc_driver:\n{}\nwakeup_sources:\n{}\n{}",
        now()?,
        text("/sys/class/rtc/rtc0/since_epoch")?,
        text(ALARM)?,
        irq,
        raw_rtc()?,
        text("/proc/driver/rtc")?,
        text("/sys/kernel/debug/wakeup_sources")?,
        path_snapshot()?
    ))
}
fn restore(p: &Plan) -> Result<()> {
    p.identity()?;
    sleep_inactive()?;
    if !p.exists("mutating") || p.exists("restored") {
        return Ok(());
    }
    if !p.exists("raw-return") {
        p.save("raw-return", &wake_snapshot()?)?;
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
    require(
        text("/sys/module/pm_stage/parameters/armed")? == "N",
        "unexpected recorder mutation",
    )?;
    if alarm_owned(&text(ALARM)?, p.fallback)? {
        require(p.exists("alarm-intent"), "alarm ownership intent missing")?;
        fs::write(ALARM, "0")?;
    }
    require(
        matches!(text(ALARM)?.as_str(), "" | "0"),
        "owned alarm not cleared",
    )?;
    require(
        matches!(text(D3COLD)?.as_str(), "0" | "1"),
        "unexpected intervention setting",
    )?;
    fs::write(D3COLD, p.get("rollback"))?;
    require(text(D3COLD)? == p.get("original"), "D3cold rollback failed")?;
    if p.exists("debug-intent") {
        debug_restore()?;
    }
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
            "epoch={}\nrtc={}\nclock={:?}\nuptime={}\nbtime_observed={}\nmem_sleep={}\nnic_d3cold_allowed={}\ndebug_sites={}\n",
            now()?,
            out("hwclock", &["--show", "--utc"])?,
            clock_status()?,
            text("/proc/uptime")?,
            text("/proc/stat")?
                .lines()
                .find(|l| l.starts_with("btime "))
                .ok_or("missing btime")?,
            text("/sys/power/mem_sleep")?,
            text(D3COLD)?,
            debug_sites(&text(DEBUG)?, "_")?
        ),
    )
}
fn return_class(observed: u64, wake: u64, fallback: u64) -> &'static str {
    if observed < wake {
        "EARLY_RETURN"
    } else if observed <= wake + 90 {
        "WOL_WINDOW_RETURN"
    } else if observed < fallback {
        "INTERMEDIATE_RETURN_UNKNOWN_CAUSE"
    } else if observed <= fallback + 180 {
        "RTC_FALLBACK_WINDOW_RETURN"
    } else {
        "LATE_RETURN"
    }
}
fn run(p: &Plan, raw: &str) -> Result<()> {
    p.save("requested", raw)?;
    p.timing(true)?;
    health(p, false)?;
    p.save("before-kernel", &journal()?)?;
    p.save("sleep-config", &sleep_config()?)?;
    p.save("wake-before", &wake_snapshot()?)?;
    p.save("debug-original", &debug_sites(&text(DEBUG)?, "_")?)?;
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
            "json",
        ])
        .args([
            "_TRANSPORT=kernel",
            "+",
            "_SYSTEMD_UNIT=systemd-suspend.service",
            "+",
            "UNIT=systemd-suspend.service",
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
    let op: Result<wake_timing::Cycle> = (|| {
        p.save(
            "mutating",
            "owned NIC d3cold 1-to-0; six existing debug sites off-to-print; serial s2idle and owned RTC alarm; normal clocks unchanged\n",
        )?;
        require(
            text(D3COLD)? == p.get("original"),
            "intervention preimage changed",
        )?;
        fs::write(D3COLD, p.get("intervention"))?;
        require(
            text(D3COLD)? == p.get("intervention"),
            "intervention readback failed",
        )?;
        debug_sites(&text(DEBUG)?, "_")?;
        p.save(
            "debug-intent",
            "six pinned originally-off debug sites; +p only\n",
        )?;
        for (site, _) in SITES {
            debug_write(site, true)?;
        }
        debug_sites(&text(DEBUG)?, "p")?;
        p.save("intervention", &path_snapshot()?)?;
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
            "WAKEPATH BEGIN id={} boot={BOOT} kind=s2idle-wake-path success_before=1 markers_before=0 nonce={} planned_wake={} age_since_recovery_boot={} duration_source=external-power-graph",
            p.get("id"),
            p.get("nonce"),
            p.wake,
            now()? - BTIME
        ))?;
        out("journalctl", &["--sync"])?;
        clock_valid()?;
        require(
            matches!(text(ALARM)?.as_str(), "" | "0"),
            "alarm appeared before arming",
        )?;
        p.save("alarm-intent", &format!("owned_epoch={}\n", p.fallback))?;
        fs::write(ALARM, p.fallback.to_string())?;
        require(
            text(ALARM)? == p.fallback.to_string(),
            "absolute alarm readback failed",
        )?;
        p.save("alarm-armed", &wake_snapshot()?)?;
        require(
            text("/sys/module/pm_stage/parameters/armed")? == "N"
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
        let (s, r) = wake_timing::receive(&rx, JOURNAL_BOOT);
        captured = s;
        let result = r?;
        // Check UTC/RTC coherence before any helper can correct a bad clock.
        clock_valid()?;
        Ok(result)
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
    health(p, true)?;
    p.save("after-kernel", &journal()?)?;
    let measurement = experiment_evidence(&captured);
    p.save("measurement", &format!("verified_D0_bridge_D3hot_NIC_PME_and_ACPI_arming={measurement:?}\nraw_registers_are_post_return_not_in_sleep\n"))?;
    measurement?;
    let class = return_class(observed, p.wake, p.fallback);
    p.save("completed", &format!("result={class}\nid={}\nboot={BOOT}\nruntime={}\nsource_sha={}\ntiming_sha={}\ncompleted_epoch={}\npre_restore_return_epoch={observed}\nmarkers_after=0\nsuccess_before=1\nsuccess_after=2\nreal_s3=false\nsource_loop_mono_us={}\nsource_loop_boot_us={}\nreceipt_loop_mono_us={}\nreceipt_loop_real_us={}\nphase_timestamps={elapsed:?}\nintervention=nic-d3cold-allowed-0\nD0_bridge_D3hot_NIC_PME_ACPI_arming=verified-source-log\nwake_cause=requires-independent-IRQ-alarm-and-Pi-evidence\nphysical_sleep_seconds=pending-dated-power-graph\nfull_pass=false\n",p.get("id"),p.get("runtime"),p.get("source_sha"),p.get("timing_sha"),now()?,elapsed.end.source_mono-elapsed.start.source_mono,elapsed.end.source_boot-elapsed.start.source_boot,elapsed.end.receipt_mono-elapsed.start.receipt_mono,elapsed.end.receipt_real-elapsed.start.receipt_real))?;
    marker(&format!(
        "WAKEPATH RETURN id={} result={class} full_pass=false",
        p.get("id")
    ))
}
fn main() -> Result<()> {
    require(
        (DAY_START..DAY_END).contains(&now()?) && out("date", &["+%F"])? == "2026-11-01",
        "outside authorized machine window",
    )?;
    let a: Vec<String> = env::args().collect();
    if a.len() == 3 && a[1] == "check-history-file" {
        reviewed_history(&fs::read_to_string(&a[2])?)?;
        println!("sealed-cumulative-history=PASS success_before=1");
        return Ok(());
    }
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
            wake_snapshot()?;
            marker(&format!(
                "WAKEPATH READY id={} nonce={}",
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
            "schema=s2idle-wake-path-20261101-first-1\nid=path-d3-20261101\nboot={BOOT}\nruntime=/nix/store/test/bin/predator-pm-wake-path\nruntime_sha={}\nsource_sha={}\ntiming_sha={}\nhandoff_sha={}\ndue=1793520900\nwake=1793521860\nrtc_fallback=1793522100\nreview=1793525400\nwindow_end=1793570400\nbtime=1793474520\nhistorical_btime=1793474520\nsuccess_before=1\nmarkers_before=0\nkind=s2idle-wake-path\nrecovery_sha=ec6d9a401a59064512c56c035894715d4123ef69ee301cc83297a373c9a83b7d\npm_async=0\npm_trace=0\npm_test=none\nmem_sleep=s2idle\nrecorder=off-normal-utc-rtc-alarm\ntpm=initialized\nec=not-run\nmeasurement=dated-power-graph-required\nreset_provenance=owner-forced-reboot-method-off-duration-unspecified\nnonce=wake-nonce\nvariable=nic-d3cold-allowed\noriginal=1\nintervention=0\nrollback=1\nhypothesis=upstream-bridge-D3-may-break-firmware-owned-PME-wake\nprediction_wol=D0-bridge-and-PME-armed-with-WoL-return-supports-power-path-sensitivity\nprediction_rtc=D0-bridge-and-PME-armed-with-RTC-only-return-shows-this-restriction-insufficient\ninstrumentation=six-pinned-existing-PCI-ACPI-debug-sites-original-off\n",
            "a".repeat(64),
            "b".repeat(64),
            "d".repeat(64),
            "c".repeat(64)
        )
    }
    #[test]
    fn strict_reviewed_boot_single_variable_protocol() {
        let s = manifest();
        Plan::parse(&s).unwrap();
        for bad in [
            s.replace("mem_sleep=s2idle", "mem_sleep=deep"),
            s.replace("success_before=1", "success_before=0"),
            s.replace("success_before=1", "success_before=2"),
            s.replace("markers_before=0", "markers_before=56"),
            s.replace("kind=s2idle-wake-path", "kind=s3"),
            s.replace("pm_test=none", "pm_test=core"),
            s.replace("wake=1793521860", "wake=1793521800"),
            s.replace("rtc_fallback=1793522100", "rtc_fallback=1793521860"),
            s.replace("due=1793520900", "due=1793481300"),
            s.replace("due=1793520900", "due=1793523601"),
            s.replace("review=1793525400", "review=1793525500"),
            s.replace("intervention=0", "intervention=1"),
            s.replace("rollback=1", "rollback=0"),
            s.replace("btime=1793474520", "btime=1793381192"),
            s.replace("off-normal-utc-rtc-alarm", "rtc-unsync-zerooffset"),
            s.replace(BOOT, "f8eb6e44-85ae-4e56-bc00-587876699945"),
            s.clone() + "schema=s2idle-wake-path-20261101-first-1\n",
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
    #[test]
    fn only_owned_alarm_can_be_removed() {
        assert!(!alarm_owned("", 12345).unwrap());
        assert!(!alarm_owned("0", 12345).unwrap());
        assert!(alarm_owned("12345", 12345).unwrap());
        assert!(alarm_owned("12346", 12345).is_err());
        assert!(alarm_owned("invalid", 12345).is_err());
    }
    #[test]
    fn cumulative_history_requires_exact_reviewed_cycle() {
        let records = [
            ("pm-wake-validation: WAKE BEGIN id=wake-20261031 boot=f9b45aa6-b820-4ab5-b18c-9324d8d53087 nonce=wake-20261031-1793481300 planned_wake=1793482260", 6790600000),
            ("PM: suspend entry (s2idle)", 6790650591u64),
            ("PM: suspend-to-idle", 6842844799),
            ("PM: resume from suspend-to-idle", 7979482880),
            ("PM: suspend exit", 7983055184),
            ("pm-wake-validation: WAKE RETURN id=wake-20261031 result=RTC_FALLBACK_WINDOW_RETURN full_pass=false", 8000273197),
        ].map(|(message,t)|serde_json::json!({"_BOOT_ID":JOURNAL_BOOT,"_TRANSPORT":"kernel","MESSAGE":message,"_SOURCE_MONOTONIC_TIMESTAMP":t.to_string(),"_SOURCE_BOOTTIME_TIMESTAMP":t.to_string()}).to_string()).join("\n");
        reviewed_history(&records).unwrap();
        for bad in [
            String::new(),
            format!("{records}\n{records}"),
            records.replace("6842844799", "6842844798"),
            records.replace("(s2idle)", "(deep)"),
            records.replace("wake-20261031", "other"),
            records.replace(JOURNAL_BOOT, "wrong-boot"),
        ] {
            assert!(reviewed_history(&bad).is_err());
        }
    }
    #[test]
    fn pinned_debug_sites_refuse_missing_foreign_or_enabled_baselines() {
        let off = SITES
            .map(|(site, function)| format!("{site} [pci]{function} =_ \"message\\n\"\n"))
            .concat();
        debug_sites(&off, "_").unwrap();
        debug_sites(&off.replace("=_", "=p"), "p").unwrap();
        for bad in [
            off.replace("=_", "=p"),
            off.replace("=_", "=fp"),
            off.replace(":2513", ":2514"),
            format!("{off}{off}"),
            off.replace("pci_pme_active", "another_function"),
        ] {
            assert!(debug_sites(&bad, "_").is_err());
        }
    }
    #[test]
    fn actual_suspend_state_and_arming_are_required() {
        let records = [
            "r8169 0000:03:00.1: PME# enabled",
            "r8169 0000:03:00.1: PCI PM: Suspend power state: D3hot",
            "pcieport 0000:00:1c.3: PCI PM: Suspend power state: D0",
            "pcieport 0000:00:1c.3: Wakeup enabled by ACPI",
        ]
        .map(|m| {
            serde_json::json!({"_BOOT_ID":JOURNAL_BOOT,"_TRANSPORT":"kernel","MESSAGE":m})
                .to_string()
        })
        .join("\n");
        experiment_evidence(&records).unwrap();
        for bad in [
            records.replace("state: D0", "state: D3hot"),
            records.replace("PME# enabled", "PME# disabled"),
            records.replace("Wakeup enabled", "Wakeup disabled"),
            records.replace("\"kernel\"", "\"stdout\""),
            records.replace(JOURNAL_BOOT, "other"),
        ] {
            assert!(experiment_evidence(&bad).is_err());
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
        assert_eq!(return_class(100, 1060, 1300), "EARLY_RETURN");
        assert_eq!(return_class(1060, 1060, 1300), "WOL_WINDOW_RETURN");
        assert_eq!(
            return_class(1200, 1060, 1300),
            "INTERMEDIATE_RETURN_UNKNOWN_CAUSE"
        );
        assert_eq!(return_class(1300, 1060, 1300), "RTC_FALLBACK_WINDOW_RETURN");
        assert_eq!(return_class(1481, 1060, 1300), "LATE_RETURN");
    }
    #[test]
    fn timex_abi() {
        assert_eq!(std::mem::size_of::<Timex>(), 208);
        assert_eq!(std::mem::offset_of!(Timex, status), 40);
        assert_eq!(std::mem::offset_of!(Timex, time), 72);
        assert_eq!(std::mem::offset_of!(Timex, tai), 160);
    }
}
