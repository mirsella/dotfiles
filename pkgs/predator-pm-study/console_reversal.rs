//! November 2: NIC system-wakeup policy S3 control with final request deadline.
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
const BOOT: &str = "d793c222-876d-4d91-adb2-5f64b50857d5";
const JB: &str = "d793c222876d4d91adb25f64b50857d5";
const OLD_BOOT: &str = "39d7579b-b86d-4d30-b910-131943e550c8";
const BTIME: u64 = 1793621941;
const STATE: &str = "/var/lib/predator-pm-console-reversal";
const PRIOR_REVERSAL: &str = "/var/lib/predator-pm-nic-reversal/nic-enabled-s3-20261102";
const PRIOR_NIC: &str = "/var/lib/predator-pm-nic-wakeup/nic-wake-s3-20261102";
const NIC: &str = "/sys/bus/pci/devices/0000:03:00.1/power/wakeup";
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
        m.is_file() && m.uid() == 0 && m.gid() == 0 && m.mode() & 0o777 == 0o600,
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
    // These seven exact records were authenticated in the sealed recovery
    // journal before this protocol existed. Memory registration at boot is not
    // a hibernation attempt. No other hibernation record is exempted.
    let boot_registration: Vec<Value> = include_str!("nic_boot_nosave.jsonl")
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    require(
        boot_registration.len() == 7,
        "boot registration fixture count",
    )?;
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
        ("PM: suspend entry", 2 + usize::from(after)),
        ("PM: suspend entry (deep)", 2 + usize::from(after)),
        ("PM: suspend exit", 2 + usize::from(after)),
        ("PM: suspend debug:", 0),
        ("Low-level resume complete", 2 + usize::from(after)),
        ("BOUNDARY BEGIN", 0),
        ("NIC_WAKE BEGIN", 1),
        ("NIC_WAKE RETURN", 1),
        ("NIC_WAKE BEGIN id=nic-wake-s3-20261102 nonce=nic1102a", 1),
        ("NIC_WAKE RETURN id=nic-wake-s3-20261102", 1),
        ("NIC_REVERSAL BEGIN", 1),
        ("NIC_REVERSAL RETURN", 1),
        (
            "NIC_REVERSAL BEGIN id=nic-enabled-s3-20261102 nonce=nice1102",
            1,
        ),
        ("NIC_REVERSAL RETURN id=nic-enabled-s3-20261102", 1),
        ("CONSOLE_REVERSAL BEGIN", usize::from(after)),
        (
            "CONSOLE_REVERSAL BEGIN id=console-y-s3-20261102 nonce=consy1102",
            usize::from(after),
        ),
        ("CONSOLE_REVERSAL RETURN", 0),
    ] {
        require(
            messages.iter().filter(|m| m.contains(needle)).count() == count,
            format!("reviewed history changed: {needle}"),
        )?;
    }
    for v in by_cursor.values() {
        let m = v["MESSAGE"].as_str().ok_or("history message")?;
        require(
            ![
                "RECORDER_OFF BEGIN",
                "CONSOLE_CONTROL BEGIN",
                "PM: suspend-to-idle",
                "pm_boundary_stage",
                "pm_boundary_compact",
                "pm_boundary_probe",
                "BUG:",
                "Oops:",
                "Call Trace:",
            ]
            .iter()
            .any(|s| m.contains(s)),
            format!("unexpected prior attempt/error: {m}"),
        )?;
        require(
            !m.contains("PM: hibernation") || boot_registration.contains(v),
            format!("unrecognized hibernation record: {m}"),
        )?;
    }
    require(!by_cursor.is_empty(), "empty boot history")?;
    for v in &boot_registration {
        let c = v["__CURSOR"].as_str().ok_or("boot registration cursor")?;
        require(
            by_cursor.get(c) == Some(v),
            "known boot registration record missing/changed",
        )?;
    }
    preserve_records(current, include_str!("console_prior_cycles.jsonl"))?;
    let new_marker = messages
        .iter()
        .position(|m| m.contains("CONSOLE_REVERSAL BEGIN"));
    let previous_return = messages
        .iter()
        .position(|m| m.contains("NIC_REVERSAL RETURN id=nic-enabled-s3-20261102"))
        .ok_or("prior return missing")?;
    if let Some(index) = new_marker {
        require(index > previous_return, "new attempt precedes prior return")?;
    }
    Ok(())
}
fn verified_kernel_history(after: bool) -> Result<()> {
    let current = kernel()?;
    reviewed_history(&current, after)?;
    for suffix in [
        "before-kernel.jsonl",
        "after-kernel.jsonl",
        "observed.jsonl",
    ] {
        preserve_records(&current, &regular(&format!("{PRIOR_NIC}-{suffix}"))?)?;
        preserve_records(&current, &regular(&format!("{PRIOR_REVERSAL}-{suffix}"))?)?;
    }
    Ok(())
}
fn preserve_records(current: &str, prior: &str) -> Result<()> {
    let mut by_cursor = std::collections::BTreeMap::new();
    for line in current.lines() {
        let v: Value = serde_json::from_str(line)?;
        let cursor = v["__CURSOR"].as_str().ok_or("current cursor")?.to_owned();
        require(
            by_cursor.insert(cursor, v).is_none(),
            "duplicate current cursor",
        )?;
    }
    for line in prior.lines() {
        let v: Value = serde_json::from_str(line)?;
        require(v["_BOOT_ID"] == JB, "prior boot changed")?;
        if v["_TRANSPORT"] == "kernel" {
            let cursor = v["__CURSOR"].as_str().ok_or("prior cursor")?;
            require(
                by_cursor.get(cursor) == Some(&v),
                "prior journal record missing/changed",
            )?;
        }
    }
    Ok(())
}
fn prior_nic_identity() -> Result<()> {
    prior_reversal_identity()?;
    require(
        hash(
            "/nix/store/vv6x9a57k1xj192qkn1xbzkb2kinkmjl-predator-pm-study-0.1.0/bin/predator-pm-nic-wakeup",
        )? == "5774502b6f2f215a88abf4b0af00f5666c93e4cb08d997eb84fab8d5e30b3b0e",
        "prior runtime pin",
    )?;
    let v: Value = serde_json::from_str(&regular(&format!("{PRIOR_NIC}-completed.json"))?)?;
    require(
        v["boot"] == BOOT
            && v["successBefore"] == 0
            && v["successAfter"] == 1
            && v["restoration"] == "PASS"
            && v["result"] == "NIC_WAKE_S3_LOWLEVEL_RETURN",
        "prior completion",
    )
}
fn prior_reversal_identity() -> Result<()> {
    let mut namespaces =
        std::collections::BTreeMap::<String, std::collections::BTreeSet<String>>::new();
    for line in include_str!("console_prior_originals.sha256").lines() {
        let (sha, path) = line.split_once("  ").ok_or("prior originals fixture")?;
        require(
            path.starts_with(&format!("{PRIOR_NIC}-"))
                || path.starts_with(&format!("{PRIOR_REVERSAL}-")),
            "unexpected prior path",
        )?;
        regular(path)?;
        require(hash(path)? == sha, format!("changed original: {path}"))?;
        let p = Path::new(path);
        let parent = p.parent().ok_or("parent")?.to_str().ok_or("parent text")?;
        let name = p
            .file_name()
            .ok_or("filename")?
            .to_str()
            .ok_or("filename text")?;
        require(
            namespaces
                .entry(parent.into())
                .or_default()
                .insert(name.into()),
            "duplicate prior original",
        )?;
    }
    require(namespaces.len() == 2, "prior namespace count")?;
    for (path, expected) in namespaces {
        let m = fs::symlink_metadata(&path)?;
        require(
            m.is_dir() && m.uid() == 0 && m.gid() == 0 && m.mode() & 0o777 == 0o700,
            "prior namespace metadata",
        )?;
        let actual = fs::read_dir(path)?
            .map(|e| Ok(e?.file_name().into_string().map_err(|_| "nontext name")?))
            .collect::<Result<std::collections::BTreeSet<String>>>()?;
        require(
            expected.len() == 21 && actual == expected,
            "prior original inventory",
        )?;
    }
    require(
        hash(
            "/nix/store/k6dn085cig7qvwgh72r74r2s5x5mja10-predator-pm-study-0.1.0/bin/predator-pm-nic-reversal",
        )? == "a099cdc2c6fb5f843cda35acc208a056fb5095770daf1763842989015bca2c61",
        "prior reversal runtime",
    )?;
    let v: Value = serde_json::from_str(&regular(&format!("{PRIOR_REVERSAL}-completed.json"))?)?;
    require(
        v["boot"] == BOOT
            && v["successBefore"] == 1
            && v["successAfter"] == 2
            && v["restoration"] == "PASS"
            && v["result"] == "NIC_REVERSAL_S3_LOWLEVEL_RETURN",
        "prior reversal completion",
    )
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
fn observed_nic(value: &str) -> Result<()> {
    require(
        value == "enabled",
        "foreign NIC policy; this protocol has no NIC write authority",
    )
}
fn owned_console(value: &str, intent: bool, requested: bool) -> Result<bool> {
    require(
        (intent && value == "Y") || (!requested && value == "N"),
        "foreign/unowned console state",
    )?;
    Ok(intent && value == "Y")
}
fn cleanup_gate(requested: bool, verified_return: bool, started: bool) -> Result<()> {
    require(
        !started && (!requested || verified_return),
        "unverified return or cleanup already attempted",
    )
}
fn nic_fixed() -> Result<()> {
    for (path, expected) in [
        (
            "/sys/class/net/enp3s0f1/device",
            "/sys/devices/pci0000:00/0000:00:1c.3/0000:03:00.1",
        ),
        (
            "/sys/bus/pci/devices/0000:03:00.1/driver",
            "/sys/bus/pci/drivers/r8169",
        ),
        (
            "/sys/bus/pci/devices/0000:03:00.0/driver",
            "/sys/bus/pci/drivers/rtsx_pci",
        ),
        (
            "/sys/bus/pci/devices/0000:00:1c.3/driver",
            "/sys/bus/pci/drivers/pcieport",
        ),
    ] {
        require(
            fs::canonicalize(path)? == Path::new(expected),
            format!("NIC topology changed: {path}"),
        )?;
    }
    for (path, expected) in [
        ("/sys/bus/pci/devices/0000:03:00.1/vendor", "0x10ec"),
        ("/sys/bus/pci/devices/0000:03:00.1/device", "0x8168"),
        ("/sys/bus/pci/devices/0000:03:00.1/class", "0x020000"),
        ("/sys/bus/pci/devices/0000:03:00.1/d3cold_allowed", "1"),
        ("/sys/bus/pci/devices/0000:03:00.1/power/control", "on"),
        ("/sys/bus/pci/devices/0000:00:1c.3/power/wakeup", "enabled"),
        ("/sys/bus/pci/devices/0000:03:00.0/power/wakeup", "disabled"),
    ] {
        require(
            text(path)? == expected,
            format!("unowned setting changed: {path}"),
        )?;
    }
    require(
        out("ethtool", &["enp3s0f1"])?
            .lines()
            .any(|l| l.trim() == "Wake-on: g"),
        "ethtool wake configuration changed",
    )
}
fn request_deadline(epoch: u64, due: u64) -> Result<()> {
    require((due..=due + 30).contains(&epoch), "final request deadline")
}
trait RequestAdapter {
    fn armed_snapshot(&mut self) -> Result<()>;
    fn sync_journal(&mut self) -> Result<()>;
    fn final_guards(&mut self) -> Result<()>;
    fn record_intent(&mut self) -> Result<()>;
    fn epoch(&mut self) -> Result<u64>;
    fn record_not_issued(&mut self, reason: &str) -> Result<()>;
    fn submit(&mut self) -> Result<String>;
}
// The production adapter and fake-clock tests execute this exact ordering.
// Durable intent can itself stall; no filesystem or command work follows the last
// clock sample before submit. Once submit is invoked, any error is ambiguous.
fn issue_suspend_request(ops: &mut impl RequestAdapter, due: u64) -> Result<String> {
    ops.armed_snapshot()?;
    ops.sync_journal()?;
    ops.final_guards()?;
    let readiness = ops
        .record_intent()
        .and_then(|_| request_deadline(ops.epoch()?, due));
    if let Err(error) = readiness {
        ops.record_not_issued(&error.to_string())?;
        return Err(error);
    }
    ops.submit()
}
fn request_may_have_run(
    call: bool,
    not_issued: Option<&str>,
    expected: &str,
    accepted: bool,
    returned: bool,
) -> Result<bool> {
    if let Some(proof) = not_issued {
        require(
            proof == expected && !accepted && !returned,
            "invalid/conflicting no-request proof",
        )?;
        return Ok(false);
    }
    require(
        call || (!accepted && !returned),
        "request outcome without intent",
    )?;
    Ok(call)
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
            "nic_policy",
            "nic_policy_writes",
            "console_baseline",
            "console_trial",
            "resume_policy",
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
            ("schema", "console-y-s3-20261102-1"),
            ("boot", BOOT),
            ("btime", "1793621941"),
            ("recorder", "absent"),
            ("efi_writes", "0"),
            ("core_review_sha", CORE_REVIEW_HASH),
            ("prior_completion_sha", COMPLETION_HASH),
            (
                "meter_scope",
                "owner-confirmed-identical-laptop-and-peripherals",
            ),
            (
                "hypothesis",
                "console-Y-reversal-after-two-console-N-returns",
            ),
            (
                "prediction",
                "Y-nonreturn-supports-association-not-unique-causality",
            ),
            (
                "negative_prediction",
                "Y-return-means-N-unnecessary-for-this-return",
            ),
            ("nic_policy", "enabled"),
            ("nic_policy_writes", "0"),
            ("console_baseline", "N"),
            ("console_trial", "Y"),
            (
                "resume_policy",
                "enabled-must-remain-enabled-no-policy-write-authority",
            ),
            (
                "rollback",
                "owned-console-N-RTC-clear-async1-after-verified-return-NIC-observation-only",
            ),
            ("retention_sha", RETAINED_HASH),
            (
                "owner_ready",
                "owner-all-permitted-windows-no-reconfirmation-20261102",
            ),
            ("pm_async", "0"),
            ("pm_trace", "0"),
        ] {
            require(m[k] == v, format!("invalid {k}"))?;
        }
        for (k, v) in [
            ("id", "console-y-s3-20261102"),
            ("nonce", "consy1102"),
            ("mode", "s3"),
            ("pm_test", "none"),
            ("success_before", "2"),
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
                && m["runtime"].ends_with("/bin/predator-pm-console-reversal"),
            "mutable/wrong runtime",
        )?;
        let due = m["due"].parse()?;
        let rtc = m["rtc"].parse()?;
        let review: u64 = m["review"].parse()?;
        require(
            (1793653200..=1793653800).contains(&due)
                && rtc == due + 300
                && review == 1793655600
                && due + 480 < review,
            "trial timing",
        )?;
        Ok(Self {
            m,
            due,
            rtc,
            prior: 2,
        })
    }
    fn path(&self, s: &str) -> String {
        format!("{STATE}/{}-{s}", self.get("id"))
    }
    fn exists(&self, s: &str) -> bool {
        Path::new(&self.path(s)).exists()
    }
    fn no_request_proof(&self) -> String {
        format!("boot={BOOT}\nid={}\nsubmit_invoked=false", self.get("id"))
    }
    fn request_may_have_run(&self) -> Result<bool> {
        let proof = if self.exists("request-not-issued.txt") {
            Some(regular(&self.path("request-not-issued.txt"))?)
        } else {
            None
        };
        request_may_have_run(
            self.exists("request-call.txt"),
            proof.as_deref(),
            &self.no_request_proof(),
            self.exists("request-ok.txt"),
            self.exists("return-verified.json"),
        )
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
        prior_nic_identity()?;
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
                == "d51502c2965a2a6da8c9af5cf0bba9b6183349e4364a3afc8ad15d4b8cd72dc8",
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
        "predator-pm-console-control-v2",
        "predator-pm-nic-wakeup",
        "predator-pm-nic-reversal",
        "predator-pm-console-reversal",
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
    nic_fixed()?;
    require(
        out(
            "systemctl",
            &[
                "show",
                "predator-pm-clock-handoff.service",
                "-p",
                "InvocationID",
                "--value",
            ],
        )? == "bea175a797f34b77af9d3d222932e3d1"
            && out(
                "systemctl",
                &[
                    "show",
                    "predator-pm-clock-handoff.service",
                    "-p",
                    "Result",
                    "--value",
                ],
            )? == "success"
            && out(
                "systemctl",
                &[
                    "show",
                    "predator-pm-clock-handoff.service",
                    "-p",
                    "ActiveState",
                    "--value",
                ],
            )? == "active"
            && !Path::new("/run/systemd/netif/state").exists()
            && hash("/var/lib/predator-handoff-recovery-20261102-v3/completed.txt")?
                == "0d8d8dec828efbd59d9297dacb4feaa97a087ab43f068fd36dd11a1552721d98",
        "successful handoff repair changed",
    )?;
    verified_kernel_history(after)?;
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
        NIC,
        "/sys/bus/pci/devices/0000:00:1c.3/power/wakeup",
        "/sys/bus/pci/devices/0000:03:00.0/power/wakeup",
        "/sys/bus/pci/devices/0000:03:00.1/d3cold_allowed",
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
        "<6>pm-console-reversal: {s}"
    )?;
    Ok(())
}
fn restore(p: &Plan) -> Result<()> {
    p.identity()?;
    inactive()?;
    if !p.exists("mutating.txt") || p.exists("restored.txt") {
        return Ok(());
    }
    let requested = p.request_may_have_run()?;
    cleanup_gate(
        requested,
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
    nic_fixed()?;
    observed_nic(&text(NIC)?)?;
    let console_intent = if p.exists("console-intent.txt") {
        require(
            regular(&p.path("console-intent.txt"))?
                == format!("boot={BOOT}\npath={CONSOLE}\noriginal=N\ntrial=Y"),
            "console intent changed",
        )?;
        true
    } else {
        false
    };
    let restore_console = owned_console(&text(CONSOLE)?, console_intent, requested)?;
    let test = text("/sys/power/pm_test")?;
    require(test.starts_with("[none]"), "foreign pm_test")?;
    require(
        text("/sys/power/pm_async")? == "0" || (!requested && text("/sys/power/pm_async")? == "1"),
        "foreign async",
    )?;
    p.save(
        "cleanup-started.txt",
        "same boot; inactive/MainPID0/empty queue; all ownership checks passed; no retry",
    )?;
    if !alarm.is_empty() {
        fs::write(ALARM, "0")?;
    }
    if restore_console {
        fs::write(CONSOLE, "N")?;
    }
    fs::write("/sys/power/pm_test", "none")?;
    fs::write("/sys/power/pm_async", "1")?;
    require(
        text(CONSOLE)? == "N" && text(NIC)? == "enabled" && text(ALARM)?.is_empty(),
        "unchanged NIC and owned alarm restoration readback",
    )?;
    let helper = out(HANDOFF, &["clock-handoff"])?;
    p.save("clock-handoff.txt", &helper)?;
    normal_clock()?;
    p.save("restored.txt", &snapshot()?)?;
    Ok(())
}
struct MachineRequest<'a> {
    plan: &'a Plan,
    submit_invoked: bool,
}
impl RequestAdapter for MachineRequest<'_> {
    fn armed_snapshot(&mut self) -> Result<()> {
        self.plan.save("alarm-armed.txt", &snapshot()?)
    }
    fn sync_journal(&mut self) -> Result<()> {
        out("journalctl", &["--sync"])?;
        Ok(())
    }
    fn final_guards(&mut self) -> Result<()> {
        self.plan.identity()?;
        inactive()?;
        quiet()?;
        normal_clock()?;
        recorders_absent()?;
        nic_fixed()?;
        require(
            text(CONSOLE)? == "Y"
                && text(NIC)? == "enabled"
                && text("/sys/power/pm_async")? == "0"
                && text("/sys/power/pm_test")?.starts_with("[none]"),
            "foreign setting before request",
        )?;
        require(
            text(ALARM)? == self.plan.rtc.to_string()
                && text("/proc/driver/rtc")?
                    .lines()
                    .any(|l| l.starts_with("alarm_IRQ") && l.ends_with("yes")),
            "RTC changed before request",
        )
    }
    fn record_intent(&mut self) -> Result<()> {
        self.plan.save(
            "request-call.txt",
            "logind SuspendWithFlags(1) intent; inhibitors enforced; final deadline still required",
        )
    }
    fn epoch(&mut self) -> Result<u64> {
        now()
    }
    fn record_not_issued(&mut self, reason: &str) -> Result<()> {
        require(!self.submit_invoked, "cannot waive an invoked request")?;
        self.plan.save("request-rejected.txt", reason)?;
        self.plan
            .save("request-not-issued.txt", &self.plan.no_request_proof())
    }
    fn submit(&mut self) -> Result<String> {
        self.submit_invoked = true;
        out(
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
        )
    }
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
            "original async=1; NIC wakeup=enabled observation-only; console=N owned trial=Y; alarm empty; owns console, serial async and RTC only after saved intents; no NIC/EFI writes or module operations",
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
        nic_fixed()?;
        observed_nic(&text(NIC)?)?;
        p.save(
            "nic-observed.txt",
            &format!("boot={BOOT}\npath={NIC}\npolicy=enabled\npolicy_writes=0"),
        )?;
        observed_nic(&text(NIC)?)?;
        nic_fixed()?;
        require(text(CONSOLE)? == "N", "console changed before owned intent")?;
        p.save(
            "console-intent.txt",
            &format!("boot={BOOT}\npath={CONSOLE}\noriginal=N\ntrial=Y"),
        )?;
        fs::write(CONSOLE, "Y")?;
        require(text(CONSOLE)? == "Y", "console trial readback")?;
        marker(&format!(
            "CONSOLE_REVERSAL BEGIN id={} nonce={} mode={} success_before={} RTC={} boot={BOOT} EFI_writes=0 console_suspend=Y nic_wakeup=enabled nic_policy_writes=0",
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
        }
        let response = issue_suspend_request(
            &mut MachineRequest {
                plan: p,
                submit_invoked: false,
            },
            p.due,
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
        nic_fixed()?;
        observed_nic(&text(NIC)?)?;
        require(text(CONSOLE)? == "Y", "foreign returned console state")?;
        require(
            text("/sys/power/suspend_stats/success")? == "3"
                && text("/sys/power/suspend_stats/fail")? == "0",
            "return counters",
        )?;
        verified_kernel_history(true)?;
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
    let cleanup = match p.request_may_have_run() {
        Ok(false) => restore(p),
        Ok(true) if operation.is_ok() => restore(p),
        Ok(true) => {
            Err("no verified return after request; cleanup deliberately not attempted".into())
        }
        Err(error) => Err(error),
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
    let result = "CONSOLE_REVERSAL_S3_LOWLEVEL_RETURN";
    p.save("completed.json",&serde_json::to_string_pretty(&json!({"result":result,"boot":BOOT,"btime":BTIME,"id":p.get("id"),"successBefore":p.prior,"successAfter":p.prior+1,"completedEpoch":now()?,"physicalSleepSeconds":null,"rtcTarget":p.rtc,"restoration":"PASS"}))?)?;
    marker(&format!(
        "CONSOLE_REVERSAL RETURN id={} result={result} physical_duration_pending EFI_writes=0 nic_policy_writes=0",
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
    let state = fs::symlink_metadata(STATE)?;
    require(
        state.is_dir() && state.uid() == 0 && state.gid() == 0 && state.mode() & 0o777 == 0o700,
        "root state identity",
    )?;
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
                "CONSOLE_REVERSAL READY id={} nonce={} boot={BOOT}",
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
    fn manifest_rejects_replay_recorder_wrong_policy_and_old_history() {
        let s = format!(
            "schema=console-y-s3-20261102-1\nid=console-y-s3-20261102\nnonce=consy1102\nboot={BOOT}\nbtime={BTIME}\nmode=s3\ndue=1793653200\nrtc=1793653500\nreview=1793655600\nruntime=/nix/store/example/bin/predator-pm-console-reversal\nruntime_sha={}\nsource_sha={}\nrecorder=absent\nefi_writes=0\nsuccess_before=2\ncore_review_sha={CORE_REVIEW_HASH}\nprior_completion_sha={COMPLETION_HASH}\nmeter_scope=owner-confirmed-identical-laptop-and-peripherals\nhypothesis=console-Y-reversal-after-two-console-N-returns\nprediction=Y-nonreturn-supports-association-not-unique-causality\nnegative_prediction=Y-return-means-N-unnecessary-for-this-return\nnic_policy=enabled\nnic_policy_writes=0\nconsole_baseline=N\nconsole_trial=Y\nresume_policy=enabled-must-remain-enabled-no-policy-write-authority\nrollback=owned-console-N-RTC-clear-async1-after-verified-return-NIC-observation-only\nretention_sha={RETAINED_HASH}\nowner_ready=owner-all-permitted-windows-no-reconfirmation-20261102\npm_async=0\npm_trace=0\npm_test=none\n",
            "a".repeat(64),
            "b".repeat(64)
        );
        Plan::parse(&s).unwrap();
        for (a, b) in [
            ("success_before=2", "success_before=0"),
            ("success_before=2", "success_before=1"),
            ("nic_policy=enabled", "nic_policy=disabled"),
            ("nic_policy_writes=0", "nic_policy_writes=1"),
            ("console_baseline=N", "console_baseline=Y"),
            ("console_trial=Y", "console_trial=N"),
            (
                "resume_policy=enabled-must-remain-enabled-no-policy-write-authority",
                "resume_policy=ignore-return-value",
            ),
            ("recorder=absent", "recorder=present"),
            ("efi_writes=0", "efi_writes=3"),
            (
                "meter_scope=owner-confirmed-identical-laptop-and-peripherals",
                "meter_scope=unknown",
            ),
            ("mode=s3", "mode=core"),
            ("pm_test=none", "pm_test=core"),
            ("rtc=1793653500", "rtc=1793653501"),
            ("pm_async=0", "pm_async=1"),
            ("due=1793653200", "due=1793653199"),
            ("due=1793653200", "due=1793653801"),
            ("review=1793655600", "review=1793655601"),
            (
                "schema=console-y-s3-20261102-1",
                "schema=nic-wake-s3-20261102-1",
            ),
            ("nonce=consy1102", "nonce=nice1102"),
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
    fn preserves_prior_cycle_then_accepts_only_one_new_cycle() {
        let boot = format!(
            "{}{}",
            include_str!("nic_boot_nosave.jsonl"),
            include_str!("console_prior_cycles.jsonl")
        );
        reviewed_history(&boot, false).unwrap();
        let j = format!(
            "{boot}{}",
            journal(&[
                "CONSOLE_REVERSAL BEGIN id=console-y-s3-20261102 nonce=consy1102",
                "PM: suspend entry (deep)",
                "ACPI: PM: Low-level resume complete",
                "PM: suspend exit",
            ])
        );
        reviewed_history(&j, true).unwrap();
        assert!(reviewed_history(&j, false).is_err());
        assert!(reviewed_history("", false).is_err());
        for s in [
            j.replace("(deep)", "(s2idle)"),
            j.replace(JB, "wrong"),
            j.replace("consy1102", "othernonce"),
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
            assert!(reviewed_history(&format!("{boot}{}", journal(&[m])), false).is_err());
        }
        for m in [
            "PM: hibernation: hibernation entry",
            "PM: hibernation: hibernation exit",
            "PM: hibernation: Registered nosave memory: [mem 0x0-0x1]",
        ] {
            assert!(reviewed_history(&format!("{boot}{}", journal(&[m])), false).is_err());
        }
        let mut records: Vec<Value> = boot
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        records[0]["_SOURCE_MONOTONIC_TIMESTAMP"] = json!("0");
        let changed = records
            .iter()
            .map(|v| v.to_string() + "\n")
            .collect::<String>();
        assert!(reviewed_history(&changed, false).is_err());
        assert!(reviewed_history(include_str!("console_boot_nosave.jsonl"), false).is_err());
        assert!(reviewed_history(include_str!("nic_boot_nosave.jsonl"), false).is_err());
        assert!(reviewed_history(&boot.replace("0x00000000", "0x00001000"), false).is_err());
        for key in [
            "_SOURCE_MONOTONIC_TIMESTAMP",
            "_SOURCE_BOOTTIME_TIMESTAMP",
            "__MONOTONIC_TIMESTAMP",
            "__REALTIME_TIMESTAMP",
        ] {
            let prior: Vec<Value> = include_str!("console_prior_cycles.jsonl")
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect();
            let mut changed = prior.clone();
            changed[1][key] = json!("1");
            let j = format!(
                "{}{}",
                include_str!("nic_boot_nosave.jsonl"),
                changed
                    .iter()
                    .map(|v| v.to_string() + "\n")
                    .collect::<String>()
            );
            assert!(reviewed_history(&j, false).is_err());
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
    fn enabled_observation_and_once_only_verified_return_cleanup() {
        observed_nic("enabled").unwrap();
        for value in ["disabled", "foreign", "", "Enabled"] {
            assert!(observed_nic(value).is_err());
        }
        let production = include_str!("console_reversal.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(!production.contains("fs::write(NIC"));
        assert!(!production.contains("nic-intent.txt"));
        cleanup_gate(false, false, false).unwrap();
        cleanup_gate(true, true, false).unwrap();
        assert!(cleanup_gate(true, false, false).is_err());
        assert!(cleanup_gate(true, true, true).is_err());
        assert!(cleanup_gate(false, false, true).is_err());
    }
    #[test]
    fn prior_inventory_and_full_value_continuity_are_strict() {
        let pins: Vec<_> = include_str!("console_prior_originals.sha256")
            .lines()
            .collect();
        assert_eq!(pins.len(), 42);
        assert_eq!(
            pins.iter().collect::<std::collections::BTreeSet<_>>().len(),
            42
        );
        let prior = journal(&["ordinary kernel record", "another record"]);
        preserve_records(&prior, &prior).unwrap();
        assert!(preserve_records(&prior.replace("ordinary", "changed"), &prior).is_err());
        assert!(preserve_records("", &prior).is_err());
        assert!(preserve_records(&format!("{prior}{prior}"), &prior).is_err());
    }
    #[test]
    fn console_cleanup_requires_owned_trial_and_verified_request_gate() {
        assert!(!owned_console("N", false, false).unwrap());
        assert!(!owned_console("N", true, false).unwrap());
        assert!(owned_console("Y", true, false).unwrap());
        cleanup_gate(true, true, false).unwrap();
        assert!(owned_console("Y", true, true).unwrap());
        for (value, intent, requested) in [
            ("Y", false, false),
            ("Y", false, true),
            ("N", true, true),
            ("N", false, true),
            ("foreign", true, false),
            ("", true, true),
        ] {
            assert!(owned_console(value, intent, requested).is_err());
        }
        assert!(cleanup_gate(true, false, false).is_err());
        assert!(cleanup_gate(true, true, true).is_err());
    }
    #[test]
    fn both_exact_prior_cycles_are_required() {
        let anchors = include_str!("nic_boot_nosave.jsonl");
        let cycles: Vec<_> = include_str!("console_prior_cycles.jsonl").lines().collect();
        assert_eq!(cycles.len(), 10);
        for missing in 0..cycles.len() {
            let history = format!(
                "{anchors}{}",
                cycles
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != missing)
                    .map(|(_, s)| format!("{s}\n"))
                    .collect::<String>()
            );
            assert!(reviewed_history(&history, false).is_err());
        }
        assert!(
            reviewed_history(
                &format!("{anchors}{}", include_str!("nic_prior_cycle.jsonl")),
                false
            )
            .is_err()
        );
    }
    #[derive(Default)]
    struct FakeRequest {
        epoch: u64,
        delays: [u64; 4],
        files: std::collections::BTreeMap<&'static str, String>,
        events: Vec<&'static str>,
        submit_count: usize,
        fail_submit: bool,
        fail_intent: bool,
        fail_proof: bool,
        unsafe_state: bool,
    }
    impl RequestAdapter for FakeRequest {
        fn armed_snapshot(&mut self) -> Result<()> {
            self.events.push("snapshot");
            self.epoch += self.delays[0];
            self.files.insert(
                "snapshot",
                "nic=enabled console=N async=0 alarm=1300".into(),
            );
            Ok(())
        }
        fn sync_journal(&mut self) -> Result<()> {
            self.events.push("sync");
            self.epoch += self.delays[1];
            Ok(())
        }
        fn final_guards(&mut self) -> Result<()> {
            self.events.push("guards");
            self.epoch += self.delays[2];
            require(!self.unsafe_state, "unsafe sleep/history/ownership")
        }
        fn record_intent(&mut self) -> Result<()> {
            self.events.push("intent");
            self.epoch += self.delays[3];
            self.files.insert("intent", "durable intent".into());
            require(!self.fail_intent, "intent fsync failed")
        }
        fn epoch(&mut self) -> Result<u64> {
            self.events.push("clock");
            Ok(self.epoch)
        }
        fn record_not_issued(&mut self, reason: &str) -> Result<()> {
            self.events.push("rejection");
            require(
                self.submit_count == 0 && !self.fail_proof,
                "no durable rejection proof",
            )?;
            self.files.insert("not-issued", "proof".into());
            self.files.insert("reason", reason.into());
            Ok(())
        }
        fn submit(&mut self) -> Result<String> {
            self.events.push("submit");
            self.submit_count += 1;
            require(!self.fail_submit, "logind status uncertain")?;
            Ok("accepted".into())
        }
    }
    fn fake_rollback(
        f: &mut FakeRequest,
        policy: &mut String,
        alarm: &mut String,
        async_value: &mut String,
    ) -> Result<()> {
        let requested = request_may_have_run(
            f.files.contains_key("intent"),
            f.files.get("not-issued").map(String::as_str),
            "proof",
            false,
            false,
        )?;
        cleanup_gate(requested, false, f.files.contains_key("cleanup-started"))?;
        inactive_values(if f.unsafe_state { "active" } else { "inactive" }, "0", "")?;
        observed_nic(policy)?;
        owned_alarm(alarm, true, 1300)?;
        require(
            async_value == "0" || (!requested && async_value == "1"),
            "foreign async",
        )?;
        f.files
            .insert("raw-return", format!("{policy} {alarm} {async_value}"));
        f.files.insert("cleanup-started", "once".into());
        alarm.clear();
        *async_value = "1".into();
        Ok(())
    }
    #[test]
    fn original_byte_pinned_tail_allows_a_late_request() {
        let original = include_str!("console_control.rs");
        let tail = original
            .split_once("p.save(\"alarm-armed.txt\", &snapshot()?)?;")
            .unwrap()
            .1
            .split_once("p.save(\"request-ok.txt\"")
            .unwrap()
            .0;
        assert_eq!(
            tail,
            "\n        }\n        out(\"journalctl\", &[\"--sync\"])?;\n        p.save(\n            \"request-call.txt\",\n            \"logind SuspendWithFlags(1), inhibitors enforced\",\n        )?;\n        let response = out(\n            \"busctl\",\n            &[\n                \"call\",\n                \"org.freedesktop.login1\",\n                \"/org/freedesktop/login1\",\n                \"org.freedesktop.login1.Manager\",\n                \"SuspendWithFlags\",\n                \"t\",\n                \"1\",\n            ],\n        )?;\n        "
        );
        for delays in [[90, 0, 0, 0], [0, 90, 0, 0]] {
            let mut f = FakeRequest {
                epoch: 1000,
                delays,
                ..Default::default()
            };
            request_deadline(f.epoch, 1000).unwrap();
            f.armed_snapshot().unwrap();
            f.sync_journal().unwrap();
            f.record_intent().unwrap();
            f.submit().unwrap();
            println!(
                "preserved original sequence: request={}, deadline=1030, calls={}",
                f.epoch, f.submit_count
            );
            assert!(f.epoch > 1030 && f.submit_count == 1);
        }
    }
    #[test]
    fn actual_request_sequence_refuses_each_slow_operation_and_rolls_back_once() {
        for i in 0..4 {
            let mut f = FakeRequest {
                epoch: 1000,
                ..Default::default()
            };
            f.delays[i] = 90;
            assert_eq!(
                issue_suspend_request(&mut f, 1000).unwrap_err().to_string(),
                "final request deadline"
            );
            assert_eq!(f.submit_count, 0);
            let (mut console, mut alarm, mut asynchronous) =
                ("enabled".into(), "1300".into(), "0".into());
            fake_rollback(&mut f, &mut console, &mut alarm, &mut asynchronous).unwrap();
            assert_eq!((&*console, &*alarm, &*asynchronous), ("enabled", "", "1"));
            assert_eq!(f.files["raw-return"], "enabled 1300 0");
            assert!(fake_rollback(&mut f, &mut console, &mut alarm, &mut asynchronous).is_err());
        }
    }
    #[test]
    fn clock_is_last_operation_before_submit_and_errors_never_authorize_cleanup() {
        for epoch in [1000, 1030] {
            let mut f = FakeRequest {
                epoch,
                ..Default::default()
            };
            issue_suspend_request(&mut f, 1000).unwrap();
            assert_eq!(
                f.events,
                ["snapshot", "sync", "guards", "intent", "clock", "submit"]
            );
            assert_eq!(f.submit_count, 1);
        }
        for epoch in [999, 1031] {
            let mut f = FakeRequest {
                epoch,
                ..Default::default()
            };
            assert!(issue_suspend_request(&mut f, 1000).is_err());
            assert_eq!(f.submit_count, 0);
        }
        for (fail_submit, fail_intent, fail_proof, unsafe_state) in [
            (true, false, false, false),
            (false, true, false, false),
            (false, true, true, false),
            (false, false, false, true),
        ] {
            let mut f = FakeRequest {
                epoch: 1000,
                fail_submit,
                fail_intent,
                fail_proof,
                unsafe_state,
                ..Default::default()
            };
            assert!(issue_suspend_request(&mut f, 1000).is_err());
            let (mut console, mut alarm, mut asynchronous) =
                ("enabled".into(), "1300".into(), "0".into());
            let rollback = fake_rollback(&mut f, &mut console, &mut alarm, &mut asynchronous);
            assert_eq!(rollback.is_ok(), fail_intent && !fail_proof);
            if rollback.is_err() {
                assert_eq!(
                    (&*console, &*alarm, &*asynchronous),
                    ("enabled", "1300", "0")
                );
            }
        }
        for (proof, accepted, returned) in [
            ("wrong", false, false),
            ("proof", true, false),
            ("proof", false, true),
        ] {
            assert!(request_may_have_run(true, Some(proof), "proof", accepted, returned).is_err());
        }
    }
    #[test]
    fn late_abort_still_refuses_foreign_alarm_nic_or_active_sleep() {
        for (console, alarm, active) in [
            ("foreign", "1300", false),
            ("enabled", "1400", false),
            ("enabled", "1300", true),
            ("disabled", "1300", false),
        ] {
            let mut f = FakeRequest {
                epoch: 1031,
                ..Default::default()
            };
            assert!(issue_suspend_request(&mut f, 1000).is_err());
            f.unsafe_state = active;
            let (mut c, mut a, mut asynchronous) = (console.into(), alarm.into(), "0".into());
            assert!(fake_rollback(&mut f, &mut c, &mut a, &mut asynchronous).is_err());
            assert_eq!((&*c, &*a, &*asynchronous), (console, alarm, "0"));
            assert!(!f.files.contains_key("cleanup-started"));
        }
    }
}
