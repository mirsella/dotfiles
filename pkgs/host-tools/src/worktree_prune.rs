//! Remove OpenCode worktrees whose work already reached the remote default branch.
//!
//! OpenCode never deletes worktrees it creates, and T3 only cleans its own. A
//! worktree goes only when it is clean, holds no nested worktree, its HEAD is
//! an ancestor of origin's default branch, nothing ran in it for `max_age`, and
//! no process or recent OpenCode session uses it. Removal uses
//! `git worktree remove` without force, so Git re-checks cleanliness itself.
use crate::{scratch::last_touched, util};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn git(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = util::command_output(Command::new("git").arg("-C").arg(cwd).args(args))?;
    Ok(String::from_utf8(output)?.trim_end().to_owned())
}

/// Exit status 0 or 1 from a Git predicate; anything else is an error.
fn git_test(cwd: &Path, args: &[&str]) -> Result<bool> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .context("start git")?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => anyhow::bail!(
            "git {} exited {}: {}",
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ),
    }
}

/// origin's default branch, preferring origin/HEAD over the usual names.
fn default_ref(worktree: &Path) -> Result<Option<String>> {
    if let Ok(target) = git(
        worktree,
        &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"],
    ) {
        return Ok(Some(target));
    }
    for candidate in ["refs/remotes/origin/main", "refs/remotes/origin/master"] {
        if git_test(worktree, &["rev-parse", "--verify", "--quiet", candidate])? {
            return Ok(Some(candidate.to_owned()));
        }
    }
    Ok(None)
}

/// Why a worktree must stay, or None when every check allows removal.
fn keep_reason(
    worktree: &Path,
    now: SystemTime,
    max_age: Duration,
    in_use: &[PathBuf],
) -> Result<Option<String>> {
    if in_use
        .iter()
        .any(|directory| directory.starts_with(worktree))
    {
        return Ok(Some("in use by a process or recent session".into()));
    }
    if !fs::symlink_metadata(worktree.join(".git"))?.is_file() {
        return Ok(Some("not a linked worktree".into()));
    }
    let status = git(
        worktree,
        &[
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ],
    )?;
    if !status.is_empty() {
        return Ok(Some(format!(
            "has uncommitted changes ({} files)",
            status.lines().count()
        )));
    }
    // `git worktree remove` deletes ignored files, including nested worktrees.
    let listed = git(worktree, &["worktree", "list", "--porcelain"])?;
    if listed
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(Path::new)
        .any(|path| path != worktree && path.starts_with(worktree))
    {
        return Ok(Some("contains a nested worktree".into()));
    }
    let Some(default) = default_ref(worktree)? else {
        return Ok(Some("origin has no default branch".into()));
    };
    if !git_test(worktree, &["merge-base", "--is-ancestor", "HEAD", &default])? {
        return Ok(Some(format!("HEAD is not in {default}")));
    }
    // Checked last: it walks the whole tree, build outputs included.
    let idle = now
        .duration_since(last_touched(worktree)?)
        .unwrap_or_default();
    if idle <= max_age {
        return Ok(Some(format!("touched {}d ago", idle.as_secs() / 86_400)));
    }
    Ok(None)
}

fn remove(worktree: &Path) -> Result<()> {
    // Run from the shared Git directory: the worktree cannot remove itself.
    let common = git(
        worktree,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    util::command_output(
        Command::new("git")
            .arg("-C")
            .arg(&common)
            .args(["worktree", "remove"])
            .arg(worktree),
    )?;
    Ok(())
}

/// Remove landed worktrees under `root/<project>/<name>`; return how many went away.
pub fn prune(
    root: &Path,
    now: SystemTime,
    max_age: Duration,
    in_use: &[PathBuf],
    dry_run: bool,
) -> Result<usize> {
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
    for project in fs::read_dir(root).with_context(|| format!("read {}", root.display()))? {
        let project = project?;
        if !project.file_type()?.is_dir() {
            continue;
        }
        for entry in fs::read_dir(project.path())? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let worktree = entry.path();
            match keep_reason(&worktree, now, max_age, in_use) {
                Ok(Some(reason)) => {
                    println!("Kept worktree {}: {reason}", worktree.display());
                    continue;
                }
                Ok(None) => {}
                Err(error) => {
                    eprintln!("worktree: keep {}: {error:#}", worktree.display());
                    continue;
                }
            }
            if dry_run {
                println!("Would remove worktree {}", worktree.display());
            } else if let Err(error) = remove(&worktree) {
                failures.push(format!("remove {}: {error:#}", worktree.display()));
                continue;
            } else {
                println!("Removed worktree {}", worktree.display());
            }
            removed += 1;
        }
        if !dry_run && fs::read_dir(project.path())?.next().is_none() {
            fs::remove_dir(project.path())?;
        }
    }
    ensure!(failures.is_empty(), "{}", failures.join("; "));
    Ok(removed)
}

