//! Prune task scratch directories left under ~/dev/scratch and ~/.t3/scratch.
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

pub const DEFAULT_MAX_AGE_DAYS: u64 = 8;

/// Newest modification time anywhere below a scratch entry, without following symlinks.
fn last_touched(path: &Path) -> Result<SystemTime> {
    let mut newest = fs::symlink_metadata(path)?.modified()?;
    let mut pending = vec![path.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in
            fs::read_dir(&directory).with_context(|| format!("read {}", directory.display()))?
        {
            let entry = entry?;
            if entry.file_type()?.is_symlink() {
                continue;
            }
            let metadata = entry.metadata()?;
            newest = newest.max(metadata.modified()?);
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
        "{} is not a real directory",
        root.display()
    );
    let mut removed = 0;
    let mut failures = Vec::new();
    for entry in fs::read_dir(root).with_context(|| format!("read {}", root.display()))? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path();
        let age = match last_touched(&path) {
            Ok(touched) => now.duration_since(touched).unwrap_or_default(),
            Err(error) => {
                eprintln!("scratch: keep {}: {error:#}", path.display());
                continue;
            }
        };
        if age <= max_age {
            continue;
        }
        if dry_run {
            println!("Would prune {}", path.display());
        } else {
            match fs::remove_dir_all(&path) {
                Ok(()) => println!("Pruned {}", path.display()),
                Err(error) => {
                    failures.push(format!("prune {}: {error}", path.display()));
                    continue;
                }
            }
        }
        removed += 1;
    }
    ensure!(failures.is_empty(), "{}", failures.join("; "));
    Ok(removed)
}

pub fn run(max_age_days: u64, dry_run: bool) -> Result<()> {
    let home = fs::canonicalize(PathBuf::from(
        std::env::var_os("HOME").context("HOME is unset")?,
    ))?;
    let max_age = Duration::from_secs(max_age_days.saturating_mul(86_400));
    let mut failures = Vec::new();
    for relative in ["dev/scratch", ".t3/scratch"] {
        let root = home.join(relative);
        match prune(&root, SystemTime::now(), max_age, dry_run) {
            Ok(removed) => println!(
                "Scratch: {removed} entries untouched for {max_age_days}d under {}",
                root.display()
            ),
            Err(error) => failures.push(format!("{error:#}")),
        }
    }
    ensure!(failures.is_empty(), "{}", failures.join("; "));
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

        let outside = tempfile::tempdir()?;
        fs::write(outside.path().join("keep"), b"x")?;

        let expired = root.path().join("expired");
        fs::create_dir_all(expired.join("deep"))?;
        fs::write(expired.join("deep/artifact"), b"x")?;
        std::os::unix::fs::symlink(outside.path(), expired.join("link"))?;
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

        std::os::unix::fs::symlink("missing", root.path().join("broken"))?;

        let file = root.path().join("notes.txt");
        fs::write(&file, b"x")?;

        assert_eq!(prune(root.path(), now, 2 * stale, true)?, 1);
        assert!(expired.exists());
        assert_eq!(prune(root.path(), now, 2 * stale, false)?, 1);
        assert!(!expired.exists());
        for name in ["active", "recent", "broken", "notes.txt"] {
            assert!(
                fs::symlink_metadata(root.path().join(name)).is_ok(),
                "{name} was pruned"
            );
        }
        assert!(outside.path().join("keep").exists());
        Ok(())
    }

    #[test]
    fn prune_rejects_a_symlinked_root() -> Result<()> {
        let root = tempfile::tempdir()?;
        let target = root.path().join("target");
        let stale = target.join("stale");
        fs::create_dir_all(&stale)?;
        let old = SystemTime::now() - Duration::from_secs(10 * 86_400);
        set_modified(&stale, old)?;
        set_modified(&target, old)?;
        let link = root.path().join("link");
        std::os::unix::fs::symlink(&target, &link)?;

        assert!(prune(&link, SystemTime::now(), Duration::from_secs(86_400), false).is_err());
        assert!(stale.exists());
        Ok(())
    }
}
