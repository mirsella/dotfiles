//! Separate RTC-instrumentation protocol. Never creates ordinary S3 or diagnostic
//! completion records. A return remains pending independently dated power proof.
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
const STATE: &str = "/var/lib/predator-pm-instrumented";
const BOOT: &str = "f8eb6e44-85ae-4e56-bc00-587876699945";
const JOURNAL_BOOT: &str = "f8eb6e4485ae4e56bc00587876699945";
const SYSTEM: &str =
    "/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363";
const KERNEL: &str = "/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage";
const MODULES: &str = "/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules";
const RECORDER: &str = "/nix/store/3jcksrsfmyw8vk1cwp0x5ynb3h1q1pdp-predator-pm-stage-20261009/lib/modules/6.18.58/extra/pm_stage.ko.xz";
const HANDOFF: &str =
    "/nix/store/l423y5sbmkgcl50cjfkxwhqg1im7b31q-predator-pm-study-0.1.0/bin/predator-pm-study";
const LAN: &str = "c4e6863b-7f76-4a4d-9a48-9937af0b5a36";

// Read-only equivalents of the pinned diagnostic history/maintenance guards.
// The diagnostic runner itself remains byte-for-byte unchanged.
mod reviewed {
    use super::*;
    pub(super) fn history(s: &str) -> Result<u64> {
        let (mut phase, mut count, mut core, mut clock) = (0, 0, false, false);
        for l in s.lines() {
            require(
                !l.contains("Low-level resume complete"),
                "real sleep in diagnostic prefix",
            )?;
            if l.contains("DIAG BEGIN id=") {
                require(phase == 0, "nested diagnostic")?;
                core = l.contains(" level=core ");
            }
            if let Some((_, d)) = l.split_once("Timekeeping suspended for ") {
                let n = d
                    .strip_suffix(" seconds")
                    .ok_or("bad duration units")?
                    .parse::<f64>()?;
                require(
                    core && phase == 2 && !clock && n.is_finite() && (4.0..=10.0).contains(&n),
                    "unaccounted timekeeping gap",
                )?;
                clock = true;
            } else if l.contains("PM: suspend entry (") {
                require(phase == 0 && l.contains("(deep)"), "bad diagnostic entry")?;
                phase = 1;
                clock = false;
            } else if l.contains("PM: suspend debug: Waiting for") {
                require(
                    phase == 1 && l.contains("Waiting for 5 second(s)."),
                    "bad diagnostic checkpoint",
                )?;
                phase = 2;
            } else if l.contains("PM: suspend exit") {
                require(phase == 2, "bad diagnostic exit")?;
                phase = 0;
                count += 1;
                core = false;
            }
        }
        require(phase == 0, "incomplete diagnostic history")?;
        Ok(count)
    }
    pub(super) fn quiet() -> Result<()> {
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
    pub(super) fn restore_allowed(s: &str, p: &str, j: &str) -> Result<()> {
        require(
            s == "inactive" && p == "0" && !j.contains("sleep") && !j.contains("suspend"),
            "sleep active/queued; refusing restoration",
        )
    }
}
fn require(ok: bool, why: impl Into<String>) -> Result<()> {
    if ok {
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
        .ok_or("no hash")?
        .into())
}
fn fields(s: &str) -> Result<BTreeMap<String, String>> {
    let mut m = BTreeMap::new();
    for l in s.lines() {
        let (k, v) = l.split_once('=').ok_or("bad field")?;
        require(
            !v.is_empty() && m.insert(k.into(), v.into()).is_none(),
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
        "<6>pm-instrumented: {s}"
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
fn sleep_inactive() -> Result<()> {
    reviewed::restore_allowed(
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

// Linux x86_64 glibc timex ABI, no dependency downloads at runtime.
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
fn adjust(tx: &mut Timex) -> Result<i32> {
    require(
        cfg!(all(target_arch = "x86_64", target_os = "linux"))
            && std::mem::size_of::<Timex>() == 208,
        "unsupported timex ABI",
    )?;
    let rc = unsafe { clock_adjtime(0, tx) };
    require(
        rc >= 0,
        format!("clock_adjtime: {}", std::io::Error::last_os_error()),
    )?;
    Ok(rc)
}
fn clock_status() -> Result<Timex> {
    let mut t = Timex::default();
    adjust(&mut t)?;
    Ok(t)
}
fn quiesce_clock() -> Result<String> {
    out("systemctl", &["stop", "systemd-timesyncd.service"])?;
    let before = clock_status()?;
    // STA_UNSYNC blocks future writes; zero ADJ_SETOFFSET cancels the RTC hrtimer.
    // An already-running freezable work item is drained by freeze_kernel_threads
    // before dpm_suspend and the low-level markers (see retained matching source).
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
        "timesyncd did not stop",
    )?;
    Ok(format!("before={before:?}\nafter={after:?}\nzero_offset_cancel=true\nlate_marker_barrier=freeze_kernel_threads_freezable_workqueue\n"))
}

struct Plan {
    m: BTreeMap<String, String>,
    due: u64,
    wake: u64,
    end: u64,
    btime: u64,
    before: u64,
    markers: u64,
    cal: bool,
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
            "due",
            "wake",
            "window_end",
            "btime",
            "historical_btime",
            "success_before",
            "markers_before",
            "kind",
            "predecessor",
            "predecessor_sha",
            "pm_async",
            "pm_trace",
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
            ("schema", "rtc-instrumented-1"),
            ("boot", BOOT),
            ("historical_btime", "1793381192"),
            ("pm_async", "0"),
            ("pm_trace", "0"),
            ("recorder", "rtc-unsync-zerooffset"),
            ("tpm", "initialized"),
            ("ec", "not-run"),
            ("measurement", "dated-power-graph-required"),
            ("reset_provenance", "owner-recovery-mechanism-unspecified"),
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
        for k in ["runtime_sha", "handoff_sha", "predecessor_sha"] {
            require(
                m[k].len() == 64 && m[k].bytes().all(|c| c.is_ascii_hexdigit()),
                "bad digest",
            )?;
        }
        require(
            m["runtime"].starts_with("/nix/store/")
                && m["runtime"].ends_with("/bin/predator-pm-instrumented"),
            "runtime not immutable",
        )?;
        require(
            matches!(m["kind"].as_str(), "calibration-core" | "s3"),
            "unsupported operation",
        )?;
        let cal = m["kind"] == "calibration-core";
        let due = m["due"].parse::<u64>()?;
        let wake = m["wake"].parse::<u64>()?;
        let end = m["window_end"].parse::<u64>()?;
        let before = m["success_before"].parse::<u64>()?;
        let markers = m["markers_before"].parse::<u64>()?;
        let btime = m["btime"].parse::<u64>()?;
        require(
            due > btime
                && due >= 1793430000
                && end == 1793484000
                && wake
                    == due
                        .checked_add(if cal { 0 } else { 960 })
                        .ok_or("overflow")?
                && wake.checked_add(600).is_some_and(|n| n < end),
            "invalid dated window/wake",
        )?;
        require(
            if cal {
                before == 9
                    && markers == 0
                    && m["predecessor"]
                        == "/var/lib/predator-pm-diagnostic/d-core-03-20261030-completed.txt"
                    && m["predecessor_sha"]
                        == "7851bcb034541c585c6385dcedd970a0e1f2fa2f3c6b0c0d93d4f1c241370ddc"
            } else {
                before == 10
                    && markers > 0
                    && m["predecessor"].starts_with(&format!("{STATE}/"))
                    && m["predecessor"].ends_with("-completed.txt")
            },
            "unsupported history transition",
        )?;
        Ok(Self {
            m,
            due,
            wake,
            end,
            btime,
            before,
            markers,
            cal,
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
            "runtime identity changed",
        )?;
        require(
            hash(HANDOFF)? == self.get("handoff_sha"),
            "clock handoff changed",
        )?;
        require(
            hash(RECORDER)? == "8fd79959fa979cabf2dfac97c4a60dbdab505cca5cf3e3902772faebd3071d9e",
            "recorder changed",
        )?;
        require(
            hash("/var/lib/predator-pm-study/last-completed.txt")?
                == "280573317fc03a7338ca233c367593ff2fac0163eb8cfe70fabc375f4648fd08",
            "ordinary anchor changed",
        )?;
        Ok(())
    }
    fn timing(&self, execute: bool) -> Result<()> {
        let n = now()?;
        require(
            n < self.end
                && if execute {
                    (self.due..=self.due + 30).contains(&n)
                } else {
                    n.checked_add(300).is_some_and(|v| v < self.due)
                },
            "outside arming/execution deadline",
        )?;
        require(out("date", &["+%F"])? == "2026-10-31", "wrong date")
    }
    fn predecessor(&self) -> Result<()> {
        require(
            hash(self.get("predecessor"))? == self.get("predecessor_sha"),
            "predecessor changed",
        )?;
        let r = fields(&root_file(self.get("predecessor"))?)?;
        require(
            r.get("boot").map(String::as_str) == Some(BOOT),
            "wrong predecessor boot",
        )?;
        if !self.cal {
            require(
                r.get("result").map(String::as_str) == Some("RTC_CORE_CALIBRATION_VERIFIED")
                    && r.get("runtime").map(String::as_str) == Some(self.get("runtime"))
                    && r.get("markers_after")
                        .ok_or("missing calibration markers")?
                        .parse::<u64>()?
                        == self.markers,
                "calibration not verified",
            )?;
        }
        Ok(())
    }
}

fn cycle(s: &str, cal: bool) -> Result<()> {
    let mut phase = 0;
    let mut delay = 0;
    for l in s.lines() {
        if l.contains("PM: suspend entry (") {
            require(phase == 0 && l.contains("(deep)"), "unexpected entry")?;
            phase = 1;
        }
        if l.contains("PM: suspend debug: Waiting for") {
            require(
                cal && phase == 1 && l.contains("Waiting for 5 second(s)."),
                "unexpected test checkpoint",
            )?;
            delay += 1;
        }
        if l.contains("Low-level resume complete") {
            require(!cal && phase == 1, "unexpected low-level return")?;
            phase = 2;
        }
        if l.contains("PM: suspend exit") {
            require(
                if cal {
                    phase == 1 && delay == 1
                } else {
                    phase == 2
                },
                "exit without ordered return",
            )?;
            phase = 3;
        }
    }
    require(
        phase == 3
            && s.contains("System returned from sleep operation")
            && (s.contains("Finished System Suspend.")
                || s.contains("Finished systemd-suspend.service - System Suspend.")),
        "incomplete cycle/service return",
    )
}
fn history(s: &str, calibration_id: Option<&str>) -> Result<()> {
    let boundary = s.find("RTC BEGIN ");
    match calibration_id {
        None => require(
            boundary.is_none() && reviewed::history(s)? == 9,
            "expected nine original diagnostics",
        ),
        Some(id) => {
            let b = boundary.ok_or("no calibration history")?;
            require(
                reviewed::history(&s[..b])? == 9,
                "original diagnostics changed",
            )?;
            let tail = &s[b..];
            require(
                tail.contains(&format!("RTC BEGIN id={id} "))
                    && tail.matches("RTC BEGIN ").count() == 1,
                "unexpected intervening operation",
            )?;
            // Kernel history has no systemd completion; append these only for the
            // ordered kernel-path validator. Original completion is pinned above.
            cycle(
                &format!("{tail}\nSystem returned from sleep operation\nFinished System Suspend."),
                true,
            )
        }
    }
}

fn health(p: &Plan, after: bool) -> Result<()> {
    p.identity()?;
    sleep_inactive()?;
    for path in [
        "/var/lib/predator-pm-study/HOLD",
        "/var/lib/predator-pm-diagnostic/HOLD",
        "/var/lib/predator-pm-instrumented/HOLD",
    ] {
        require(!Path::new(path).exists(), "HOLD present")?;
    }
    for (path, value) in [
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
    ] {
        require(text(path)? == value, format!("unexpected {path}"))?;
    }
    require(
        text("/sys/power/suspend_stats/success")?.parse::<u64>()? == p.before + u64::from(after),
        "counter mismatch",
    )?;
    require(
        text("/sys/power/pm_test")?.starts_with("[none]")
            && text("/sys/power/mem_sleep")?.contains("[deep]"),
        "wrong PM mode",
    )?;
    require(
        matches!(text("/sys/class/rtc/rtc0/wakealarm")?.as_str(), "" | "0"),
        "RTC alarm present",
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
        "NTP not synchronized",
    )?;
    require(
        out("zpool", &["list", "-H", "-o", "name"])? == "fast\ntank"
            && out("zpool", &["status", "-x"])? == "all pools are healthy"
            && !out("zpool", &["status"])?.contains("in progress"),
        "pool health/maintenance",
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
            format!("wrong mount {m}"),
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
    reviewed::quiet()?;
    if !after {
        require(
            text("/sys/module/pm_stage/parameters/markers")?.parse::<u64>()? == p.markers,
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
        let prior = if p.cal {
            None
        } else {
            Some(
                fields(&root_file(p.get("predecessor"))?)?
                    .remove("id")
                    .ok_or("no prior id")?,
            )
        };
        history(&journal()?, prior.as_deref())?;
    }
    Ok(())
}

fn raw_rtc() -> Result<String> {
    Ok(format!("rtc_date={}\nrtc_time={}\nhwclock={}\nlast_action={}\nlast_value={}\nlast_start={}\nmarkers={}\nclock={:?}\nuptime={}\n",text("/sys/class/rtc/rtc0/date")?,text("/sys/class/rtc/rtc0/time")?,out("hwclock",&["--show","--utc"] )?,text("/sys/module/pm_stage/parameters/last_action")?,text("/sys/module/pm_stage/parameters/last_value")?,text("/sys/module/pm_stage/parameters/last_start")?,text("/sys/module/pm_stage/parameters/markers")?,clock_status()?,text("/proc/uptime")?))
}
fn validate_rtc(s: &str, before: u64) -> Result<u64> {
    let f = fields(s)?;
    require(
        f["rtc_date"] == "2023-04-10"
            && f["rtc_time"].starts_with("00:00:")
            && f["last_action"] == "17"
            && f["last_value"] == "3"
            && f["last_start"] == "N",
        "unexpected retained RTC marker",
    )?;
    let n = f["markers"].parse()?;
    require(n > before, "no new markers")?;
    Ok(n)
}
fn restore(p: &Plan) -> Result<()> {
    p.identity()?;
    sleep_inactive()?;
    if !p.exists("mutating") {
        return Ok(());
    }
    if p.exists("restored") {
        return Ok(());
    }
    // Always preserve hardware clock and volatile phase before any writer.
    if !p.exists("raw-return") {
        p.save("raw-return", &raw_rtc()?)?;
    }
    fs::write("/sys/module/pm_stage/parameters/armed", "N")?;
    let mode = text("/sys/power/pm_test")?;
    require(
        mode.starts_with("[none]") || (p.cal && mode.contains("[core]")),
        "unexpected cleanup mode",
    )?;
    fs::write("/sys/power/pm_test", "none")?;
    fs::write("/sys/power/pm_async", "1")?;
    let log = out(HANDOFF, &["clock-handoff"])?;
    p.save("clock-handoff", &log)?;
    require(
        clock_status()?.status & 0x40 == 0,
        "clock restore still UNSYNC",
    )?;
    p.save(
        "restored",
        &format!(
            "epoch={}\nrtc={}\nclock={:?}\nuptime={}\nbtime_observed={}\n",
            now()?,
            out("hwclock", &["--show", "--utc"])?,
            clock_status()?,
            text("/proc/uptime")?,
            text("/proc/stat")?
                .lines()
                .find(|l| l.starts_with("btime "))
                .ok_or("no btime")?
        ),
    )
}
fn receive(rx: &mpsc::Receiver<std::io::Result<String>>, cal: bool) -> (String, Result<()>) {
    let deadline = Instant::now() + Duration::from_secs(2400);
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
            return cycle(&s, cal);
        }
        require(
            !l.contains("Failed to suspend system")
                && !l.contains("Failed to start System Suspend"),
            "suspend failed",
        )?;
    })();
    (s, r)
}
fn run(p: &Plan, raw: &str) -> Result<()> {
    p.save("requested", raw)?;
    p.predecessor()?;
    p.timing(true)?;
    health(p, false)?;
    require(
        out("systemctl", &["list-jobs", "--no-legend", "--plain"])?.is_empty(),
        "jobs active",
    )?;
    p.save("before-kernel", &journal()?)?;
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
        p.save("mutating", "clock-and-PM-mutations-starting\n")?;
        p.save("clock-quiescence", &quiesce_clock()?)?;
        fs::write("/sys/power/pm_async", "0")?;
        if p.cal {
            fs::write("/sys/power/pm_test", "core")?;
        }
        require(
            text("/sys/power/pm_test")?.contains(if p.cal { "[core]" } else { "[none]" })
                && text("/sys/power/pm_async")? == "0",
            "PM selection failed",
        )?;
        reviewed::quiet()?;
        p.identity()?;
        p.timing(true)?;
        marker(&format!("RTC BEGIN id={} boot={BOOT} kind={} success_before={} markers_before={} nonce={} planned_wake={} age_since_original_boot={} duration_source=external-power-graph",p.get("id"),p.get("kind"),p.before,p.markers,p.get("nonce"),p.wake,now()? - 1793381192))?;
        out("journalctl", &["--sync"])?;
        require(
            clock_status()?.status & 0x40 != 0,
            "RTC writeback reenabled",
        )?;
        fs::write("/sys/module/pm_stage/parameters/armed", "Y")?;
        require(
            text("/sys/module/pm_stage/parameters/armed")? == "Y",
            "recorder failed to arm",
        )?;
        p.save("request-call", "SuspendWithFlags(1); inhibitors enforced\n")?;
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
        let (s, r) = receive(&rx, p.cal);
        captured = s;
        r
    })();
    let stop = child.kill();
    let reap = child.wait();
    let join = reader.join();
    p.save("observed-journal", &captured)?;
    p.save("return",&format!("operation={op:?}\nreader_stop={stop:?}\nreader_reap={reap:?}\nreader_joined={}\npre_restore_epoch={}\n",join.is_ok(),now()?))?;
    let cleanup = restore(p);
    p.save(
        "outcome",
        &format!("operation={op:?}\ncleanup={cleanup:?}\n"),
    )?;
    op?;
    cleanup?;
    stop?;
    reap?;
    require(join.is_ok(), "journal reader panic")?;
    let n = validate_rtc(
        &root_file(&format!("{STATE}/{}-raw-return.txt", p.get("id")))?,
        p.markers,
    )?;
    health(p, true)?;
    require(
        if p.cal {
            now()? <= p.due + 240
        } else {
            (p.wake..=p.wake + 180).contains(&now()?)
        },
        "return outside planned window",
    )?;
    p.save("after-kernel", &journal()?)?;
    p.save("completed",&format!("result={}\nid={}\nboot={BOOT}\nruntime={}\ncompleted_epoch={}\nmarkers_after={n}\nsuccess_after={}\nreal_s3={}\nphysical_sleep_seconds={}\nfull_pass=false\n",if p.cal {"RTC_CORE_CALIBRATION_VERIFIED"}else{"INSTRUMENTED_RETURN_PENDING_POWER_PROOF"},p.get("id"),p.get("runtime"),now()?,p.before+1,!p.cal,if p.cal {"not-applicable"}else{"pending-dated-power-graph"}))?;
    marker(&format!("RTC RETURN id={} full_pass=false", p.get("id")))
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
            p.predecessor()?;
            p.timing(false)?;
            health(&p, false)?;
            require(
                out("systemctl", &["list-jobs", "--no-legend", "--plain"])?.is_empty(),
                "jobs active",
            )?;
            marker(&format!(
                "RTC READY id={} nonce={}",
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
    #[test]
    fn cleanup_never_crosses_an_active_sleep_job() {
        reviewed::restore_allowed("inactive", "0", "").unwrap();
        for (s, p, j) in [
            ("active", "42", ""),
            ("activating", "0", ""),
            ("inactive", "0", "17 suspend.target start waiting"),
            ("inactive", "42", ""),
            ("failed", "0", ""),
        ] {
            assert!(reviewed::restore_allowed(s, p, j).is_err());
        }
    }
    fn manifest() -> String {
        format!("schema=rtc-instrumented-1\nid=rtc-cal-20261031\nboot={BOOT}\nruntime=/nix/store/test/bin/predator-pm-instrumented\nruntime_sha={}\nhandoff_sha={}\ndue=1793430000\nwake=1793430000\nwindow_end=1793484000\nbtime=1793381193\nhistorical_btime=1793381192\nsuccess_before=9\nmarkers_before=0\nkind=calibration-core\npredecessor=/var/lib/predator-pm-diagnostic/d-core-03-20261030-completed.txt\npredecessor_sha=7851bcb034541c585c6385dcedd970a0e1f2fa2f3c6b0c0d93d4f1c241370ddc\npm_async=0\npm_trace=0\nrecorder=rtc-unsync-zerooffset\ntpm=initialized\nec=not-run\nmeasurement=dated-power-graph-required\nreset_provenance=owner-recovery-mechanism-unspecified\nnonce=rtc-nonce\n", "a".repeat(64), "b".repeat(64))
    }
    #[test]
    fn manifests_require_explicit_history_modes_and_external_proof() {
        let good = manifest();
        Plan::parse(&good).unwrap();
        for bad in [
            good.replace("success_before=9", "success_before=0"),
            good.replace("markers_before=0", "markers_before=56"),
            good.replace("kind=calibration-core", "kind=s3"),
            good.replace("pm_trace=0", "pm_trace=1"),
            good.replace("dated-power-graph-required", "scheduled-delay"),
            good.clone() + "schema=rtc-instrumented-1\n",
            good.replace("window_end=1793484000", "window_end=1793430001"),
        ] {
            assert!(Plan::parse(&bad).is_err());
        }
        let real = good
            .replace("kind=calibration-core", "kind=s3")
            .replace("wake=1793430000", "wake=1793430960")
            .replace("success_before=9", "success_before=10")
            .replace("markers_before=0", "markers_before=56")
            .replace(
                "/var/lib/predator-pm-diagnostic/d-core-03-20261030-completed.txt",
                "/var/lib/predator-pm-instrumented/rtc-cal-20261031-completed.txt",
            );
        Plan::parse(&real).unwrap();
        assert!(Plan::parse(&real.replace("wake=1793430960", "wake=1793430900")).is_err());
    }
    #[test]
    fn timex_abi_matches_linux_x86_64() {
        assert_eq!(std::mem::size_of::<Timex>(), 208);
        assert_eq!(std::mem::offset_of!(Timex, status), 40);
        assert_eq!(std::mem::offset_of!(Timex, time), 72);
        assert_eq!(std::mem::offset_of!(Timex, tai), 160);
    }
    const END: &str = "System returned from sleep operation\nFinished System Suspend.\n";
    #[test]
    fn real_return_requires_ordered_lowlevel_and_no_debug_checkpoint() {
        let s =
            format!("PM: suspend entry (deep)\nLow-level resume complete\nPM: suspend exit\n{END}");
        cycle(&s, false).unwrap();
        for b in [
            s.replace("Low-level resume complete\n", ""),
            s.replace("(deep)", "(s2idle)"),
            s.clone() + "PM: suspend entry (deep)\n",
            s.replace(
                "PM: suspend exit",
                "PM: suspend debug: Waiting for 5 second(s).\nPM: suspend exit",
            ),
        ] {
            assert!(cycle(&b, false).is_err());
        }
        assert!(cycle(&s, true).is_err());
    }
    #[test]
    fn calibration_never_counts_as_real_sleep() {
        let s=format!("PM: suspend entry (deep)\nPM: suspend debug: Waiting for 5 second(s).\nPM: suspend exit\n{END}");
        cycle(&s, true).unwrap();
        assert!(cycle(&s, false).is_err());
        assert!(cycle(
            &s.replace(
                "PM: suspend exit",
                "Low-level resume complete\nPM: suspend exit"
            ),
            true
        )
        .is_err());
    }
    #[test]
    fn receiver_waits_for_late_kernel_exit_after_service_finish() {
        let (tx, rx) = mpsc::channel();
        for l in [
            "PM: suspend entry (deep)",
            "Low-level resume complete",
            "System returned from sleep operation",
            "Finished System Suspend.",
            "PM: suspend exit",
        ] {
            tx.send(Ok(l.into())).unwrap();
        }
        receive(&rx, false).1.unwrap();
    }
    #[test]
    fn retained_rtc_requires_actual_calendar_and_new_marker() {
        let s="rtc_date=2023-04-10\nrtc_time=00:00:02\nlast_action=17\nlast_value=3\nlast_start=N\nmarkers=56";
        assert_eq!(validate_rtc(s, 0).unwrap(), 56);
        for b in [
            s.replace("2023-04-10", "2026-10-31"),
            s.replace("markers=56", "markers=0"),
            s.replace("last_start=N", "last_start=Y"),
            s.replace("last_action=17", "last_action=10"),
        ] {
            assert!(validate_rtc(&b, 0).is_err());
        }
    }
    #[test]
    fn no_duplicate_or_missing_history() {
        let d="PM: suspend entry (deep)\nPM: suspend debug: Waiting for 5 second(s).\nPM: suspend exit\n";
        history(&d.repeat(9), None).unwrap();
        assert!(history(&d.repeat(8), None).is_err());
        let s = format!("{}RTC BEGIN id=cal kind=calibration-core\n{d}", d.repeat(9));
        history(&s, Some("cal")).unwrap();
        assert!(history(&(s + "RTC BEGIN id=extra\n"), Some("cal")).is_err());
    }
}