/// Directories of OpenCode sessions updated since `since`.
pub fn recent_session_dirs(database: &Path, since: SystemTime) -> Result<Vec<PathBuf>> {
    let since_ms = since.duration_since(UNIX_EPOCH)?.as_millis();
    let output = util::command_output(
        Command::new("sqlite3")
            .args(["-readonly", "-json", "-cmd", ".timeout 3000"])
            .arg(database)
            .arg(format!(
                "SELECT DISTINCT directory FROM session_v2 WHERE time_updated >= {since_ms}"
            )),
    )
    .with_context(|| format!("read OpenCode sessions from {}", database.display()))?;
    // sqlite3 emits no bytes for an empty result set.
    if output.is_empty() {
        return Ok(Vec::new());
    }
    #[derive(Deserialize)]
    struct Row {
        directory: PathBuf,
    }
    let rows: Vec<Row> = serde_json::from_slice(&output).context("invalid session result")?;
    Ok(rows.into_iter().map(|row| row.directory).collect())
}

/// Working directories of every process we can inspect.
pub fn process_cwds() -> Result<Vec<PathBuf>> {
    let mut cwds = Vec::new();
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        if !entry
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|b| b.is_ascii_digit())
        {
            continue;
        }
        // Processes exit or belong to other users; both are expected.
        if let Ok(cwd) = fs::read_link(entry.path().join("cwd")) {
            cwds.push(cwd);
        }
    }
    Ok(cwds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_git(cwd: &Path, args: &[&str]) -> Result<()> {
        util::command_output(
            Command::new("git")
                .arg("-C")
                .arg(cwd)
                .args([
                    "-c",
                    "user.name=test",
                    "-c",
                    "user.email=test@example.invalid",
                ])
                .args(args)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1"),
        )?;
        Ok(())
    }

    /// Age a whole tree, as an untouched worktree would be.
    fn age(path: &Path, time: SystemTime) -> Result<()> {
        if fs::symlink_metadata(path)?.is_dir() {
            for entry in fs::read_dir(path)? {
                age(&entry?.path(), time)?;
            }
        }
        fs::File::open(path)?.set_modified(time)?;
        Ok(())
    }

    #[test]
    fn prune_removes_only_landed_idle_worktrees() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let base = fs::canonicalize(temp.path())?;
        let remote = base.join("remote.git");
        let checkout = base.join("checkout");
        run_git(
            &base,
            &["init", "--quiet", "--bare", "-b", "main", "remote.git"],
        )?;
        run_git(
            &base,
            &["clone", "--quiet", remote.to_str().unwrap(), "checkout"],
        )?;
        run_git(
            &checkout,
            &["commit", "--quiet", "--allow-empty", "-m", "init"],
        )?;
        run_git(&checkout, &["push", "--quiet", "origin", "main"])?;
        run_git(&checkout, &["remote", "set-head", "origin", "main"])?;
        fs::write(checkout.join(".git/info/exclude"), ".scratch/\n")?;

        let root = base.join("worktrees");
        let project = root.join("abc123");
        fs::create_dir_all(&project)?;
        let add = |name: &str| -> Result<PathBuf> {
            let path = project.join(name);
            run_git(
                &checkout,
                &[
                    "worktree",
                    "add",
                    "--quiet",
                    "--detach",
                    path.to_str().unwrap(),
                    "main",
                ],
            )?;
            Ok(path)
        };
        let landed = add("landed")?;
        let dirty = add("dirty")?;
        fs::write(dirty.join("wip.txt"), "wip")?;
        let unlanded = add("unlanded")?;
        run_git(
            &unlanded,
            &["commit", "--quiet", "--allow-empty", "-m", "local"],
        )?;
        let busy = add("busy")?;
        let nesting = add("nesting")?;
        let nested = nesting.join(".scratch/worktrees/inner");
        run_git(
            &checkout,
            &[
                "worktree",
                "add",
                "--quiet",
                "--detach",
                nested.to_str().unwrap(),
                "main",
            ],
        )?;

        let now = SystemTime::now();
        let max_age = Duration::from_secs(8 * 86_400);
        age(&project, now - 2 * max_age)?;
        let recent = add("recent")?;

        let in_use = vec![busy.join("src")];
        assert_eq!(prune(&root, now, max_age, &in_use, true)?, 1);
        assert!(landed.exists());
        assert_eq!(prune(&root, now, max_age, &in_use, false)?, 1);
        assert!(!landed.exists());
        for kept in [&dirty, &unlanded, &busy, &nesting, &nested, &recent] {
            assert!(kept.exists(), "{} was removed", kept.display());
        }
        assert!(dirty.join("wip.txt").exists());
        Ok(())
    }

    #[test]
    fn recent_session_dirs_filters_by_update_time() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let database = temp.path().join("opencode.db");
        util::command_output(Command::new("sqlite3").arg(&database).arg(
            "CREATE TABLE session_v2 (directory TEXT, time_updated INTEGER);
             INSERT INTO session_v2 VALUES ('/old', 1000), ('/new', 5000), ('/new', 6000);",
        ))?;
        let since = UNIX_EPOCH + Duration::from_millis(2000);
        assert_eq!(
            recent_session_dirs(&database, since)?,
            vec![PathBuf::from("/new")]
        );
        assert!(recent_session_dirs(&database, UNIX_EPOCH + Duration::from_secs(10))?.is_empty());
        Ok(())
    }
}
