//! November 2: console-suspended, recorder-off RTC-only S3 control.
//! Forked protocol; historical runtime/source/guards remain unchanged.
mod boundary_record;
use boundary_record::{Result, fields, require};
use serde_json::{Value, json};
use std::{
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const BOOT: &str = "7f8080f5-9d58-47ef-b529-598d493e0d43";
const JB: &str = "7f8080f59d5847efb529598d493e0d43";
const OLD_BOOT: &str = "39d7579b-b86d-4d30-b910-131943e550c8";
const BTIME: u64 = 1793566604;
const STATE: &str = "/var/lib/predator-pm-console-control";
const CONSOLE: &str = "/sys/module/printk/parameters/console_suspend";
const SYSTEM: &str =
    "/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363";
const KERNEL: &str = "/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage";
const MODULES: &str = "/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules";
const CORE_REVIEW: &str = "/var/lib/predator-pm-efi-s3-reviewed/core-control-review.json";
const CORE_REVIEW_HASH: &str = "78a64967499872a4cd565d4c4788402b55432fb418c884b9c297d6a93af31896";
const PREVIOUS: &str = "/var/lib/predator-pm-efi-s3-reviewed/s3-efi-v2-20261101";
const COMPLETION_HASH: &str = "4e303475cf76fe2f972237f93ba15d18c5a77c6a52a9f6685e284eb1dbc96121";
const ALARM: &str = "/sys/class/rtc/rtc0/wakealarm";
const RETAINED: &str =
    "/sys/firmware/efi/efivars/PMS3-20261101-a-47d95147-3665-4bda-8ddf-d48f0e339023";
const RETAINED_HASH: &str = "176cbbc62c46cd68c977a07d6c3f775d770ff0983176190646c17f9c18d37b63";
const HANDOFF: &str =
    "/nix/store/qzs6qmgvvka19nv4dyihz9rc8mbybkkv-predator-pm-study-0.1.0/bin/predator-pm-study";
fn text(p: impl AsRef<Path>) -> Result<String> {
    let p = p.as_ref();
    Ok(fs::read_to_string(p)
        .map_err(|e| format!("{}: {e}", p.display()))?
        .trim()
        .into())
}
fn now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
fn window() -> Result<()> {
    require(
        (1793602800..1793656800).contains(&now()?),
        "outside November2 08:00-23:00 CET",
    )
}
fn out(p: &str, a: &[&str]) -> Result<String> {
    let v = Command::new("timeout")
        .arg("90s")
        .arg(p)
        .args(a)
        .env("TZ", "Europe/Paris")
        .output()?;
    require(
        v.status.success(),
        format!(
            "{p} {a:?}: {} {}",
            v.status,
            String::from_utf8_lossy(&v.stderr)
        ),
    )?;
    Ok(String::from_utf8(v.stdout)?.trim().into())
}
fn hash(p: &str) -> Result<String> {
    Ok(out("sha256sum", &[p])?
        .split_whitespace()
        .next()
        .ok_or("hash missing")?
        .into())
}
fn regular(p: &str) -> Result<String> {
    let m = fs::symlink_metadata(p)?;
    require(
        m.is_file() && m.uid() == 0 && m.mode() & 0o777 == 0o600,
        "expected root regular0600",
    )?;
    text(p)
}
fn quiet() -> Result<()> {
    let v = out(
        "systemctl",
        &[
            "list-units",
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
    require(v.is_empty(), format!("maintenance active: {v}"))?;
    require(
        Command::new("pgrep")
            .args(["-G", "nixbld"])
            .status()?
            .code()
            == Some(1),
        "builder active/inspection failed",
    )
}
fn inactive_values(state: &str, pid: &str, jobs: &str) -> Result<()> {
    require(
        state == "inactive" && pid == "0" && jobs.trim().is_empty(),
        "active/queued sleep; cleanup forbidden",
    )
}
fn inactive() -> Result<()> {
    inactive_values(
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
        &out("systemctl", &["list-jobs", "--no-legend"])?,
    )
}
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
    fn clock_adjtime(clock: i32, t: *mut Timex) -> i32;
}
fn clock() -> Result<Timex> {
    require(
        cfg!(all(target_arch = "x86_64", target_os = "linux"))
            && std::mem::size_of::<Timex>() == 208,
        "timex ABI",
    )?;
    let mut t = Timex::default();
    require(
        unsafe { clock_adjtime(0, &mut t) } >= 0,
        "clock status failed",
    )?;
    Ok(t)
}
fn normal_clock() -> Result<()> {
    require(
        text("/sys/class/rtc/rtc0/since_epoch")?
            .parse::<u64>()?
            .abs_diff(now()?)
            <= 2
            && text("/etc/adjtime")?.lines().last() == Some("UTC")
            && clock()?.status & 0x40 == 0,
        "RTC/UTC/UNSYNC invariant",
    )
}
fn kernel() -> Result<String> {
    out("journalctl", &["-b", JB, "-k", "--no-pager", "-o", "json"])
}
// /proc/stat btime is a wall-clock estimate, not a boot identifier. The core
// control demonstrated a one-second change across NTP handoff on the same boot.
fn boot_identity(boot: &str, estimate: u64) -> Result<()> {
    require(
        boot == BOOT && estimate.abs_diff(BTIME) <= 2,
        "boot UUID changed or btime exceeds reviewed two-second tolerance",
    )
}
fn reviewed_core(v: &Value) -> Result<()> {
    require(
        v["result"] == "CORE_TRANSPORT_INDEPENDENTLY_VERIFIED"
            && v["boot"] == OLD_BOOT
            && v["successAfter"] == 1
            && v["originalHarnessResult"] == "POST_CLEANUP_EXACT_BTIME_GUARD_FAILURE"
            && v["originalCompletionAbsent"] == true
            && v["cleanupVerified"] == true
            && v["fullHealthVerified"] == true,
        "core review is incomplete or changed",
    )
}
fn recorders_values(loaded: &[bool], rtc_armed: &str, markers: &str, trace: &str) -> Result<()> {
    require(
        !loaded.iter().any(|v| *v) && rtc_armed == "N" && markers == "0" && trace == "0",
        "recorder-off invariant violated",
    )
}
fn recorders_absent() -> Result<()> {
    recorders_values(
        &[
            "pm_boundary_stage",
            "pm_boundary_compact",
            "pm_boundary_probe",
        ]
        .map(|n| Path::new(&format!("/sys/module/{n}")).exists()),
        &text("/sys/module/pm_stage/parameters/armed")?,
        &text("/sys/module/pm_stage/parameters/markers")?,
        &text("/sys/power/pm_trace")?,
    )
}
fn reviewed_history(current: &str, after: bool) -> Result<()> {
    let mut by_cursor = std::collections::BTreeMap::new();
    let mut messages = Vec::new();
    for line in current.lines() {
        let v: Value = serde_json::from_str(line)?;
        require(
            v["_BOOT_ID"] == JB && v["_TRANSPORT"] == "kernel",
            "history boot/transport",
        )?;
        messages.push(v["MESSAGE"].as_str().ok_or("history message")?.to_owned());
        let c = v["__CURSOR"].as_str().ok_or("history cursor")?.to_owned();
        require(by_cursor.insert(c, v).is_none(), "duplicate history cursor")?;
    }
    for (needle, count) in [
        ("PM: suspend entry", usize::from(after)),
        ("PM: suspend entry (deep)", usize::from(after)),
        ("PM: suspend exit", usize::from(after)),
        ("PM: suspend debug:", 0),
        ("Low-level resume complete", usize::from(after)),
        ("BOUNDARY BEGIN", 0),
        ("CONSOLE_CONTROL BEGIN", usize::from(after)),
        (
            "CONSOLE_CONTROL BEGIN id=console-s3-20261102 nonce=cons1102a",
            usize::from(after),
        ),
    ] {
        require(
            messages.iter().filter(|m| m.contains(needle)).count() == count,
            format!("reviewed history changed: {needle}"),
        )?;
    }
    require(
        !messages.iter().any(|m| {
            [
                "RECORDER_OFF BEGIN",
                "PM: suspend-to-idle",
                "PM: hibernation",
                "pm_boundary_stage",
                "pm_boundary_compact",
                "pm_boundary_probe",
                "BUG:",
                "Oops:",
                "Call Trace:",
            ]
            .iter()
            .any(|s| m.contains(s))
        }),
        "unexpected prior attempt/error",
    )?;
    require(!by_cursor.is_empty(), "empty boot history")?;
    Ok(())
}
fn recorder_off_cycle(journal: &str) -> Result<Value> {
    for line in journal.lines() {
        let v: Value = serde_json::from_str(line)?;
        if v["_TRANSPORT"] == "kernel" {
            let m = v["MESSAGE"].as_str().ok_or("kernel message")?;
            require(
                ![
                    "pm_boundary_stage",
                    "pm_boundary_compact",
                    "pm_boundary_probe",
                ]
                .iter()
                .any(|n| m.contains(n)),
                "unexpected boundary recorder activity",
            )?;
        }
    }
    boundary_record::cycle(journal, BOOT, "s3")
}
fn owned_alarm(alarm: &str, intent: bool, target: u64) -> Result<()> {
    require(
        alarm.is_empty() || (intent && alarm == target.to_string()),
        "foreign RTC alarm",
    )
}
fn owned_console(value: &str, intent: bool, requested: bool) -> Result<()> {
    require(
        (intent && value == "Y") || (!requested && value == "N"),
        "foreign/unowned console value; cleanup forbidden",
    )
}
fn cleanup_gate(requested: bool, verified_return: bool, started: bool) -> Result<()> {
    require(
        !started && (!requested || verified_return),
        "unverified return or cleanup already attempted",
    )
}
struct Plan {
    m: std::collections::BTreeMap<String, String>,
    due: u64,
    rtc: u64,
    prior: u64,
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
            "nonce",
            "boot",
            "btime",
            "mode",
            "due",
            "rtc",
            "review",
            "runtime",
            "runtime_sha",
            "source_sha",
            "recorder",
            "efi_writes",
            "success_before",
            "core_review_sha",
            "prior_completion_sha",
            "meter_scope",
            "hypothesis",
            "prediction",
            "negative_prediction",
            "rollback",
            "console_original",
            "console_trial",
            "retention_sha",
            "owner_ready",
            "pm_async",
            "pm_trace",
            "pm_test",
        ];
        require(
            m.len() == keys.len() && keys.iter().all(|k| m.contains_key(*k)),
            "manifest fields",
        )?;
        for (k, v) in [
            ("schema", "console-control-s3-20261102-1"),
            ("boot", BOOT),
            ("btime", "1793566604"),
            ("recorder", "absent"),
            ("efi_writes", "0"),
            ("core_review_sha", CORE_REVIEW_HASH),
            ("prior_completion_sha", COMPLETION_HASH),
            (
                "meter_scope",
                "owner-confirmed-identical-laptop-and-peripherals",
            ),
            ("hypothesis", "active-consoles-may-perturb-S3-power-entry"),
            (
                "prediction",
                "near3W-and-return-support-console-sensitivity-not-unique-causality",
            ),
            (
                "negative_prediction",
                "15W-nonreturn-shows-active-consoles-not-necessary-for-this-failure",
            ),
            ("console_original", "N"),
            ("console_trial", "Y"),
            (
                "rollback",
                "owned-consoleN-RTC-clear-async1-after-verified-return-and-inactive-sleep",
            ),
            ("retention_sha", RETAINED_HASH),
            ("owner_ready", "owner-confirmed-0915-1030-20261102"),
            ("pm_async", "0"),
            ("pm_trace", "0"),
        ] {
            require(m[k] == v, format!("invalid {k}"))?;
        }
        for (k, v) in [
            ("id", "console-s3-20261102"),
            ("nonce", "cons1102a"),
            ("mode", "s3"),
            ("pm_test", "none"),
            ("success_before", "0"),
        ] {
            require(m[k] == v, format!("invalid {k}"))?;
        }
        for k in ["runtime_sha", "source_sha"] {
            require(
                m[k].len() == 64 && m[k].bytes().all(|b| b.is_ascii_hexdigit()),
                "bad hash",
            )?;
        }
        require(
            m["runtime"].starts_with("/nix/store/")
                && m["runtime"].ends_with("/bin/predator-pm-console-control"),
            "mutable/wrong runtime",
        )?;
        let due = m["due"].parse()?;
        let rtc = m["rtc"].parse()?;
        let review: u64 = m["review"].parse()?;
        require(
            (1793609400..=1793610000).contains(&due)
                && rtc == due + 300
                && review == 1793611800
                && due + 480 < review,
            "trial timing",
        )?;
        Ok(Self {
            m,
            due,
            rtc,
            prior: 0,
        })
    }
    fn path(&self, s: &str) -> String {
        format!("{STATE}/{}-{s}", self.get("id"))
    }
    fn exists(&self, s: &str) -> bool {
        Path::new(&self.path(s)).exists()
    }
    fn save(&self, s: &str, data: &str) -> Result<()> {
        let mut f = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(self.path(s))?;
        f.write_all(data.as_bytes())?;
        f.sync_all()?;
        fs::File::open(STATE)?.sync_all()?;
        Ok(())
    }
    fn identity(&self) -> Result<()> {
        window()?;
        let observed_btime = text("/proc/stat")?
            .lines()
            .find_map(|l| l.strip_prefix("btime ").map(str::to_owned))
            .ok_or("missing btime")?
            .parse::<u64>()?;
        boot_identity(&text("/proc/sys/kernel/random/boot_id")?, observed_btime)?;
        if observed_btime != BTIME {
            eprintln!(
                "Unchanged boot UUID; wall-clock btime estimate {observed_btime} differs from reviewed anchor {BTIME} within the explicit two-second clock-synchronization tolerance"
            );
        }
        for (p, v) in [
            ("/run/current-system", SYSTEM),
            ("/run/booted-system", SYSTEM),
            ("/run/current-system/kernel", KERNEL),
            ("/run/booted-system/kernel", KERNEL),
            ("/run/current-system/kernel-modules", MODULES),
            ("/run/booted-system/kernel-modules", MODULES),
        ] {
            require(
                fs::canonicalize(p)? == Path::new(v),
                format!("frozen identity {p}"),
            )?;
        }
        require(
            env::current_exe()? == Path::new(self.get("runtime"))
                && hash(self.get("runtime"))? == self.get("runtime_sha")
                && hash(RETAINED)? == RETAINED_HASH,
            "binary/retention pin",
        )?;
        require(
            hash(HANDOFF)? == "b8d111d32b28e900a2a1a7c620fa4a4590b2ba1e6bfcdf19cf3758038433090b",
            "helper changed",
        )?;
        require(
            text("/proc/cmdline")?
                .split_whitespace()
                .any(|s| s == "no_console_suspend"),
            "frozen console boot argument changed",
        )?;
        require(
            hash(&format!("/var/lib/predator-pm-study/{BOOT}-boot-rtc.txt"))?
                == "d9b84289486044ff0b6c721e0e762c26680bdfb46d98458f9a093f3a1e623d7d",
            "recovery boot RTC anchor changed",
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
                "/var/lib/predator-pm-wake-path/path-d3-v2-20261101-plan.conf",
                "ab8a52f58f39a2dddb7363a0b8d3290a346856c6d9d2dbf9991124be522ea653",
            ),
            (
                "/var/lib/predator-pm-wake-path/path-d3-v2-20261101-completed.txt",
                "584492d7d63f6224de4aa3543559962ae41e49fe118252608d111155fe25e222",
            ),
        ] {
            require(hash(p)? == h, "historical anchor changed")?;
        }
        {
            let path = CORE_REVIEW;
            require(
                hash(path)? == self.get("core_review_sha"),
                "independent core review pin",
            )?;
            let v: Value = serde_json::from_str(&regular(path)?)?;
            reviewed_core(&v)?;
            for (suffix, expected) in [
                (
                    "plan.conf",
                    "95344780cda5a343d3bbe5c757456aff0408d5ae027587d5da0d4215f7f3e5e3",
                ),
                (
                    "observed.jsonl",
                    "73e15c93f69bffc2c5bbfb257faebb098556e45ef9ed9a481b263c4ed7c9de61",
                ),
                (
                    "outcome.txt",
                    "ed6c49b6b4c269651901180e12f4c4a7dff6ca29a1e1d235af6b641f09ac2b4e",
                ),
                (
                    "raw-return.txt",
                    "4e04b945b2e5bed0b61ef755ea8346a04b52b48c9043c0d08060b07235e6a0c4",
                ),
                (
                    "restored.txt",
                    "33c76ad264bf7b7f766935fb0e1122cb56c9618977aba88645baba6cdc84864e",
                ),
                (
                    "stage-status.txt",
                    "8ec9a59873daefb342530ce89044b651a2b3e280d817e260d7d0c7b49af7a48e",
                ),
                (
                    "stage-readback.txt",
                    "27710f1f377a715009da5559cd4830a0290628a4f7d29e40d26bb12b235536de",
                ),
            ] {
                require(
                    hash(&format!(
                        "/var/lib/predator-pm-efi-s3/core-efi-20261101-{suffix}"
                    ))? == expected,
                    "reviewed original core artifact changed",
                )?;
            }
            require(
                !Path::new("/var/lib/predator-pm-efi-s3/core-efi-20261101-completed.json").exists(),
                "original failed harness must not acquire a synthetic completion",
            )?;
            boundary_record::verify_stage(
                &regular("/var/lib/predator-pm-efi-s3/core-efi-20261101-stage-status.txt")?,
                &regular("/var/lib/predator-pm-efi-s3/core-efi-20261101-stage-readback.txt")?,
                "core",
                "core1101a",
                OLD_BOOT,
            )?;
        }
        require(
            hash(&format!("{PREVIOUS}-plan.conf"))?
                == "bb535457f04bf08d1663900ff454339b1521f68e81de6cf1f98b111f2183bd5c"
                && hash(&format!("{PREVIOUS}-completed.json"))? == COMPLETION_HASH
                && hash(&format!("{PREVIOUS}-observed.jsonl"))?
                    == "c763f171292f2add61aa461bb7750d39ecbd78504c1a670b5995f12603b089ca",
            "previous S3 trial anchor changed",
        )?;
        let prior: Value = serde_json::from_str(&regular(&format!("{PREVIOUS}-completed.json"))?)?;
        require(
            prior["boot"] == OLD_BOOT
                && prior["result"] == "S3_LOWLEVEL_RETURN"
                && prior["successBefore"] == 1
                && prior["successAfter"] == 2
                && prior["restoration"] == "PASS",
            "previous S3 completion changed",
        )?;
        Ok(())
    }
    fn timing(&self, run: bool) -> Result<()> {
        window()?;
        let n = now()?;
        require(
            if run {
                (self.due..=self.due + 30).contains(&n)
            } else {
                n + 300 < self.due
            },
            "deadline/lead guard",
        )
    }
}
fn health(p: &Plan, after: bool) -> Result<()> {
    p.identity()?;
    inactive()?;
    let sleep = out("systemd-analyze", &["cat-config", "systemd/sleep.conf"])?;
    let lines: Vec<_> = sleep
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with(';'))
        .collect();
    require(
        lines == ["[Sleep]", "AllowHibernation=false", "AllowSuspend=true"],
        "sleep configuration override",
    )?;
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
        .is_empty()
            && fs::canonicalize("/etc/systemd/system/systemd-suspend.service")?
                == Path::new(
                    "/nix/store/6qqq5prql4952z0wfm5ymjfr7cx4ab7z-systemd-262/example/systemd/system/systemd-suspend.service",
                ),
        "suspend unit changed",
    )?;
    for d in [
        "predator-pm-study",
        "predator-pm-diagnostic",
        "predator-pm-instrumented",
        "predator-pm-s2idle",
        "predator-pm-wake-validation",
        "predator-pm-wake-path",
        "predator-pm-boundary",
        "predator-pm-efi-s3",
        "predator-pm-efi-s3-reviewed",
        "predator-pm-recorder-off",
        "predator-pm-console-control",
    ] {
        require(
            !Path::new(&format!("/var/lib/{d}/HOLD")).exists(),
            "HOLD present",
        )?;
    }
    for (path, v) in [
        (CONSOLE, "N"),
        ("/sys/power/pm_async", "1"),
        ("/sys/power/pm_trace", "0"),
        ("/sys/module/pm_stage/parameters/armed", "N"),
        ("/sys/module/pm_stage/parameters/markers", "0"),
        ("/sys/power/suspend_stats/fail", "0"),
        ("/sys/power/pm_debug_messages", "1"),
        ("/sys/power/pm_print_times", "1"),
        ("/sys/module/kernel/parameters/initcall_debug", "Y"),
        ("/sys/class/power_supply/ACAD/online", "1"),
        ("/sys/class/graphics/fb0/blank", "4"),
        ("/sys/module/suspend/parameters/pm_test_delay", "5"),
        ("/sys/power/mem_sleep", "s2idle [deep]"),
        ("/sys/bus/pci/devices/0000:03:00.1/d3cold_allowed", "1"),
        ("/sys/bus/pci/devices/0000:03:00.1/power/wakeup", "enabled"),
    ] {
        require(text(path)? == v, format!("baseline changed {path}"))?;
    }
    require(
        text("/sys/power/pm_test")?.starts_with("[none]") && text(ALARM)?.is_empty(),
        "test/alarm not ordinary",
    )?;
    require(
        text("/sys/power/suspend_stats/success")?.parse::<u64>()? == p.prior + u64::from(after),
        "cumulative counter changed",
    )?;
    recorders_absent()?;
    require(
        text("/proc/acpi/button/lid/LID0/state")?.ends_with("closed")
            && out("ethtool", &["enp3s0f1"])?
                .lines()
                .any(|l| l.trim() == "Wake-on: g"),
        "lid/WoL",
    )?;
    require(
        Path::new("/dev/tpm0").exists()
            && Path::new("/dev/tpmrm0").exists()
            && fs::canonicalize("/sys/bus/acpi/devices/MSFT0101:00/driver")?
                == Path::new("/sys/bus/acpi/drivers/tpm_crb")
            && !text("/proc/cmdline")?.contains("crb_acpi_driver_init"),
        "TPM/security changed",
    )?;
    for n in ["acer_wmi", "acer_wmi_ph31751", "nouveau", "nvidia"] {
        require(
            !Path::new(&format!("/sys/module/{n}")).exists(),
            "Acer driver loaded",
        )?;
    }
    let ec = out(
        "journalctl",
        &[
            "-b",
            JB,
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
            "boot audit missing",
        )?;
    }
    for u in ["night-suspend.timer", "predator-pm-next.timer"] {
        require(
            out("systemctl", &["show", u, "-p", "ActiveState", "--value"])? == "inactive",
            "conflicting timer",
        )?;
    }
    require(
        out(
            "nmcli",
            &["-g", "GENERAL.CON-UUID", "device", "show", "enp3s0f1"],
        )? == "c4e6863b-7f76-4a4d-9a48-9937af0b5a36"
            && out("nmcli", &["-g", "IP4.DNS", "device", "show", "enp3s0f1"])?
                .lines()
                .any(|l| l == "192.168.1.1"),
        "LAN/DNS",
    )?;
    require(
        out("timedatectl", &["show", "-p", "NTPSynchronized", "--value"])? == "yes",
        "NTP",
    )?;
    normal_clock()?;
    require(
        out("zpool", &["list", "-H", "-o", "name"])? == "fast\ntank"
            && out("zpool", &["status", "-x"])? == "all pools are healthy"
            && !out("zpool", &["status"])?.contains("in progress"),
        "storage/maintenance",
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
            format!("service {u}"),
        )?;
    }
    let containers = out("podman", &["ps", "--format", "{{.Names}} {{.Status}}"])?;
    for n in [
        "immich_server",
        "immich_postgres",
        "immich_redis",
        "immich_ml",
    ] {
        require(
            containers
                .lines()
                .any(|l| l.starts_with(&format!("{n} ")) && l.contains("(healthy)")),
            format!("container {n}"),
        )?;
    }
    require(
        out(
            "systemctl",
            &["list-units", "--failed", "--no-legend", "--plain"],
        )?
        .is_empty()
            && out("systemctl", &["list-jobs", "--no-legend", "--plain"])?.is_empty(),
        "failed units/jobs",
    )?;
    quiet()?;
    reviewed_history(&kernel()?, after)?;
    Ok(())
}
fn snapshot() -> Result<String> {
    recorders_absent()?;
    let mut s = format!(
        "epoch={}\nboot={}\nrtc_epoch={}\nalarm={}\nclock={:?}\nuptime={}\n{}\n{}\n",
        now()?,
        text("/proc/sys/kernel/random/boot_id")?,
        text("/sys/class/rtc/rtc0/since_epoch")?,
        text(ALARM)?,
        clock()?,
        text("/proc/uptime")?,
        out("hwclock", &["--show", "--utc"])?,
        text("/proc/driver/rtc")?
    );
    for f in [
        CONSOLE,
        "/proc/stat",
        "/sys/kernel/debug/wakeup_sources",
        "/proc/interrupts",
        "/proc/acpi/wakeup",
        "/sys/power/pm_async",
        "/sys/power/pm_test",
    ] {
        s += &format!("{f}:\n{}\n", text(f)?);
    }
    for n in [
        "ff_rt_clk",
        "ff_pwr_btn",
        "ff_slp_btn",
        "gpe14",
        "gpe66",
        "gpe69",
        "gpe6D",
        "sci",
        "sci_not",
        "gpe_all",
        "error",
    ] {
        s += &format!(
            "acpi_{n}={}\n",
            text(format!("/sys/firmware/acpi/interrupts/{n}"))?
        );
    }
    for dev in ["0000:00:1c.3", "0000:03:00.0", "0000:03:00.1"] {
        let mut data = [0; 256];
        fs::File::open(format!("/sys/bus/pci/devices/{dev}/config"))?.read_exact(&mut data)?;
        s += &format!(
            "pci_{dev}={}\n",
            data.iter().map(|x| format!("{x:02x}")).collect::<String>()
        );
    }
    match text("/sys/power/pm_wakeup_irq") {
        Ok(v) => s += &format!("wake_irq={v}\n"),
        Err(e) => s += &format!("wake_irq_read_error={e}\n"),
    };
    s += &format!(
        "boundary_recorders=absent\nprotocol_EFI_writes=0\nEFI_capacity:\n{}\nBattery:\n{}\n",
        out("df", &["-B1", "/sys/firmware/efi/efivars"])?,
        text("/sys/class/power_supply/BAT1/uevent")?
    );
    Ok(s)
}
fn marker(s: &str) -> Result<()> {
    println!("{s}");
    writeln!(
        fs::OpenOptions::new().write(true).open("/dev/kmsg")?,
        "<6>pm-console-control: {s}"
    )?;
    Ok(())
}
fn restore(p: &Plan) -> Result<()> {
    p.identity()?;
    inactive()?;
    if !p.exists("mutating.txt") || p.exists("restored.txt") {
        return Ok(());
    }
    cleanup_gate(
        p.exists("request-call.txt"),
        p.exists("return-verified.json"),
        p.exists("cleanup-started.txt"),
    )?;
    if !p.exists("raw-return.txt") {
        p.save("raw-return.txt", &snapshot()?)?;
    }
    recorders_absent()?;
    require(
        text("/sys/power/mem_sleep")? == "s2idle [deep]"
            && text("/sys/module/pm_stage/parameters/armed")? == "N",
        "foreign mode/RTC recorder",
    )?;
    let alarm = text(ALARM)?;
    owned_alarm(&alarm, p.exists("alarm-intent.txt"), p.rtc)?;
    let console = text(CONSOLE)?;
    let intent = p.exists("console-intent.txt")
        && regular(&p.path("console-intent.txt"))? == format!("boot={BOOT}\noriginal=N\ntrial=Y");
    owned_console(&console, intent, p.exists("request-call.txt"))?;
    let test = text("/sys/power/pm_test")?;
    require(test.starts_with("[none]"), "foreign pm_test")?;
    require(
        text("/sys/power/pm_async")? == "0"
            || (!p.exists("request-call.txt") && text("/sys/power/pm_async")? == "1"),
        "foreign async",
    )?;
    p.save(
        "cleanup-started.txt",
        "same boot; inactive/MainPID0/empty queue; all ownership checks passed; no retry",
    )?;
    if !alarm.is_empty() {
        fs::write(ALARM, "0")?;
    }
    if intent {
        fs::write(CONSOLE, p.get("console_original"))?;
    }
    fs::write("/sys/power/pm_test", "none")?;
    fs::write("/sys/power/pm_async", "1")?;
    require(
        text(CONSOLE)? == "N" && text(ALARM)?.is_empty(),
        "owned console/alarm restoration readback",
    )?;
    let helper = out(HANDOFF, &["clock-handoff"])?;
    p.save("clock-handoff.txt", &helper)?;
    normal_clock()?;
    p.save("restored.txt", &snapshot()?)?;
    Ok(())
}
fn run(p: &Plan, manifest: &str) -> Result<()> {
    p.save("requested.txt", manifest)?;
    p.timing(true)?;
    health(p, false)?;
    p.save("before-kernel.jsonl", &kernel()?)?;
    p.save("before.txt", &snapshot()?)?;
    let cursor = out(
        "journalctl",
        &["-b", JB, "-n", "0", "--show-cursor", "--no-pager"],
    )?;
    let cursor = cursor
        .lines()
        .find_map(|l| l.strip_prefix("-- cursor: "))
        .ok_or("journal cursor missing")?
        .to_owned();
    p.save("cursor.txt", &cursor)?;
    let mut reader = Command::new("journalctl")
        .args([
            "-b",
            JB,
            "--after-cursor",
            &cursor,
            "--follow",
            "--no-pager",
            "-o",
            "json",
            "_TRANSPORT=kernel",
            "+",
            "_SYSTEMD_UNIT=systemd-suspend.service",
            "+",
            "UNIT=systemd-suspend.service",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let stdout = reader.stdout.take().ok_or("journal pipe")?;
    let (tx, rx) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        for l in BufReader::new(stdout).lines() {
            if tx.send(l).is_err() {
                break;
            }
        }
    });
    let mut captured = String::new();
    let operation = (|| -> Result<()> {
        p.save(
            "mutating.txt",
            "original async=1; console=N; alarm empty; owns serial async, console and RTC only after saved intents; no EFI writes or module operations",
        )?;
        recorders_absent()?;
        fs::write("/sys/power/pm_async", "0")?;
        fs::write("/sys/power/pm_test", p.get("pm_test"))?;
        require(
            text("/sys/power/pm_async")? == "0"
                && text("/sys/power/pm_test")?.contains(&format!("[{}]", p.get("pm_test"))),
            "mode readback",
        )?;
        inactive()?;
        quiet()?;
        p.identity()?;
        p.timing(true)?;
        normal_clock()?;
        recorders_absent()?;
        require(text(CONSOLE)? == "N", "console changed before intent")?;
        p.save(
            "console-intent.txt",
            &format!("boot={BOOT}\noriginal=N\ntrial=Y"),
        )?;
        fs::write(CONSOLE, p.get("console_trial"))?;
        require(text(CONSOLE)? == "Y", "console trial readback")?;
        marker(&format!(
            "CONSOLE_CONTROL BEGIN id={} nonce={} mode={} success_before={} RTC={} boot={BOOT} EFI_writes=0 console_suspend=Y",
            p.get("id"),
            p.get("nonce"),
            p.get("mode"),
            p.prior,
            p.rtc
        ))?;
        if p.rtc != 0 {
            require(text(ALARM)?.is_empty(), "foreign alarm before arming")?;
            p.save("alarm-intent.txt", &p.rtc.to_string())?;
            fs::write(ALARM, p.rtc.to_string())?;
            require(
                text(ALARM)? == p.rtc.to_string()
                    && text("/proc/driver/rtc")?
                        .lines()
                        .any(|l| l.starts_with("alarm_IRQ") && l.ends_with("yes")),
                "RTC alarm readback/IRQ",
            )?;
            p.save("alarm-armed.txt", &snapshot()?)?;
        }
        out("journalctl", &["--sync"])?;
        p.save(
            "request-call.txt",
            "logind SuspendWithFlags(1), inhibitors enforced",
        )?;
        let response = out(
            "busctl",
            &[
                "call",
                "org.freedesktop.login1",
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
                "SuspendWithFlags",
                "t",
                "1",
            ],
        )?;
        p.save("request-ok.txt", &format!("true\n{response}"))?;
        let deadline = Instant::now() + Duration::from_secs(660);
        let (mut exit, mut finished) = (false, false);
        while !(exit && finished) {
            let line = rx.recv_timeout(
                deadline
                    .checked_duration_since(Instant::now())
                    .ok_or("return deadline")?,
            )??;
            captured += &line;
            captured.push('\n');
            require(
                captured.len() < 16 * 1024 * 1024,
                "journal evidence overflow",
            )?;
            let v: Value = serde_json::from_str(&line)?;
            require(v["_BOOT_ID"] == JB, "observed boot changed")?;
            let m = v["MESSAGE"].as_str().ok_or("journal message")?;
            if v["_TRANSPORT"] == "kernel" && m.contains("PM: suspend exit") {
                exit = true;
            }
            if v["UNIT"] == "systemd-suspend.service" && m.contains("Finished System Suspend") {
                finished = true;
            }
        }
        inactive()?;
        p.save("raw-return.txt", &snapshot()?)?;
        require(text(CONSOLE)? == "Y", "console changed during trial")?;
        require(
            text("/sys/power/suspend_stats/success")? == "1"
                && text("/sys/power/suspend_stats/fail")? == "0",
            "return counters",
        )?;
        reviewed_history(&kernel()?, true)?;
        p.save(
            "return-verified.json",
            &serde_json::to_string_pretty(&recorder_off_cycle(&captured)?)?,
        )?;
        Ok(())
    })();
    let _ = reader.kill();
    let wait = reader.wait();
    let join = thread.join();
    let evidence = p.save("observed.jsonl", &captured);
    let cleanup = if operation.is_ok() || !p.exists("request-call.txt") {
        restore(p)
    } else {
        Err("no verified return after request; cleanup deliberately not attempted".into())
    };
    p.save("outcome.txt",&format!("operation={operation:?}\nevidence={evidence:?}\ncleanup={cleanup:?}\nreader={wait:?}\njoin_ok={}\n",join.is_ok()))?;
    operation?;
    evidence?;
    cleanup?;
    health(p, true)?;
    let cycle = recorder_off_cycle(&captured)?;
    p.save(
        "verification.json",
        &serde_json::to_string_pretty(
            &json!({"recordersAbsent":true,"protocolEfiWrites":0,"cycle":cycle}),
        )?,
    )?;
    p.save("after-kernel.jsonl", &kernel()?)?;
    let result = "CONSOLE_CONTROL_S3_LOWLEVEL_RETURN";
    p.save("completed.json",&serde_json::to_string_pretty(&json!({"result":result,"boot":BOOT,"btime":BTIME,"id":p.get("id"),"successBefore":p.prior,"successAfter":p.prior+1,"completedEpoch":now()?,"physicalSleepSeconds":null,"rtcTarget":p.rtc,"restoration":"PASS"}))?)?;
    marker(&format!(
        "CONSOLE_CONTROL RETURN id={} result={result} physical_duration_pending EFI_writes=0",
        p.get("id")
    ))?;
    Ok(())
}
fn main() -> Result<()> {
    let a: Vec<_> = env::args().collect();
    require(a.len() == 4, "usage: check|run|restore MANIFEST SHA")?;
    require(out("id", &["-u"])? == "0", "root required")?;
    window()?;
    require(hash(&a[2])? == a[3], "manifest hash")?;
    let manifest = regular(&a[2])?;
    let p = Plan::parse(&manifest)?;
    require(
        a[2] == format!("{STATE}/{}-plan.conf", p.get("id")),
        "manifest location",
    )?;
    match a[1].as_str() {
        "check" => {
            p.timing(false)?;
            health(&p, false)?;
            snapshot()?;
            marker(&format!(
                "CONSOLE_CONTROL READY id={} nonce={} boot={BOOT}",
                p.get("id"),
                p.get("nonce")
            ))
        }
        "run" => run(&p, &manifest),
        "restore" => restore(&p),
        _ => Err("unknown command".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleanup_never_crosses_active_sleep() {
        inactive_values("inactive", "0", "").unwrap();
        for (s, p, j) in [
            ("active", "1", ""),
            ("inactive", "3", ""),
            ("inactive", "0", "2 suspend.target start"),
        ] {
            assert!(inactive_values(s, p, j).is_err());
        }
    }
    #[test]
    fn clock_layout() {
        assert_eq!(std::mem::size_of::<Timex>(), 208);
        assert_eq!(std::mem::offset_of!(Timex, status), 40);
    }
    #[test]
    fn manifest_rejects_replay_recorder_wrong_console_and_old_history() {
        let s = format!(
            "schema=console-control-s3-20261102-1\nid=console-s3-20261102\nnonce=cons1102a\nboot={BOOT}\nbtime={BTIME}\nmode=s3\ndue=1793609400\nrtc=1793609700\nreview=1793611800\nruntime=/nix/store/example/bin/predator-pm-console-control\nruntime_sha={}\nsource_sha={}\nrecorder=absent\nefi_writes=0\nsuccess_before=0\ncore_review_sha={CORE_REVIEW_HASH}\nprior_completion_sha={COMPLETION_HASH}\nmeter_scope=owner-confirmed-identical-laptop-and-peripherals\nhypothesis=active-consoles-may-perturb-S3-power-entry\nprediction=near3W-and-return-support-console-sensitivity-not-unique-causality\nnegative_prediction=15W-nonreturn-shows-active-consoles-not-necessary-for-this-failure\nconsole_original=N\nconsole_trial=Y\nrollback=owned-consoleN-RTC-clear-async1-after-verified-return-and-inactive-sleep\nretention_sha={RETAINED_HASH}\nowner_ready=owner-confirmed-0915-1030-20261102\npm_async=0\npm_trace=0\npm_test=none\n",
            "a".repeat(64),
            "b".repeat(64)
        );
        Plan::parse(&s).unwrap();
        for (a, b) in [
            ("success_before=0", "success_before=2"),
            ("console_original=N", "console_original=Y"),
            ("console_trial=Y", "console_trial=N"),
            ("recorder=absent", "recorder=present"),
            ("efi_writes=0", "efi_writes=3"),
            (
                "meter_scope=owner-confirmed-identical-laptop-and-peripherals",
                "meter_scope=unknown",
            ),
            ("mode=s3", "mode=core"),
            ("pm_test=none", "pm_test=core"),
            ("rtc=1793609700", "rtc=1793609701"),
            ("pm_async=0", "pm_async=1"),
            ("due=1793609400", "due=1793609399"),
            ("due=1793609400", "due=1793610001"),
            ("review=1793611800", "review=1793611801"),
            ("core_review_sha=", "core_completion_sha="),
            (BOOT, "bf51c3ac-164e-4bfa-b56c-7b71df526b7f"),
        ] {
            assert!(Plan::parse(&s.replace(a, b)).is_err());
        }
        assert!(Plan::parse(&(s + "pm_test=core\n")).is_err());
    }
    #[test]
    fn no_recorder_or_foreign_alarm_is_accepted() {
        recorders_values(&[false, false, false], "N", "0", "0").unwrap();
        for i in 0..3 {
            let mut a = [false; 3];
            a[i] = true;
            assert!(recorders_values(&a, "N", "0", "0").is_err());
        }
        for (a, m, t) in [("Y", "0", "0"), ("N", "1", "0"), ("N", "0", "1")] {
            assert!(recorders_values(&[false; 3], a, m, t).is_err());
        }
        owned_alarm("", false, 123).unwrap();
        owned_alarm("123", true, 123).unwrap();
        assert!(owned_alarm("123", false, 123).is_err());
        assert!(owned_alarm("124", true, 123).is_err());
    }
    fn journal(messages: &[&str]) -> String {
        messages.iter().enumerate().map(|(i,m)| json!({"_BOOT_ID":JB,"_TRANSPORT":"kernel","__CURSOR":format!("cursor{i}"),"MESSAGE":m,"_SOURCE_MONOTONIC_TIMESTAMP":(100+i*100).to_string(),"_SOURCE_BOOTTIME_TIMESTAMP":(100+i*100).to_string(),"__MONOTONIC_TIMESTAMP":(100+i).to_string(),"__REALTIME_TIMESTAMP":(1000+i).to_string()}).to_string()+"\n").collect()
    }
    #[test]
    fn requires_clean_recovery_boot_then_only_this_single_cycle() {
        reviewed_history(&journal(&["Linux version 6.18.58"]), false).unwrap();
        let j = journal(&[
            "CONSOLE_CONTROL BEGIN id=console-s3-20261102 nonce=cons1102a",
            "PM: suspend entry (deep)",
            "ACPI: PM: Low-level resume complete",
            "PM: suspend exit",
        ]);
        reviewed_history(&j, true).unwrap();
        assert!(reviewed_history(&j, false).is_err());
        assert!(reviewed_history("", false).is_err());
        for s in [
            j.replace("(deep)", "(s2idle)"),
            j.replace(JB, "wrong"),
            j.replace("cons1102a", "othernonce"),
            format!("{j}{j}"),
        ] {
            assert!(reviewed_history(&s, true).is_err());
        }
        for m in [
            "BOUNDARY BEGIN",
            "RECORDER_OFF BEGIN",
            "PM: suspend entry (deep)",
            "BUG:",
            "pm_boundary_stage",
        ] {
            assert!(reviewed_history(&journal(&[m]), false).is_err());
        }
    }
    #[test]
    fn new_cycle_requires_source_clocks_and_no_boundary_activity() {
        let j = journal(&[
            "PM: suspend entry (deep)",
            "ACPI: PM: Low-level resume complete",
            "PM: suspend exit",
        ]);
        let v = recorder_off_cycle(&j).unwrap();
        assert_eq!(
            v["entryExitIntervalsMicros"]["_SOURCE_MONOTONIC_TIMESTAMP"],
            200
        );
        assert_eq!(v["entryExitIntervalsMicros"]["__MONOTONIC_TIMESTAMP"], 2);
        assert!(recorder_off_cycle(&j.replace("_SOURCE_BOOTTIME_TIMESTAMP", "missing")).is_err());
        for n in [
            "pm_boundary_stage",
            "pm_boundary_compact",
            "pm_boundary_probe",
        ] {
            let extra = journal(&[n]);
            assert!(recorder_off_cycle(&(j.clone() + &extra)).is_err());
        }
    }
    #[test]
    fn boot_identity_allows_only_bounded_wall_clock_estimate_change() {
        for t in BTIME - 2..=BTIME + 2 {
            boot_identity(BOOT, t).unwrap();
        }
        assert!(boot_identity(BOOT, BTIME + 3).is_err());
        assert!(boot_identity(BOOT, BTIME - 3).is_err());
        assert!(boot_identity("bf51c3ac-164e-4bfa-b56c-7b71df526b7f", BTIME).is_err());
    }
    #[test]
    fn independent_review_does_not_relabel_original_harness_failure() {
        let v = json!({"result":"CORE_TRANSPORT_INDEPENDENTLY_VERIFIED","boot":OLD_BOOT,"successAfter":1,
            "originalHarnessResult":"POST_CLEANUP_EXACT_BTIME_GUARD_FAILURE","originalCompletionAbsent":true,
            "cleanupVerified":true,"fullHealthVerified":true});
        reviewed_core(&v).unwrap();
        for (key, bad) in [
            ("result", json!("CORE_TRANSPORT_PASS")),
            ("boot", json!("other")),
            ("successAfter", json!(0)),
            ("originalCompletionAbsent", json!(false)),
            ("cleanupVerified", json!(false)),
            ("fullHealthVerified", json!(false)),
            ("originalHarnessResult", json!("PASS")),
        ] {
            let mut changed = v.clone();
            changed[key] = bad;
            assert!(reviewed_core(&changed).is_err());
        }
    }
    #[test]
    fn console_ownership_and_once_only_verified_return_cleanup() {
        owned_console("N", false, false).unwrap();
        owned_console("N", true, false).unwrap();
        owned_console("Y", true, true).unwrap();
        for (v, intent, requested) in [
            ("Y", false, false),
            ("N", true, true),
            ("Y", false, true),
            ("foreign", true, true),
        ] {
            assert!(owned_console(v, intent, requested).is_err());
        }
        cleanup_gate(false, false, false).unwrap();
        cleanup_gate(true, true, false).unwrap();
        assert!(cleanup_gate(true, false, false).is_err());
        assert!(cleanup_gate(true, true, true).is_err());
        assert!(cleanup_gate(false, false, true).is_err());
    }
}
