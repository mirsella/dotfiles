//! Repeats suspend cycles on Predator and records each one durably, so a hang
//! leaves "enter" without "return" for the cycle that never came back.
//!
//! Build (static, on main): `rustc -O --edition 2021 -D warnings
//!     --target x86_64-unknown-linux-musl pm-batch.rs`
//! Run as root: `pm-batch <label> <dry|s3|s2idle> <cycles> [wake_s] [gap_s] [nosmi|gpu-off]`
//!              `pm-batch <label> snapshot`
//!
//! - `dry`    : `pm_test=core`. The kernel runs the whole suspend path, waits
//!              five seconds and resumes without asking the firmware to sleep.
//! - `s3`     : real suspend to RAM, woken by a relative RTC alarm.
//! - `s2idle` : suspend to idle, woken by the same alarm.
//! - `nosmi`  : clears SLP_SMI_EN before each real suspend, so the sleep
//!              request goes to the chipset without the firmware's SMM sleep
//!              handler running first. The bit is restored on return.
//! - `snapshot`: writes hardware and power state to
//!              `logs/<label>.snapshot.txt`, for comparing a fresh boot with
//!              one that has been awake for hours.
//!
//! A batch refuses to start unless every pool is healthy, SLP_SMI_EN is set
//! and systemd's job queue empties within ten minutes. It exits non-zero unless
//! every cycle returned and the sleep settings were put back.
//!
//! The log is `<dir of this binary>/logs/<label>.jsonl`. Creating
//! `<dir>/logs/STOP`, or sending SIGINT or SIGTERM, ends the batch after the
//! current cycle.
use std::{
    env, fs,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const PM_TEST: &str = "/sys/power/pm_test";
const MEM_SLEEP: &str = "/sys/power/mem_sleep";
const STATE: &str = "/sys/power/state";
const ALARM: &str = "/sys/class/rtc/rtc0/wakealarm";
const STATS: &str = "/sys/power/suspend_stats";
const LOCK: &str = "/run/pm-batch.lock";
const GPU: &str = "/sys/bus/pci/devices/0000:01:00.0";
const GPU_PORT: &str = "/sys/bus/pci/devices/0000:00:01.0";
const BIN: &str = "/run/current-system/sw/bin";
/// Offsets from the ACPI PM I/O base of an Intel PCH.
const SMI_EN: u64 = 0x30;
const SLP_SMI_EN: u8 = 1 << 4;

type R<T> = Result<T, String>;

/// Set by SIGINT or SIGTERM, so a stopped batch still restores its settings.
static SIGNALLED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: i32) {
    SIGNALLED.store(true, Ordering::Relaxed);
}

extern "C" {
    fn signal(number: i32, handler: extern "C" fn(i32)) -> usize;
}

fn read(path: &str) -> String {
    fs::read_to_string(path)
        .map(|s| s.trim().to_owned())
        .unwrap_or_else(|e| format!("<{e}>"))
}

fn write(path: &str, value: &str) -> R<()> {
    fs::write(path, value).map_err(|e| format!("write {value:?} to {path}: {e}"))
}

/// The bracketed choice in a sysfs selector such as `s2idle [deep]`.
fn selected(path: &str) -> String {
    let text = read(path);
    text.split_whitespace()
        .find_map(|w| w.strip_prefix('[').and_then(|w| w.strip_suffix(']')))
        .unwrap_or(&text)
        .to_owned()
}

fn counter(name: &str) -> u64 {
    read(&format!("{STATS}/{name}")).parse().unwrap_or(u64::MAX)
}

fn wall() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn uptime() -> f64 {
    read("/proc/uptime")
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0)
}

fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Output of a system tool, or a bracketed note when it cannot run.
fn command(program: &str, args: &[&str]) -> String {
    match Command::new(format!("{BIN}/{program}")).args(args).output() {
        Ok(out) => format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
        .trim()
        .to_owned(),
        Err(e) => format!("<{program}: {e}>"),
    }
}

