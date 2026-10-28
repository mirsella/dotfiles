//! Prune task scratch directories that agent sessions leave under ~/dev/.scratch.
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

pub const DEFAULT_MAX_AGE_DAYS: u64 = 3;

/// Newest modification time anywhere below a scratch entry, without following symlinks.
fn last_touched(path: &Path) -> Result<SystemTime> {
    let mut newest = fs::symlink_metadata(path)?.modified()?;
    let mut pending = vec![path.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                eprintln!("scratch: skip {}: {error}", directory.display());
                continue;
            }
        };
        for entry in entries {
            let entry = entry?;
            let metadata = entry.metadata()?;
            let modified = metadata.modified()?;
            if modified > newest {
                newest = modified;
            }
            if metadata.is_dir() {
                pending.push(entry.path());
            }
        }
    }
    Ok(newest)
}

/// Remove stale direct children of a scratch root and return how many went away.
pub fn prune(root: &Path, now: SystemTime, max_age: Duration, dry_run: bool) -> Result<usize> {
    let root_metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        root_metadata.is_dir(),
        "{} is not a directory",
        root.display()
    );
    let mut removed = 0;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.metadata()?.is_dir() {
            continue;
        }
        let path = entry.path();
        let age = now.duration_since(last_touched(&path)?).unwrap_or_default();
        if age <= max_age {
            continue;
        }
        if dry_run {
            println!("Would prune {}", path.display());
        } else {
            fs::remove_dir_all(&path).with_context(|| format!("prune {}", path.display()))?;
            println!("Pruned {}", path.display());
        }
        removed += 1;
    }
    Ok(removed)
}

pub fn run(root: Option<PathBuf>, max_age_days: u64, dry_run: bool) -> Result<()> {
    let home = fs::canonicalize(PathBuf::from(
        std::env::var_os("HOME").context("HOME is unset")?,
    ))?;
    let root = match root {
        Some(root) => root,
        None => home.join("dev/.scratch"),
    };
    if let Ok(metadata) = fs::symlink_metadata(&root) {
        ensure!(metadata.is_dir(), "{} is not a directory", root.display());
        let canonical = fs::canonicalize(&root)?;
        ensure!(
            canonical.starts_with(&home) && canonical != home,
            "refusing to prune {} outside a home subdirectory",
            canonical.display()
        );
    }
    let max_age = Duration::from_secs(max_age_days * 86_400);
    let removed = prune(&root, SystemTime::now(), max_age, dry_run)?;
    println!(
        "Scratch: {removed} entries untouched for {max_age_days}d under {}",
        root.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_modified(path: &Path, time: SystemTime) -> Result<()> {
        fs::File::open(path)?.set_modified(time)?;
        Ok(())
    }

    #[test]
    fn prune_removes_only_stale_directories() -> Result<()> {
        let root = tempfile::tempdir()?;
        let now = SystemTime::now();
        let stale = Duration::from_secs(86_400);

        let expired = root.path().join("expired");
        fs::create_dir_all(expired.join("deep"))?;
        fs::write(expired.join("deep/artifact"), b"x")?;
        set_modified(&expired.join("deep/artifact"), now - 10 * stale)?;
        set_modified(&expired.join("deep"), now - 10 * stale)?;
        set_modified(&expired, now - 10 * stale)?;

        let active = root.path().join("active");
        fs::create_dir_all(active.join("deep"))?;
        fs::write(active.join("deep/artifact"), b"x")?;
        set_modified(&active.join("deep/artifact"), now - 10 * stale)?;
        set_modified(&active.join("deep"), now - 10 * stale)?;
        set_modified(&active, now - 10 * stale)?;
        set_modified(&active.join("deep/artifact"), now)?;

        let recent = root.path().join("recent");
        fs::create_dir(&recent)?;

        let outside = tempfile::tempdir()?;
        fs::write(outside.path().join("keep"), b"x")?;
        std::os::unix::fs::symlink(outside.path(), root.path().join("linked"))?;

        let file = root.path().join("notes.txt");
        fs::write(&file, b"x")?;

        assert_eq!(prune(root.path(), now, 2 * stale, true)?, 1);
        assert!(expired.exists());
        assert_eq!(prune(root.path(), now, 2 * stale, false)?, 1);
        assert!(!expired.exists());
        for name in ["active", "recent", "linked", "notes.txt"] {
            assert!(root.path().join(name).exists(), "{name} was pruned");
        }
        assert!(outside.path().join("keep").exists());
        Ok(())
    }
}
