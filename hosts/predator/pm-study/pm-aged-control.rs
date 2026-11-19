// Guarded suspend study trials and awake GPU measurement.
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::thread::sleep;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const RUNNER: &str = "/home/mirsella/dev/scratch/predator-suspend/pm-batch";
const GPU_OFF_RUNNER: &str = "/home/mirsella/dev/scratch/predator-suspend/pm-batch-gpu-off";
const GPU: &str = "/sys/bus/pci/devices/0000:01:00.0";
const PORT: &str = "/sys/bus/pci/devices/0000:00:01.0";
const GPU_MEASUREMENT: Duration = Duration::from_secs(25 * 60);
const MAINTENANCE: &[&str] = &[
    "db-backup.service",
    "ente-db-backup.service",
    "hermes-backup.service",
    "sanoid.service",
    "restic-backups-fast.service",
    "recovery-backup.service",
    "restic-check.service",
    "zfs-scrub.service",
    "zpool-trim.service",
    "fstrim.service",
];

fn require(ok: bool, why: impl Into<String>) -> Result<()> {
    if !ok {
        return Err(why.into().into());
    }
    Ok(())
}

fn command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.env("LC_ALL", "C").env("TZ", "Europe/Paris").env(
        "PATH",
        "/run/wrappers/bin:/run/current-system/sw/bin:/nix/var/nix/profiles/default/bin",
    );
    cmd
}

fn output(program: &str, args: &[&str]) -> Result<String> {
    let out = command(program).args(args).output()?;
    if !out.status.success() {
        return Err(format!(
            "{program} {args:?} exited with {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8(out.stdout)?)
}

fn read(path: &str) -> Result<String> {
    fs::read_to_string(path).map_err(|err| format!("cannot read {path}: {err}").into())
}

fn mount_rows(text: &str) -> Vec<&str> {
    let mut rows: Vec<_> = text.lines().collect();
    rows.sort_unstable();
    rows
}

fn trial_window(clock: u32) -> bool {
    (30..2300).contains(&clock)
}

enum Arm {
    AgedControl,
    ShortBypass,
    AgedBypassProbe,
    GpuMeter,
    GpuOff,
    AgedS2idle,
}

#[derive(Clone, Copy)]
enum Stage {
    Initial,
    BeforeSleep,
}

impl Stage {
    fn expected(self, arm: &Arm) -> (&'static str, &'static [&'static str]) {
        match (self, arm) {
            (Self::Initial, _) => ("0", &[]),
            (Self::BeforeSleep, Arm::AgedControl | Arm::GpuOff | Arm::AgedS2idle) => {
                ("5", &["before.snapshot.txt", "dry.jsonl"])
            }
            (Self::BeforeSleep, Arm::ShortBypass | Arm::AgedBypassProbe | Arm::GpuMeter) => {
                ("0", &["before.snapshot.txt"])
            }
        }
    }
}

fn check_trial_logs(logs: &Path, label: &str, expected: &[&str]) -> Result<()> {
    let prefix = format!("{label}-");
    let mut found = Vec::new();
    for entry in fs::read_dir(logs)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == "STOP" {
            return Err("runner STOP flag exists".into());
        }
        if let Some(suffix) = name.to_string_lossy().strip_prefix(&prefix) {
            require(
                entry.file_type()?.is_file(),
                format!("not a regular trial file: {}", entry.path().display()),
            )?;
            found.push(suffix.to_owned());
        }
    }
    found.sort_unstable();
    require(
        found == expected,
        format!("{label} trial files: expected {expected:?}, found {found:?}"),
    )
}

impl Arm {
    fn from_label(label: &str) -> Result<Self> {
        for (prefix, arm) in [
            ("aged-", Self::AgedControl),
            ("nosmi-short-", Self::ShortBypass),
            ("nosmi-aged-probe-", Self::AgedBypassProbe),
            ("gpu-meter-", Self::GpuMeter),
            ("gpu-off-", Self::GpuOff),
            ("s2idle-aged-", Self::AgedS2idle),
        ] {
            if label
                .strip_prefix(prefix)
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            {
                return Ok(arm);
            }
        }
        Err(format!("invalid trial label: {label}").into())
    }

    fn check_age(&self, uptime: f64) -> Result<()> {
        match self {
            Self::AgedControl | Self::AgedBypassProbe | Self::GpuOff | Self::AgedS2idle => {
                require(uptime >= 32_400.0, "less than nine hours awake")
            }
            Self::ShortBypass => require(
                (0.0..14_400.0).contains(&uptime),
                "short bypass requires less than four hours since boot",
            ),
            Self::GpuMeter => Ok(()),
        }
    }

    fn runner(&self) -> &'static str {
        match self {
            Self::GpuOff => GPU_OFF_RUNNER,
            _ => RUNNER,
        }
    }

    fn sleep_args(&self) -> &'static [&'static str] {
        match self {
            Self::AgedControl => &["s3", "1", "120", "30"],
            Self::ShortBypass => &["s3", "3", "60", "30", "nosmi"],
            Self::AgedBypassProbe => &["s3", "1", "120", "30", "nosmi"],
            Self::GpuOff => &["s3", "1", "120", "30", "gpu-off"],
            Self::AgedS2idle => &["s2idle", "1", "120", "30"],
            Self::GpuMeter => unreachable!("awake measurement must not call suspend"),
        }
    }
}

struct Baseline<'a> {
    arm: Arm,
    label: &'a str,
    boot: &'a str,
    system: &'a str,
    runner_hash: &'a str,
    storage: &'a Path,
}