/// PM1a event block address from the FADT; the PCH's other power-management
/// registers sit at fixed offsets from it.
fn pm_base() -> R<u64> {
    let fadt = fs::read("/sys/firmware/acpi/tables/FACP").map_err(|e| format!("FADT: {e}"))?;
    let bytes: [u8; 4] = fadt
        .get(56..60)
        .and_then(|b| b.try_into().ok())
        .ok_or("FADT too short")?;
    match u32::from_le_bytes(bytes) {
        0 => Err("FADT has no PM1a event block".into()),
        base => Ok(u64::from(base)),
    }
}

fn port_read(address: u64, length: usize) -> R<Vec<u8>> {
    let mut bytes = vec![0; length];
    File::open("/dev/port")
        .and_then(|mut port| {
            port.seek(SeekFrom::Start(address))?;
            port.read_exact(&mut bytes)
        })
        .map_err(|e| format!("read port {address:#x}: {e}"))?;
    Ok(bytes)
}

fn port_write(address: u64, byte: u8) -> R<()> {
    OpenOptions::new()
        .write(true)
        .open("/dev/port")
        .and_then(|mut port| {
            port.seek(SeekFrom::Start(address))?;
            port.write_all(&[byte])
        })
        .map_err(|e| format!("write port {address:#x}: {e}"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Sets or clears SLP_SMI_EN and returns the low SMI_EN byte read back.
fn slp_smi(base: u64, enabled: bool) -> R<u8> {
    let now = port_read(base + SMI_EN, 1)?[0];
    let wanted = if enabled {
        now | SLP_SMI_EN
    } else {
        now & !SLP_SMI_EN
    };
    if wanted != now {
        port_write(base + SMI_EN, wanted)?;
    }
    let after = port_read(base + SMI_EN, 1)?[0];
    if (after & SLP_SMI_EN != 0) != enabled {
        return Err(format!(
            "SLP_SMI_EN did not change: SMI_EN low byte {after:#04x}"
        ));
    }
    Ok(after)
}

fn entries(dir: &str) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// One line per entry of `dir`, each showing the listed attributes.
fn table(dir: &str, attributes: &[&str]) -> String {
    entries(dir)
        .iter()
        .map(|name| {
            let cells: Vec<String> = attributes
                .iter()
                .map(|a| format!("{a}={}", read(&format!("{dir}/{name}/{a}"))))
                .collect();
            format!("{name} {}", cells.join(" "))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn files(dir: &str, skip: &[&str]) -> String {
    entries(dir)
        .iter()
        .filter(|name| !skip.contains(&name.as_str()))
        .map(|name| format!("# {name}\n{}", read(&format!("{dir}/{name}"))))
        .collect::<Vec<_>>()
        .join("\n")
}

fn snapshot(dir: &Path, label: &str) -> R<()> {
    let mut parts: Vec<(String, String)> = Vec::new();
    let mut add = |title: &str, body: String| parts.push((title.to_owned(), body));
    add("uptime", read("/proc/uptime"));
    add("wall", format!("{:.3}", wall()));
    add("boot", read("/proc/sys/kernel/random/boot_id"));
    add("kernel", read("/proc/sys/kernel/osrelease"));
    add("loadavg", read("/proc/loadavg"));
    add("meminfo", read("/proc/meminfo"));
    add("suspend_stats", files(STATS, &[]));
    add("acpi wakeup", read("/proc/acpi/wakeup"));
    add("lid", read("/proc/acpi/button/lid/LID0/state"));
    add(
        "clocksource",
        read("/sys/devices/system/clocksource/clocksource0/current_clocksource"),
    );
    add(
        "pci",
        table(
            "/sys/bus/pci/devices",
            &["power_state", "power/control", "power/runtime_status"],
        ),
    );
    add(
        "usb",
        table(
            "/sys/bus/usb/devices",
            &["power/control", "power/runtime_status"],
        ),
    );
    add(
        "pmc_core",
        files(
            "/sys/kernel/debug/pmc_core",
            &["ltr_ignore", "ltr_restore", "slp_s0_dbg_latch"],
        ),
    );
    add(
        "cpuidle cpu0",
        table(
            "/sys/devices/system/cpu/cpu0/cpuidle",
            &["name", "usage", "time"],
        ),
    );
    add(
        "ec ram",
        fs::read("/sys/kernel/debug/ec/ec0/io")
            .map(|ram| {
                ram.chunks(16)
                    .enumerate()
                    .map(|(row, bytes)| format!("{:02x}: {}", row * 16, hex(bytes)))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_else(|e| format!("<{e}>")),
    );
    add("mei", table("/sys/class/mei", &["dev_state", "fw_status"]));
    add(
        "power supply",
        table("/sys/class/power_supply", &["uevent"]),
    );
    add("thermal", table("/sys/class/thermal", &["type", "temp"]));
    add(
        "acpi interrupts (non-zero)",
        entries("/sys/firmware/acpi/interrupts")
            .iter()
            .map(|n| (n, read(&format!("/sys/firmware/acpi/interrupts/{n}"))))
            .filter(|(_, v)| !v.starts_with("0 ") && !v.starts_with('<'))
            .map(|(n, v)| format!("{n} {v}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    add(
        "pch registers",
        match pm_base() {
            Ok(base) => [
                ("PM1_STS_EN", 0x00, 4),
                ("PM1_CNT", 0x04, 4),
                ("SMI_EN", SMI_EN, 4),
                ("SMI_STS", 0x34, 4),
                ("TCO1_STS", 0x64, 2),
                ("TCO2_STS", 0x66, 2),
                ("GPE0_STS", 0x80, 16),
                ("GPE0_EN", 0x90, 16),
            ]
            .iter()
            .map(|(name, offset, length)| {
                let value = port_read(base + offset, *length)
                    .map(|b| hex(&b))
                    .unwrap_or_else(|e| format!("<{e}>"));
                format!("{name} (bytes from {:#x}) {value}", base + offset)
            })
            .collect::<Vec<_>>()
            .join("\n"),
            Err(e) => format!("<{e}>"),
        },
    );
    add(
        "disk power mode",
        entries("/sys/block")
            .iter()
            .filter(|n| n.starts_with("sd"))
            .map(|n| {
                let out = command("smartctl", &["-n", "standby", "-i", &format!("/dev/{n}")]);
                let line = out
                    .lines()
                    .filter(|l| l.contains("Power mode") || l.contains("STANDBY"))
                    .collect::<Vec<_>>()
                    .join("; ");
                format!(
                    "{n} {} | {line}",
                    read(&format!("/sys/block/{n}/device/model"))
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    );
    add("modules", {
        let mut names: Vec<String> = read("/proc/modules")
            .lines()
            .filter_map(|l| l.split(' ').next().map(str::to_owned))
            .collect();
        names.sort();
        names.join(" ")
    });
    add("zpool", command("zpool", &["status", "-x"]));
    add("jobs", command("systemctl", &["list-jobs", "--no-legend"]));

    let path = dir.join(format!("{label}.snapshot.txt"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| format!("{}: {e} (labels are never reused)", path.display()))?;
    for (title, body) in parts {
        writeln!(file, "## {title}\n{body}\n").map_err(|e| e.to_string())?;
    }
    file.sync_all().map_err(|e| e.to_string())?;
    println!("snapshot {}", path.display());
    Ok(())
}

struct Log(File);

impl Log {
    /// Appends one JSON object and forces it to disk before returning.
    fn line(&mut self, fields: &[(&str, String)]) -> R<()> {
        let body: Vec<String> = fields
            .iter()
            .map(|(k, v)| format!("{}:{v}", quote(k)))
            .collect();
        writeln!(self.0, "{{{}}}", body.join(","))
            .and_then(|()| self.0.sync_all())
            .map_err(|e| format!("log write: {e}"))
    }
}

/// Puts the sleep settings back however the batch ends.
struct Restore {
    mem_sleep: String,
    /// PM base when this batch clears SLP_SMI_EN.
    smi: Option<u64>,
}

impl Restore {
    /// Attempts every setting and reports the ones that failed.
    fn apply(&self) -> R<()> {
        let failures: Vec<String> = [
            write(PM_TEST, "none"),
            write(MEM_SLEEP, &self.mem_sleep),
            write(ALARM, "0"),
            self.smi
                .map_or(Ok(()), |base| slp_smi(base, true).map(drop)),
        ]
        .into_iter()
        .filter_map(Result::err)
        .collect();
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}

/// Covers the paths that leave `run` early with an error.
impl Drop for Restore {
    fn drop(&mut self) {
        if let Err(error) = self.apply() {
            eprintln!("restore failed: {error}");
        }
    }
}

fn modules() -> String {
    let wanted = [
        "nouveau",
        "nvidia",
        "i915",
        "mei_me",
        "ath10k_pci",
        "btusb",
        "r8169",
        "xhci_pci",
        "acer_wmi",
        "acer_wmi_ph31751",
        "ec_sys",
        "pm_stage",
        "netconsole",
        "tpm_crb",
    ];
    let loaded = read("/proc/modules");
    wanted
        .iter()
        .filter(|m| loaded.lines().any(|l| l.split(' ').next() == Some(m)))
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

fn require_gpu_off() -> R<()> {
    for dev in [GPU, GPU_PORT] {
        if read(&format!("{dev}/power_state")) != "D3cold"
            || read(&format!("{dev}/power/runtime_status")) != "suspended"
            || read(&format!("{dev}/power/control")) != "auto"
        {
            return Err(format!("{dev} is not runtime suspended in D3cold on auto"));
        }
    }
    Ok(())
}

fn run() -> R<()> {
    let args: Vec<String> = env::args().collect();
    let is_snapshot = args.len() == 3 && args[2] == "snapshot";
    if !is_snapshot && !(4..=7).contains(&args.len()) {
        return Err(
            "usage: pm-batch <label> <dry|s3|s2idle> <cycles> [wake_s=30] [gap_s=30] [nosmi|gpu-off]\n       pm-batch <label> snapshot"
                .into(),
        );
    }
    let label = &args[1];
    if label.is_empty() || !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("label: letters, digits and dashes only".into());
    }
    let dir: PathBuf = env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("binary has no directory")?
        .join("logs");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if is_snapshot {
        return snapshot(&dir, label);
    }
    let mode = args[2].as_str();
    let (pm_test, sleep_kind) = match mode {
        "dry" => ("core", "deep"),
        "s3" => ("none", "deep"),
        "s2idle" => ("none", "s2idle"),
        _ => return Err(format!("unknown mode {mode}")),
    };
    let number = |i: usize, default: u64| -> R<u64> {
        args.get(i).map_or(Ok(default), |s| {
            s.parse().map_err(|_| format!("bad number {s}"))
        })
    };
    let cycles = number(3, 0)?;
    let wake = number(4, 30)?;
    let gap = number(5, 30)?;
    if cycles == 0 || cycles > 500 || !(15..=3600).contains(&wake) || !(5..=7200).contains(&gap) {
        return Err("cycles 1..=500, wake 15..=3600 s, gap 5..=7200 s".into());
    }
    if selected(PM_TEST) != "none" {
        return Err(format!("pm_test is already {}", read(PM_TEST)));
    }
    if !read(ALARM).is_empty() {
        return Err(format!("a wake alarm is already armed: {}", read(ALARM)));
    }

    let (no_smi, gpu_off) = match args.get(6).map(String::as_str) {
        None => (false, false),
        Some("nosmi") if mode != "dry" => (true, false),
        Some("gpu-off") if mode == "s3" && cycles == 1 => (false, true),
        Some(other) => return Err(format!("unknown or misplaced option {other}")),
    };
    let pm_base = pm_base()?;
    // A killed `nosmi` batch leaves the bit clear, which would silently turn
    // the next control into another `nosmi` run.
    let smi_en = port_read(pm_base + SMI_EN, 1)?[0];
    if smi_en & SLP_SMI_EN == 0 {
        return Err(format!(
            "SLP_SMI_EN is already clear (SMI_EN low byte {smi_en:#04x}); reboot to restore the firmware's setting"
        ));
    }
    let smi_base = no_smi.then_some(pm_base);
    let pools = command("zpool", &["status", "-x"]);
    if pools != "all pools are healthy" {
        return Err(format!("pools are not healthy: {pools}"));
    }
    let stop = dir.join("STOP");
    if stop.exists() {
        return Err(format!("{} exists; remove it first", stop.display()));
    }
    // The kernel drops this lock when the process ends, however it ends.
    let lock = File::create(LOCK).map_err(|e| format!("{LOCK}: {e}"))?;
    lock.try_lock()
        .map_err(|e| format!("another batch holds {LOCK}: {e}"))?;
    // An unattended trial should outlast a unit that happens to be starting.
    let patience = Instant::now() + Duration::from_secs(600);
    loop {
        let jobs = command("systemctl", &["list-jobs", "--no-legend"]);
        if jobs.is_empty() {
            break;
        }
        if gpu_off || Instant::now() >= patience {
            return Err(format!("systemd still has jobs in progress: {jobs}"));
        }
        thread::sleep(Duration::from_secs(10));
    }
    // From here a signal only ends the batch between cycles.
    unsafe {
        signal(2, on_signal);
        signal(15, on_signal);
    }
    let restore = Restore {
        mem_sleep: selected(MEM_SLEEP),
        smi: smi_base,
    };
    let path = dir.join(format!("{label}.jsonl"));
    let file = OpenOptions::new()
        .append(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| format!("{}: {e} (labels are never reused)", path.display()))?;
    File::open(&dir)
        .and_then(|d| d.sync_all())
        .map_err(|e| e.to_string())?;
    let mut log = Log(file);

    write(MEM_SLEEP, sleep_kind)?;
    write(PM_TEST, pm_test)?;
    let gpu = |name: &str| quote(&read(&format!("{GPU}/{name}")));
    let port = |name: &str| quote(&read(&format!("{GPU_PORT}/{name}")));
    log.line(&[
        ("event", quote("start")),
        ("label", quote(label)),
        ("mode", quote(mode)),
        ("cycles", cycles.to_string()),
        ("wake_s", wake.to_string()),
        ("gap_s", gap.to_string()),
        ("nosmi", no_smi.to_string()),
        ("gpu_off", gpu_off.to_string()),
        ("smi_en_low", quote(&format!("{smi_en:#04x}"))),
        ("wall", format!("{:.3}", wall())),
        ("uptime", format!("{:.2}", uptime())),
        ("boot", quote(&read("/proc/sys/kernel/random/boot_id"))),
        ("kernel", quote(&read("/proc/sys/kernel/osrelease"))),
        ("cmdline", quote(&read("/proc/cmdline"))),
        (
            "system",
            quote(
                &fs::canonicalize("/run/current-system")
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
            ),
        ),
        ("mem_sleep", quote(&read(MEM_SLEEP))),
        ("pm_test", quote(&read(PM_TEST))),
        ("pm_async", quote(&read("/sys/power/pm_async"))),
        ("gpu_control", gpu("power/control")),
        ("gpu_runtime", gpu("power/runtime_status")),
        ("gpu_port_state", port("power_state")),
        ("gpu_port_runtime", port("power/runtime_status")),
        ("modules", quote(&modules())),
        ("success_before", counter("success").to_string()),
        ("fail_before", counter("fail").to_string()),
    ])?;

    let (mut abnormal, mut abnormal_in_a_row) = (0, 0);
    let mut completed = 0;
    let mut reason = "ran every cycle";
    for cycle in 1..=cycles {
        if SIGNALLED.load(Ordering::Relaxed) {
            reason = "signal";
            break;
        }
        if stop.exists() {
            reason = "STOP file";
            break;
        }
        let (success, fail) = (counter("success"), counter("fail"));
        if pm_test == "none" {
            write(ALARM, "0")?;
            write(ALARM, &format!("+{wake}"))?;
        }
        let smi_armed = match smi_base {
            Some(base) => format!("{:#04x}", slp_smi(base, false)?),
            None => String::new(),
        };
        if gpu_off {
            require_gpu_off()?;
        }
        log.line(&[
            ("event", quote("enter")),
            ("cycle", cycle.to_string()),
            ("smi_en_low", quote(&smi_armed)),
            ("wall", format!("{:.3}", wall())),
            ("uptime", format!("{:.2}", uptime())),
        ])?;
        let (before, awake) = (wall(), Instant::now());
        // Blocks until the machine is running again. A hang never returns.
        let result = fs::write(STATE, "mem");
        let away = wall() - before;
        let _ = write(ALARM, "0");
        // What the firmware left in SMI_EN, before the bit is put back.
        let smi_after = smi_base.map_or(String::new(), |base| {
            port_read(base + SMI_EN, 1)
                .map_or_else(|e| format!("<{e}>"), |b| format!("{:#04x}", b[0]))
        });
        let smi_restored = smi_base.map_or(Ok(()), |base| slp_smi(base, true).map(drop));
        let (success_delta, fail_delta) = (
            counter("success").wrapping_sub(success),
            counter("fail").wrapping_sub(fail),
        );
        let slept_enough = pm_test != "none" || away + 10.0 >= wake as f64;
        let outcome = match (&result, success_delta, slept_enough) {
            (Ok(()), 1, true) => "returned",
            (Ok(()), 1, false) => "returned-early",
            (Ok(()), _, _) => "returned-uncounted",
            (Err(_), _, _) => "refused",
        };
        log.line(&[
            ("event", quote("return")),
            ("cycle", cycle.to_string()),
            ("outcome", quote(outcome)),
            ("away_s", format!("{away:.1}")),
            ("running_s", format!("{:.1}", awake.elapsed().as_secs_f64())),
            (
                "error",
                quote(
                    &result
                        .as_ref()
                        .err()
                        .map(|e| e.to_string())
                        .unwrap_or_default(),
                ),
            ),
            ("success_delta", success_delta.to_string()),
            ("fail_delta", fail_delta.to_string()),
            ("wake_irq", quote(&read("/sys/power/pm_wakeup_irq"))),
            ("smi_en_low_after", quote(&smi_after)),
            ("wall", format!("{:.3}", wall())),
        ])?;
        // The next cycle would not be the arm it claims to be.
        smi_restored.map_err(|e| format!("cycle {cycle}: {e}"))?;
        completed = cycle;
        if outcome == "returned" {
            abnormal_in_a_row = 0;
        } else {
            abnormal += 1;
            abnormal_in_a_row += 1;
        }
        if abnormal_in_a_row == 3 {
            reason = "three abnormal cycles in a row";
            break;
        }
        let next = Instant::now() + Duration::from_secs(gap);
        while cycle < cycles && Instant::now() < next && !SIGNALLED.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_secs(1));
        }
    }
    let restored = restore.apply();
    std::mem::forget(restore);
    log.line(&[
        ("event", quote("done")),
        ("completed", completed.to_string()),
        ("abnormal", abnormal.to_string()),
        ("reason", quote(reason)),
        (
            "restore_error",
            quote(restored.as_ref().err().map_or("", String::as_str)),
        ),
        ("wall", format!("{:.3}", wall())),
        ("pm_test", quote(&read(PM_TEST))),
        ("mem_sleep", quote(&read(MEM_SLEEP))),
    ])?;
    let summary = format!(
        "{completed}/{cycles} cycles, {abnormal} abnormal: {reason}; log {}",
        path.display()
    );
    restored.map_err(|e| format!("restore failed: {e}; {summary}"))?;
    if completed == cycles && abnormal == 0 {
        println!("{summary}");
        Ok(())
    } else {
        Err(summary)
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pm-batch: {error}");
            ExitCode::FAILURE
        }
    }
}
