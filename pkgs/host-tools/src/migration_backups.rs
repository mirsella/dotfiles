//! Only explicitly enrolled, complete migration bundles participate in retention.
use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MARKER: &str = ".retention-managed.json";
const KEEP: usize = 2;
const MAX_AGE: Duration = Duration::from_secs(30 * 86400);

#[derive(Subcommand)]
pub enum Operation {
    /// Enroll a completed, direct child of ~/dev/opencode-v1-backups.
    Enroll { bundle: PathBuf },
    /// Keep the newest two enrolled bundles and every bundle younger than 30 days.
    Prune {
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    version: u32,
    completed_at: u64,
}

fn now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

fn enroll(root: &Path, bundle: &Path, completed_at: u64) -> Result<()> {
    ensure!(
        fs::symlink_metadata(bundle)?.is_dir(),
        "Bundle must be a real directory"
    );
    let bundle = fs::canonicalize(bundle)?;
    ensure!(
        bundle.parent() == Some(fs::canonicalize(root)?.as_path()),
        "Bundle must be a direct child of the migration backup root"
    );
    let marker = bundle.join(MARKER);
    if marker.try_exists()? {
        return Ok(());
    }
    let marker_value = Marker {
        version: 1,
        completed_at,
    };
    crate::util::atomic_write(&marker, &serde_json::to_vec(&marker_value)?)
}

fn prune(root: &Path, current_time: u64, dry_run: bool) -> Result<usize> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        fs::symlink_metadata(root)?.is_dir(),
        "Backup root must be a real directory"
    );
    let mut bundles = Vec::new();
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let marker = entry.path().join(MARKER);
        let metadata = match fs::symlink_metadata(&marker) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        ensure!(metadata.len() < 4096, "Invalid retention marker size");
        let value: Marker =
            serde_json::from_slice(&fs::read(&marker)?).context("Invalid retention marker")?;
        ensure!(value.version == 1, "Unknown retention marker version");
        bundles.push((value.completed_at, entry.path()));
    }
    bundles.sort_by(|a, b| b.cmp(a));
    let mut removed = 0;
    for (completed_at, path) in bundles.into_iter().skip(KEEP) {
        if current_time.saturating_sub(completed_at) < MAX_AGE.as_secs() {
            continue;
        }
        if dry_run {
            println!("Would prune {}", path.display());
        } else {
            fs::remove_dir_all(&path).with_context(|| format!("Prune {}", path.display()))?;
            println!("Pruned {}", path.display());
        }
        removed += 1;
    }
    Ok(removed)
}

pub fn run(operation: Operation) -> Result<()> {
    let root = PathBuf::from(std::env::var_os("HOME").context("HOME is unset")?)
        .join("dev/opencode-v1-backups");
    match operation {
        Operation::Enroll { bundle } => enroll(&root, &bundle, now()?),
        Operation::Prune { dry_run } => {
            let removed = prune(&root, now()?, dry_run)?;
            println!("Retention: newest {KEEP} bundles plus 30 days; {removed} expired bundles");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retention_preserves_newest_recent_unenrolled_and_symlinked_bundles() -> Result<()> {
        let root = tempfile::tempdir()?;
        let current_time = 100 * 86400;
        for (name, day) in [("expired", 1), ("newer", 2), ("recent", 99)] {
            let bundle = root.path().join(name);
            fs::create_dir(&bundle)?;
            fs::write(bundle.join("state.sqlite"), b"backup")?;
            enroll(root.path(), &bundle, day * 86400)?;
        }
        let unknown = root.path().join("unmanaged");
        fs::create_dir(&unknown)?;
        let outside = tempfile::tempdir()?;
        let link = root.path().join("linked");
        std::os::unix::fs::symlink(outside.path(), &link)?;
        assert!(enroll(root.path(), &link, 1).is_err());
        assert_eq!(prune(root.path(), current_time, true)?, 1);
        assert!(root.path().join("expired").exists());
        assert_eq!(prune(root.path(), current_time, false)?, 1);
        assert!(!root.path().join("expired").exists());
        for name in ["newer", "recent", "unmanaged", "linked"] {
            assert!(root.path().join(name).exists());
        }
        assert!(outside.path().exists());
        Ok(())
    }
}
