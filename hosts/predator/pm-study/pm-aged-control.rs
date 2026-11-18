// Guarded dry-run + S3 control. "check" runs the initial preflight only.
use std::env;
use std::fs;
use std::io;
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

struct Baseline<'a> {
    label: &'a str,
    boot: &'a str,
    system: &'a str,
    runner_hash: &'a str,
    storage: &'a Path,
}

impl Baseline<'_> {
    fn preflight(&self, successes: &str) -> Result<()> {
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
        require(awake >= 32_400.0, "less than nine hours awake")?;
        let clock: u32 = output("date", &["+%H%M"])?.trim().parse()?;
        require((30..2300).contains(&clock), "backup exclusion window")?;

        let logs = Path::new(RUNNER).with_file_name("logs");
        match logs.join("STOP").symlink_metadata() {
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
            Ok(_) => return Err("runner STOP flag exists".into()),
        }
        if successes == "0" {
            let prefix = format!("{}-", self.label);
            for entry in fs::read_dir(logs)? {
                require(
                    !entry?.file_name().to_string_lossy().starts_with(&prefix),
                    "trial labels already used",
                )?;
            }
        }
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
        println!("Aged control preflight passed, uptime {awake:.0} seconds, expected successes {successes}");
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
        self.preflight("0")?;
        let result = self
            .batch("before", &["snapshot"])
            .and_then(|()| self.batch("dry", &["dry", "5", "30", "10"]))
            .and_then(|()| self.preflight("5"))
            .and_then(|()| output("sync", &[]).map(|_| ()))
            .and_then(|()| self.batch("s3", &["s3", "1", "120", "30"]));
        let after = self.batch("after", &["snapshot"]);
        if let (Err(first), Err(second)) = (&result, &after) {
            eprintln!("Aged control: {first}; after snapshot also failed: {second}");
        }
        result.and(after)
    }
}

fn main() -> ExitCode {
    let args: Vec<_> = env::args().collect();
    let [_, mode, label, boot, system, runner_hash, storage] = args.as_slice() else {
        eprintln!("Usage: pm-aged-control <check|run> <aged-N> <boot-id> <system> <runner-sha256> <storage-records>");
        return ExitCode::FAILURE;
    };
    if !label
        .strip_prefix("aged-")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    {
        eprintln!("Invalid aged control label: {label}");
        return ExitCode::FAILURE;
    }
    let baseline = Baseline {
        label,
        boot,
        system,
        runner_hash,
        storage: Path::new(storage),
    };
    let result = match mode.as_str() {
        "check" => baseline.preflight("0"),
        "run" => baseline.run(),
        _ => Err(format!("unknown mode: {mode}").into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Aged control stopped: {err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mount_rows;

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