impl Baseline<'_> {
    fn preflight(&self, stage: Stage, planned: Option<&str>) -> Result<()> {
        let (successes, expected_logs) = stage.expected(&self.arm);
        require(
            read("/proc/sys/kernel/random/boot_id")?.trim() == self.boot,
            "boot changed",
        )?;
        for path in [
            "/run/current-system",
            "/run/booted-system",
            "/nix/var/nix/profiles/system",
        ] {
            require(
                fs::canonicalize(path)? == Path::new(self.system),
                format!("system changed: {path}"),
            )?;
        }
        for (file, expected) in [("success", successes), ("fail", "0")] {
            let value = read(&format!("/sys/power/suspend_stats/{file}"))?;
            require(
                value.trim() == expected,
                format!(
                    "suspend {file}: expected {expected}, found {}",
                    value.trim()
                ),
            )?;
        }
        let uptime = read("/proc/uptime")?;
        let awake: f64 = uptime
            .split_whitespace()
            .next()
            .ok_or("missing uptime")?
            .parse()?;
        // All arms start from a boot with no intervening suspends.
        let clock: u32 = output("date", &["+%H%M"])?.trim().parse()?;
        require(trial_window(clock), "backup exclusion window")?;
        let age_at_start = if let Some(slot) = planned {
            require(
                matches!(self.arm, Arm::GpuOff | Arm::AgedS2idle),
                "prepare is only for GPU-off or aged s2idle",
            )?;
            let now: i64 = output("date", &["+%s"])?.trim().parse()?;
            let when: i64 = output("date", &["--date", slot, "+%s"])?.trim().parse()?;
            let clock = output("date", &["--date", slot, "+%H%M"])?.trim().parse()?;
            require(trial_window(clock), "planned time overlaps backups")?;
            require(when > now, "planned time is not in the future")?;
            awake + (when - now) as f64
        } else {
            awake
        };
        self.arm.check_age(age_at_start)?;
        if matches!(self.arm, Arm::GpuMeter) {
            require(clock < 2230, "GPU measurement overlaps the backup window")?;
        }
        if matches!(self.arm, Arm::GpuOff | Arm::AgedS2idle) {
            let sleep = read("/sys/power/mem_sleep")?;
            require(
                read("/sys/power/pm_test")?
                    .split_whitespace()
                    .any(|v| v == "[none]")
                    && sleep.split_whitespace().any(|v| v == "[deep]")
                    && read("/sys/class/rtc/rtc0/wakealarm")?.trim().is_empty(),
                "sleep mode changed or an RTC alarm is already armed",
            )?;
            if matches!(self.arm, Arm::AgedS2idle) {
                require(
                    sleep.split_whitespace().any(|v| v == "s2idle"),
                    "s2idle is not available",
                )?;
                check_gpu_d0()?;
            }
        }

        let logs = Path::new(RUNNER).with_file_name("logs");
        check_trial_logs(&logs, self.label, expected_logs)?;
        let runner_hash = output("sha256sum", &[self.arm.runner()])?;
        require(
            runner_hash.split_whitespace().next() == Some(self.runner_hash),
            "runner changed",
        )?;
        let inhibitors = output("systemd-inhibit", &["--list", "--no-legend", "--no-pager"])?;
        require(
            !inhibitors
                .lines()
                .any(|row| matches!(row.split_whitespace().last(), Some("block" | "block-weak"))),
            format!("blocking inhibitor: {inhibitors}"),
        )?;
        require(
            output("zpool", &["status", "-x"])?.trim() == "all pools are healthy",
            "pools not healthy",
        )?;
        require(
            !output("zpool", &["status"])?.contains("in progress"),
            "storage scan in progress",
        )?;
        let guids = output(
            "zpool",
            &[
                "get",
                "-Hp",
                "-o",
                "name,property,value",
                "guid",
                "fast",
                "tank",
                "backups",
            ],
        )?;
        require(
            guids == fs::read_to_string(self.storage.join("pool-guids.txt"))?,
            "pool identity changed",
        )?;
        let mounts = output("findmnt", &["-rn", "-t", "zfs", "-o", "SOURCE,TARGET"])?;
        let saved = fs::read_to_string(self.storage.join("zfs-mounts.txt"))?;
        require(
            mount_rows(&mounts) == mount_rows(&saved),
            format!("mounts changed:\nsaved:\n{saved}live:\n{mounts}"),
        )?;

        let mut args = vec![
            "list-units",
            "--type=service",
            "--state=active,activating,deactivating",
            "--no-legend",
            "--no-pager",
        ];
        args.extend(MAINTENANCE);
        let maintenance = output("systemctl", &args)?;
        require(
            maintenance.trim().is_empty(),
            format!("maintenance active: {maintenance}"),
        )?;
        let failed = output("systemctl", &["--failed", "--no-legend", "--no-pager"])?;
        require(
            failed.trim().is_empty(),
            format!("failed units present: {failed}"),
        )?;
        let cloud = output("nextcloud-occ", &["status", "--output=json"])?;
        require(
            cloud.contains("\"maintenance\":false"),
            "Nextcloud maintenance or unknown status",
        )?;
        print!("{}", output("predator-boot", &["audit"])?);
        println!(
            "{} preflight passed, uptime {awake:.0} seconds, expected successes {successes}",
            self.label
        );
        if let Some(slot) = planned {
            println!("Projected awake time at {slot}: {age_at_start:.0} seconds");
        }
        Ok(())
    }

    fn batch(&self, suffix: &str, args: &[&str]) -> Result<()> {
        let status = command(self.arm.runner())
            .arg(format!("{}-{suffix}", self.label))
            .args(args)
            .status()?;
        require(
            status.success(),
            format!("{suffix}: runner exited with {status}"),
        )
    }

    fn run(&self) -> Result<()> {
        self.preflight(Stage::Initial, None)?;
        if matches!(self.arm, Arm::GpuMeter) {
            return self.measure_gpu();
        }
        self.batch("before", &["snapshot"])?;
        let dry = match self.arm {
            Arm::AgedControl | Arm::GpuOff | Arm::AgedS2idle => {
                self.batch("dry", &["dry", "5", "30", "10"])
            }
            _ => Ok(()),
        };
        let sleep = self.arm.sleep_args();
        let result = dry
            .and_then(|()| self.preflight(Stage::BeforeSleep, None))
            .and_then(|()| {
                if matches!(self.arm, Arm::GpuOff) {
                    self.enable_gpu_pm()
                } else {
                    Ok(())
                }
            })
            .and_then(|()| output("sync", &[]).map(|_| ()))
            .and_then(|()| self.batch(sleep[0], sleep));
        let after = self.batch("after", &["snapshot"]);
        if let (Err(first), Err(second)) = (&result, &after) {
            eprintln!(
                "{}: {first}; after snapshot also failed: {second}",
                self.label
            );
        }
        result.and(after)
    }

    fn gpu_restore_marker(&self) -> PathBuf {
        PathBuf::from(format!("/run/pm-{}/restore-on", self.label))
    }

    fn gpu_meter_preflight(&self) -> Result<()> {
        require(
            output("systemctl", &["list-jobs", "--no-legend", "--no-pager"])?
                .trim()
                .is_empty(),
            "systemd jobs are running",
        )?;
        let timers = output(
            "systemctl",
            &["list-timers", "--no-legend", "--no-pager", "pm-*"],
        )?;
        require(timers.trim().is_empty(), "a study timer is armed")?;
        check_gpu_d0()
    }

    fn enable_gpu_pm(&self) -> Result<()> {
        check_gpu_d0()?;
        // Arm systemd's ExecStopPost before making any change.
        fs::File::create_new(self.gpu_restore_marker())?;
        fs::write(format!("{GPU}/power/control"), "auto")?;
        wait_gpu_state("D3cold", "suspended", Duration::from_secs(45))?;
        println!("GPU policy set to auto; both GPU and port in D3cold");
        Ok(())
    }

    fn check(&self, planned: Option<&str>) -> Result<()> {
        self.preflight(Stage::Initial, planned)?;
        match self.arm {
            Arm::GpuMeter => self.gpu_meter_preflight(),
            Arm::GpuOff => check_gpu_d0(),
            _ => Ok(()),
        }
    }

    fn restore_gpu(&self) -> Result<()> {
        require(
            matches!(self.arm, Arm::GpuMeter | Arm::GpuOff),
            "not a GPU trial",
        )?;
        if !self.gpu_restore_marker().try_exists()? {
            return Ok(());
        }
        check_unbound_gpu()?;
        fs::write(format!("{GPU}/power/control"), "on")?;
        require(
            read(&format!("{GPU}/power/control"))?.trim() == "on",
            "GPU policy did not restore to on",
        )?;
        wait_gpu_state("D0", "active", Duration::from_secs(20))?;
        println!("GPU policy restored to on; both GPU and port back in D0");
        Ok(())
    }

    fn measure_gpu(&self) -> Result<()> {
        self.gpu_meter_preflight()?;
        let logs = Path::new(RUNNER).with_file_name("logs");
        let mut log = fs::File::create_new(logs.join(format!("{}-awake.txt", self.label)))?;
        gpu_event(&mut log, "D0 baseline")?;
        self.enable_gpu_pm()?;
        let start = Instant::now();
        gpu_event(&mut log, "both GPU and port in D3cold; measurement begins")?;
        while start.elapsed() < GPU_MEASUREMENT {
            sleep(
                GPU_MEASUREMENT
                    .saturating_sub(start.elapsed())
                    .min(Duration::from_secs(30)),
            );
            require(!logs.join("STOP").try_exists()?, "runner STOP flag exists")?;
            require(gpu_state("D3cold", "suspended")?, "GPU or port left D3cold")?;
            gpu_event(&mut log, "D3cold sample")?;
        }
        gpu_event(&mut log, "25-minute D3cold measurement complete")
    }
}

