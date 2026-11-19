// Guarded suspend study trials. "check" runs the initial preflight only.
use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const RUNNER: &str = "/home/mirsella/dev/scratch/predator-suspend/pm-batch";
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

enum Arm {
    AgedControl,
    ShortBypass,
    AgedBypassProbe,
}

#[derive(Clone, Copy)]
enum Stage {
    Initial,
    BeforeS3,
}

impl Stage {
    fn expected(self, arm: &Arm) -> (&'static str, &'static [&'static str]) {
        match (self, arm) {
            (Self::Initial, _) => ("0", &[]),
            (Self::BeforeS3, Arm::AgedControl) => ("5", &["before.snapshot.txt", "dry.jsonl"]),
            (Self::BeforeS3, Arm::ShortBypass | Arm::AgedBypassProbe) => {
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
            Self::AgedControl | Self::AgedBypassProbe => {
                require(uptime >= 32_400.0, "less than nine hours awake")
            }
            Self::ShortBypass => require(
                (0.0..14_400.0).contains(&uptime),
                "short bypass requires less than four hours since boot",
            ),
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
    fn preflight(&self, stage: Stage) -> Result<()> {
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
        self.arm.check_age(awake)?;
        let clock: u32 = output("date", &["+%H%M"])?.trim().parse()?;
        require((30..2300).contains(&clock), "backup exclusion window")?;

        let logs = Path::new(RUNNER).with_file_name("logs");
        check_trial_logs(&logs, self.label, expected_logs)?;
        let runner_hash = output("sha256sum", &[RUNNER])?;
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
        Ok(())
    }

    fn batch(&self, suffix: &str, args: &[&str]) -> Result<()> {
        let status = command(RUNNER)
            .arg(format!("{}-{suffix}", self.label))
            .args(args)
            .status()?;
        require(
            status.success(),
            format!("{suffix}: runner exited with {status}"),
        )
    }

    fn run(&self) -> Result<()> {
        self.preflight(Stage::Initial)?;
        self.batch("before", &["snapshot"])?;
        let dry = match self.arm {
            Arm::AgedControl => self.batch("dry", &["dry", "5", "30", "10"]),
            Arm::ShortBypass | Arm::AgedBypassProbe => Ok(()),
        };
        let s3: &[&str] = match self.arm {
            Arm::AgedControl => &["s3", "1", "120", "30"],
            Arm::ShortBypass => &["s3", "3", "60", "30", "nosmi"],
            Arm::AgedBypassProbe => &["s3", "1", "120", "30", "nosmi"],
        };
        let result = dry
            .and_then(|()| self.preflight(Stage::BeforeS3))
            .and_then(|()| output("sync", &[]).map(|_| ()))
            .and_then(|()| self.batch("s3", s3));
        let after = self.batch("after", &["snapshot"]);
        if let (Err(first), Err(second)) = (&result, &after) {
            eprintln!(
                "{}: {first}; after snapshot also failed: {second}",
                self.label
            );
        }
        result.and(after)
    }
}

fn main() -> ExitCode {
    let args: Vec<_> = env::args().collect();
    let [_, mode, label, boot, system, runner_hash, storage] = args.as_slice() else {
        eprintln!("Usage: pm-aged-control <check|run> <aged-N|nosmi-short-N|nosmi-aged-probe-N> <boot-id> <system> <runner-sha256> <storage-records>");
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
    let result = match mode.as_str() {
        "check" => baseline.preflight(Stage::Initial),
        "run" => baseline.run(),
        _ => Err(format!("unknown mode: {mode}").into()),
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
        let before_s3 = Stage::BeforeS3.expected(&short);
        assert_eq!(initial.0, before_s3.0);

        fs::write(logs.join("nosmi-short-1-s3.jsonl"), "").unwrap();
        assert!(check_trial_logs(&logs, "nosmi-short-2", initial.1).is_ok());
        assert!(check_trial_logs(&logs, "nosmi-short-2", before_s3.1).is_err());
        fs::write(logs.join("nosmi-short-2-before.snapshot.txt"), "").unwrap();
        assert!(check_trial_logs(&logs, "nosmi-short-2", initial.1).is_err());
        assert!(check_trial_logs(&logs, "nosmi-short-2", before_s3.1).is_ok());
        fs::write(logs.join("nosmi-short-2-s3.jsonl"), "").unwrap();
        assert!(check_trial_logs(&logs, "nosmi-short-2", before_s3.1).is_err());

        let probe = Arm::AgedBypassProbe;
        fs::write(logs.join("nosmi-aged-probe-1-before.snapshot.txt"), "").unwrap();
        let (successes, expected) = Stage::BeforeS3.expected(&probe);
        assert_eq!(successes, "0");
        assert!(check_trial_logs(&logs, "nosmi-aged-probe-1", expected).is_ok());

        let (successes, expected) = Stage::BeforeS3.expected(&aged);
        assert_eq!(successes, "5");
        fs::write(logs.join("aged-6-before.snapshot.txt"), "").unwrap();
        assert!(check_trial_logs(&logs, "aged-6", expected).is_err());
        fs::write(logs.join("aged-6-dry.jsonl"), "").unwrap();
        assert!(check_trial_logs(&logs, "aged-6", expected).is_ok());
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
}
