//! November 1: one RTC-only S3 trial anchored to the separately reviewed core control.
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
const BOOT: &str = "39d7579b-b86d-4d30-b910-131943e550c8";
const JB: &str = "39d7579bb86d4d30b910131943e550c8";
const BTIME: u64 = 1793535079;
const STATE: &str = "/var/lib/predator-pm-efi-s3-reviewed";
const SYSTEM: &str =
    "/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363";
const KERNEL: &str = "/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage";
const MODULES: &str = "/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules";
const STAGE: &str = "/nix/store/ymy989dq8bxys9j2jkanpc3d80qmvliy-predator-pm-boundary-stage-20261101/lib/modules/6.18.58/extra/pm_boundary_stage.ko.xz";
const STAGE_HASH: &str = "90535c76374d5e2c29ba03780949b8917084c01fb6a610c0427a32e3471627c0";
const PARAM: &str = "/sys/module/pm_boundary_stage/parameters";
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
        (1793516400..1793570400).contains(&now()?),
        "outside November1 08:00-23:00 CET",
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
        state == "inactive" && pid == "0" && !jobs.contains("sleep") && !jobs.contains("suspend"),
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
            && v["boot"] == BOOT
            && v["successAfter"] == 1
            && v["originalHarnessResult"] == "POST_CLEANUP_EXACT_BTIME_GUARD_FAILURE"
            && v["originalCompletionAbsent"] == true
            && v["cleanupVerified"] == true
            && v["fullHealthVerified"] == true,
        "core review is incomplete or changed",
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
            "module",
            "module_sha",
            "success_before",
            "core_review_sha",
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
            ("schema", "efi-s3-reviewed-20261101-2"),
            ("boot", BOOT),
            ("btime", "1793535079"),
            ("module", STAGE),
            ("module_sha", STAGE_HASH),
            ("retention_sha", RETAINED_HASH),
            ("owner_ready", "owner-confirmed-ready-20261101"),
            ("pm_async", "0"),
            ("pm_trace", "0"),
        ] {
            require(m[k] == v, format!("invalid {k}"))?;
        }
        let core = m["mode"] == "core";
        require(!core && m["mode"] == "s3", "reviewed S3-only mode")?;
        for (k, v) in [
            (
                "id",
                if core {
                    "core-efi-20261101"
                } else {
                    "s3-efi-v2-20261101"
                },
            ),
            ("nonce", if core { "core1101a" } else { "deep1101a" }),
            ("pm_test", if core { "core" } else { "none" }),
            ("success_before", if core { "0" } else { "1" }),
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
                && m["runtime"].ends_with("/bin/predator-pm-boundary-s3"),
            "mutable/wrong runtime",
        )?;
        let due = m["due"].parse()?;
        let rtc = m["rtc"].parse()?;
        let review: u64 = m["review"].parse()?;
        require(
            (1793535600..=1793566800).contains(&due)
                && rtc == if core { 0 } else { due + 300 }
                && review == due + if core { 900 } else { 600 }
                && review < 1793570400,
            "trial timing",
        )?;
        require(
            if core {
                m["core_review_sha"] == "none"
            } else {
                m["core_review_sha"].len() == 64
                    && m["core_review_sha"].bytes().all(|b| b.is_ascii_hexdigit())
            },
            "core review anchor",
        )?;
        Ok(Self {
            m,
            due,
            rtc,
            prior: if core { 0 } else { 1 },
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
                && hash(STAGE)? == STAGE_HASH
                && hash(RETAINED)? == RETAINED_HASH,
            "binary/module/retention pin",
        )?;
        require(
            hash(HANDOFF)? == "b8d111d32b28e900a2a1a7c620fa4a4590b2ba1e6bfcdf19cf3758038433090b",
            "helper changed",
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
        if self.prior == 1 {
            let path = format!("{STATE}/core-control-review.json");
            require(
                hash(&path)? == self.get("core_review_sha"),
                "independent core review pin",
            )?;
            let v: Value = serde_json::from_str(&regular(&path)?)?;
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
                BOOT,
            )?;
        }
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
    ] {
        require(
            !Path::new(&format!("/var/lib/{d}/HOLD")).exists(),
            "HOLD present",
        )?;
    }
    for (path, v) in [
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
    require(
        !Path::new(PARAM).exists()
            && !Path::new("/sys/module/pm_boundary_compact").exists()
            && !Path::new("/sys/module/pm_boundary_probe").exists(),
        "probe already loaded",
    )?;
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
    for n in ["acer_wmi", "acer_wmi_ph31751"] {
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
    if !after {
        let j = kernel()?;
        let messages: Vec<String> = j
            .lines()
            .map(|l| -> Result<String> {
                let v: Value = serde_json::from_str(l)?;
                require(v["_BOOT_ID"] == JB, "prior journal boot")?;
                Ok(v["MESSAGE"].as_str().ok_or("kernel text")?.into())
            })
            .collect::<Result<_>>()?;
        for needle in [
            "PM: suspend entry (deep)",
            "PM: suspend exit",
            "PM: suspend debug:",
            "BOUNDARY BEGIN",
        ] {
            require(
                messages.iter().filter(|m| m.contains(needle)).count() == p.prior as usize,
                format!("unexpected prior history {needle}"),
            )?;
        }
        require(
            !messages.iter().any(|m| {
                m.contains("Low-level resume complete")
                    || m.contains("PM: suspend-to-idle")
                    || m.contains("BUG:")
                    || m.contains("Oops:")
            }),
            "unexpected prior low-level history",
        )?;
    }
    Ok(())
}
fn snapshot() -> Result<String> {
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
    Ok(s)
}
fn marker(s: &str) -> Result<()> {
    println!("{s}");
    writeln!(
        fs::OpenOptions::new().write(true).open("/dev/kmsg")?,
        "<6>pm-boundary: {s}"
    )?;
    Ok(())
}
fn restore(p: &Plan) -> Result<()> {
    p.identity()?;
    inactive()?;
    if !p.exists("mutating.txt") || p.exists("restored.txt") {
        return Ok(());
    }
    if !p.exists("raw-return.txt") {
        p.save("raw-return.txt", &snapshot()?)?;
    }
    require(
        text("/sys/power/mem_sleep")? == "s2idle [deep]"
            && text("/sys/module/pm_stage/parameters/armed")? == "N",
        "foreign mode/RTC recorder",
    )?;
    let alarm = text(ALARM)?;
    require(
        alarm.is_empty()
            || (p.rtc != 0 && p.exists("alarm-intent.txt") && alarm == p.rtc.to_string()),
        "foreign RTC alarm",
    )?;
    if !alarm.is_empty() {
        fs::write(ALARM, "0")?;
    }
    if Path::new(PARAM).exists() {
        require(
            text(format!("{PARAM}/nonce"))? == p.get("nonce")
                && text(format!("{PARAM}/boot_id"))? == BOOT
                && text(format!("{PARAM}/mode"))? == p.get("mode"),
            "foreign phase module",
        )?;
        fs::write(format!("{PARAM}/armed"), "N")?;
        if !p.exists("stage-status.txt") {
            p.save("stage-status.txt", &text(format!("{PARAM}/status"))?)?;
        }
        if !p.exists("stage-readback.txt") && !p.exists("stage-readback-error.txt") {
            match text(format!("{PARAM}/readback")) {
                Ok(v) => p.save("stage-readback.txt", &v)?,
                Err(e) => p.save("stage-readback-error.txt", &e.to_string())?,
            }
        }
        out("rmmod", &["pm_boundary_stage"])?;
    }
    let test = text("/sys/power/pm_test")?;
    require(
        test.starts_with("[none]") || (p.get("mode") == "core" && test.contains("[core]")),
        "foreign pm_test",
    )?;
    require(
        matches!(text("/sys/power/pm_async")?.as_str(), "0" | "1"),
        "foreign async",
    )?;
    fs::write("/sys/power/pm_test", "none")?;
    fs::write("/sys/power/pm_async", "1")?;
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
            "owns stage module, serial async and core test only; RTC only when intent saved",
        )?;
        out(
            "insmod",
            &[
                STAGE,
                &format!("nonce={}", p.get("nonce")),
                &format!("boot_id={BOOT}"),
                &format!("mode={}", p.get("mode")),
            ],
        )?;
        p.save("stage-before.txt", &text(format!("{PARAM}/status"))?)?;
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
        fs::write(format!("{PARAM}/armed"), "Y")?;
        require(
            text(format!("{PARAM}/armed"))? == "Y",
            "recorder arm readback",
        )?;
        marker(&format!(
            "BOUNDARY BEGIN id={} nonce={} mode={} success_before={} RTC={} boot={BOOT}",
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
        normal_clock()?;
        Ok(())
    })();
    let _ = reader.kill();
    let wait = reader.wait();
    let join = thread.join();
    let evidence = p.save("observed.jsonl", &captured);
    let cleanup = restore(p);
    p.save("outcome.txt",&format!("operation={operation:?}\nevidence={evidence:?}\ncleanup={cleanup:?}\nreader={wait:?}\njoin_ok={}\n",join.is_ok()))?;
    operation?;
    evidence?;
    cleanup?;
    health(p, true)?;
    let recording = boundary_record::verify_stage(
        &regular(&p.path("stage-status.txt"))?,
        &regular(&p.path("stage-readback.txt"))?,
        p.get("mode"),
        p.get("nonce"),
        BOOT,
    )?;
    let cycle = boundary_record::cycle(&captured, BOOT, p.get("mode"))?;
    p.save(
        "verification.json",
        &serde_json::to_string_pretty(&json!({"recording":recording,"cycle":cycle}))?,
    )?;
    p.save("after-kernel.jsonl", &kernel()?)?;
    let result = if p.prior == 0 {
        "CORE_TRANSPORT_PASS"
    } else {
        "S3_LOWLEVEL_RETURN"
    };
    p.save("completed.json",&serde_json::to_string_pretty(&json!({"result":result,"boot":BOOT,"btime":BTIME,"id":p.get("id"),"successBefore":p.prior,"successAfter":p.prior+1,"completedEpoch":now()?,"physicalSleepSeconds":null,"rtcTarget":p.rtc,"restoration":"PASS"}))?)?;
    marker(&format!(
        "BOUNDARY RETURN id={} result={result} physical_duration_pending",
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
                "BOUNDARY READY id={} nonce={} boot={BOOT}",
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
    fn manifest_rejects_replay_and_unreviewed_second_cycle() {
        let s = format!(
            "schema=efi-s3-reviewed-20261101-2\nid=s3-efi-v2-20261101\nnonce=deep1101a\nboot={BOOT}\nbtime={BTIME}\nmode=s3\ndue=1793538900\nrtc=1793539200\nreview=1793539500\nruntime=/nix/store/example/bin/predator-pm-boundary-s3\nruntime_sha={}\nsource_sha={}\nmodule={STAGE}\nmodule_sha={STAGE_HASH}\nsuccess_before=1\ncore_review_sha={}\nretention_sha={RETAINED_HASH}\nowner_ready=owner-confirmed-ready-20261101\npm_async=0\npm_trace=0\npm_test=none\n",
            "a".repeat(64),
            "b".repeat(64),
            "c".repeat(64)
        );
        Plan::parse(&s).unwrap();
        for (a, b) in [
            ("success_before=1", "success_before=0"),
            ("mode=s3", "mode=core"),
            ("pm_test=none", "pm_test=core"),
            ("rtc=1793539200", "rtc=1793539201"),
            ("pm_async=0", "pm_async=1"),
            ("due=1793538900", "due=1793570400"),
            ("review=1793539500", "review=1793539501"),
            ("core_review_sha=", "core_completion_sha="),
            (BOOT, "bf51c3ac-164e-4bfa-b56c-7b71df526b7f"),
        ] {
            assert!(Plan::parse(&s.replace(a, b)).is_err());
        }
        assert!(Plan::parse(&(s + "pm_test=core\n")).is_err());
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
        let v = json!({"result":"CORE_TRANSPORT_INDEPENDENTLY_VERIFIED","boot":BOOT,"successAfter":1,
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
}