fn check_gpu_d0() -> Result<()> {
    check_unbound_gpu()?;
    require(
        read(&format!("{GPU}/power/control"))?.trim() == "on"
            && read(&format!("{PORT}/power/control"))?.trim() == "auto"
            && read(&format!("{GPU}/d3cold_allowed"))?.trim() == "1"
            && read(&format!("{PORT}/d3cold_allowed"))?.trim() == "1"
            && gpu_state("D0", "active")?,
        "GPU and port do not match the awake D0 baseline",
    )
}

fn check_unbound_gpu() -> Result<()> {
    let unbound = match fs::symlink_metadata(format!("{GPU}/driver")) {
        Ok(_) => false,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => true,
        Err(err) => return Err(err.into()),
    };
    require(
        read(&format!("{GPU}/vendor"))?.trim() == "0x10de"
            && read(&format!("{GPU}/device"))?.trim() == "0x1c20"
            && unbound,
        "the expected unbound NVIDIA GPU is not present",
    )
}

fn gpu_state(state: &str, runtime: &str) -> Result<bool> {
    for dev in [GPU, PORT] {
        if read(&format!("{dev}/power_state"))?.trim() != state
            || read(&format!("{dev}/power/runtime_status"))?.trim() != runtime
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn wait_gpu_state(state: &str, runtime: &str, timeout: Duration) -> Result<()> {
    let start = Instant::now();
    loop {
        if gpu_state(state, runtime)? {
            return Ok(());
        }
        require(
            start.elapsed() < timeout,
            format!("GPU or port did not reach {state}"),
        )?;
        sleep(Duration::from_secs(1));
    }
}

fn gpu_event(log: &mut fs::File, event: &str) -> Result<()> {
    let stamp = output("date", &["--iso-8601=seconds"])?;
    writeln!(log, "{} {event}", stamp.trim())?;
    log.sync_data()?;
    println!("{event}");
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<_> = env::args().collect();
    let [_, mode, label, boot, system, runner_hash, storage, rest @ ..] = args.as_slice() else {
        eprintln!("Usage: pm-aged-control <check|prepare|run|gpu-restore> <label> <boot-id> <system> <runner-sha256> <storage-records> [planned-time for prepare]");
        return ExitCode::FAILURE;
    };
    let arm = match Arm::from_label(label) {
        Ok(arm) => arm,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };
    let baseline = Baseline {
        arm,
        label,
        boot,
        system,
        runner_hash,
        storage: Path::new(storage),
    };
    let result = match (mode.as_str(), rest) {
        ("check", []) => baseline.check(None),
        ("prepare", [slot]) => baseline.check(Some(slot)),
        ("run", []) => baseline.run(),
        ("gpu-restore", []) => baseline.restore_gpu(),
        _ => Err(format!("bad operation or arguments: {mode}").into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{label} stopped: {err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{check_trial_logs, mount_rows, Arm, Stage};
    use std::fs;

    #[test]
    fn trial_files_follow_the_stage_not_the_suspend_counter() {
        let logs =
            std::env::temp_dir().join(format!("pm-aged-control-test-{}", std::process::id()));
        fs::create_dir(&logs).unwrap();
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).expect("clean up the trial log fixture");
            }
        }
        let _cleanup = Cleanup(logs.clone());
        let short = Arm::ShortBypass;
        let aged = Arm::AgedControl;
        let initial = Stage::Initial.expected(&short);
        let before_sleep = Stage::BeforeSleep.expected(&short);
        assert_eq!(initial.0, before_sleep.0);

        fs::write(logs.join("nosmi-short-1-s3.jsonl"), "").unwrap();
        assert!(check_trial_logs(&logs, "nosmi-short-2", initial.1).is_ok());
        assert!(check_trial_logs(&logs, "nosmi-short-2", before_sleep.1).is_err());
        fs::write(logs.join("nosmi-short-2-before.snapshot.txt"), "").unwrap();
        assert!(check_trial_logs(&logs, "nosmi-short-2", initial.1).is_err());
        assert!(check_trial_logs(&logs, "nosmi-short-2", before_sleep.1).is_ok());
        fs::write(logs.join("nosmi-short-2-s3.jsonl"), "").unwrap();
        assert!(check_trial_logs(&logs, "nosmi-short-2", before_sleep.1).is_err());

        let probe = Arm::AgedBypassProbe;
        fs::write(logs.join("nosmi-aged-probe-1-before.snapshot.txt"), "").unwrap();
        let (successes, expected) = Stage::BeforeSleep.expected(&probe);
        assert_eq!(successes, "0");
        assert!(check_trial_logs(&logs, "nosmi-aged-probe-1", expected).is_ok());

        let (successes, expected) = Stage::BeforeSleep.expected(&aged);
        assert_eq!(successes, "5");
        fs::write(logs.join("aged-6-before.snapshot.txt"), "").unwrap();
        assert!(check_trial_logs(&logs, "aged-6", expected).is_err());
        fs::write(logs.join("aged-6-dry.jsonl"), "").unwrap();
        assert!(check_trial_logs(&logs, "aged-6", expected).is_ok());
        for arm in [Arm::GpuOff, Arm::AgedS2idle] {
            assert_eq!(Stage::BeforeSleep.expected(&arm), (successes, expected));
        }
        fs::create_dir(logs.join("STOP")).unwrap();
        assert!(check_trial_logs(&logs, "aged-6", expected).is_err());
    }

    #[test]
    fn each_label_selects_its_own_age_window() {
        let control = Arm::from_label("aged-5").unwrap();
        assert!(control.check_age(32_399.0).is_err());
        assert!(control.check_age(32_400.0).is_ok());
        let short = Arm::from_label("nosmi-short-1").unwrap();
        assert!(short.check_age(14_399.0).is_ok());
        assert!(short.check_age(14_400.0).is_err());
        let probe = Arm::from_label("nosmi-aged-probe-1").unwrap();
        assert!(probe.check_age(32_399.0).is_err());
        assert!(probe.check_age(32_400.0).is_ok());
        let meter = Arm::from_label("gpu-meter-1").unwrap();
        assert!(matches!(meter, Arm::GpuMeter));
        assert!(meter.check_age(10.0).is_ok());
        let gpu_off = Arm::from_label("gpu-off-1").unwrap();
        assert!(gpu_off.check_age(32_399.0).is_err());
        assert!(gpu_off.check_age(32_400.0).is_ok());
        let s2idle = Arm::from_label("s2idle-aged-1").unwrap();
        assert!(matches!(s2idle, Arm::AgedS2idle));
        assert!(s2idle.check_age(32_399.0).is_err());
        assert!(s2idle.check_age(32_400.0).is_ok());
        assert!(Arm::from_label("nosmi-short-").is_err());
        assert!(Arm::from_label("nosmi-short-1-s3").is_err());
    }

    #[test]
    fn mount_order_is_ignored_but_mount_changes_are_not() {
        let saved = "backups/data/recovery /srv/backup/recovery\nbackups/data /srv/backup\n";
        let live = "backups/data /srv/backup\nbackups/data/recovery /srv/backup/recovery\n";
        assert_eq!(mount_rows(saved), mount_rows(live));
        assert_ne!(mount_rows(saved), mount_rows("backups/data /srv/backup\n"));
        assert_ne!(
            mount_rows(saved),
            mount_rows(&live.replace("/srv/backup/recovery", "/other"))
        );
    }

    #[test]
    fn aged_s2idle_requests_only_one_rtc_wake_without_a_hardware_override() {
        let arm = Arm::AgedS2idle;
        assert_eq!(arm.runner(), super::RUNNER);
        assert_eq!(arm.sleep_args(), ["s2idle", "1", "120", "30"]);
    }
}
