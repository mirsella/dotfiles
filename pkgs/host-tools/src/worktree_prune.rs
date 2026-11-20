//! Shared safety checks for agent cleanup, plus OpenCode's and ~/dev's worktrees.
//!
//! OpenCode's git strategy creates detached worktrees under
//! ~/.local/share/opencode/worktree/<project id prefix>/<name> and never removes
//! them. Its saved inventory (the `worktree` table behind `GET /api/worktree`)
//! records every linked worktree it discovers, T3's included, so location is
//! the only ownership signal; its next refresh drops entries for removed folders.
//!
//! Linked worktrees under ~/dev belong to no tool. They go after a longer idle
//! period, and `git worktree lock` keeps one indefinitely.
//!
//! A candidate goes only when nothing committed would be lost and nothing uses
//! it: no uncommitted changes, every commit kept by a branch, tag or
//! remote-tracking branch that outlives the removal, no initialized submodule,
//! not locked, no recent change, and no process, mount or recent session
//! inside. A recent change is a directory changed by a created, renamed or
//! deleted file, Git working in it, or a changed file Git would lose. Nested checkouts are held to the same rule; a nested
//! clone's own branches go with it, so its commits must be on a remote. Ignored
//! files such as build outputs are regenerable and go too. The first pass
//! checks every candidate in parallel; a removal checks again against fresh
//! state.
use crate::salvage::{self, Save, Saved, Store, Watched};
use crate::{tree_scan, util};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fmt, fs,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

/// A reason to keep a candidate. Any other error is a failure that fails the run.
///
/// A blocker is something that will not resolve on its own, such as unsaved or
/// unpushed work, found once the candidate is idle past its retention and
/// nothing uses it. Everything else is waiting: recent activity, an active
/// owner, a process inside or a deliberate lock.
#[derive(Debug)]
pub(crate) struct Keep {
    reason: String,
    blocker: bool,
}

impl fmt::Display for Keep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for Keep {}

pub(crate) fn keep(reason: impl Into<String>) -> anyhow::Error {
    Keep {
        reason: reason.into(),
        blocker: false,
    }
    .into()
}

pub(crate) fn blocked(reason: impl Into<String>) -> anyhow::Error {
    Keep {
        reason: reason.into(),
        blocker: true,
    }
    .into()
}

macro_rules! keep_unless {
    ($condition:expr, $($reason:tt)+) => {
        if !$condition {
            return Err($crate::worktree_prune::keep(format!($($reason)+)));
        }
    };
}
pub(crate) use keep_unless;

macro_rules! block_unless {
    ($condition:expr, $($reason:tt)+) => {
        if !$condition {
            return Err($crate::worktree_prune::blocked(format!($($reason)+)));
        }
    };
}

/// Why a candidate stays.
#[derive(Debug, PartialEq)]
pub(crate) struct Kept {
    pub(crate) reason: String,
    pub(crate) blocker: bool,
}

/// What both passes agreed on, or why to keep the candidate.
pub(crate) type Decision<T> = Result<std::result::Result<T, Kept>>;

/// Allow removal only when a fresh second check agrees with the first.
pub(crate) fn agree<T: PartialEq>(first: T, second: Result<T>) -> Decision<T> {
    decide_once(second.and_then(|second| {
        keep_unless!(second == first, "changed during inspection");
        Ok(second)
    }))
}

/// Turn one check's result into a decision: keep reasons stay, failures fail.
pub(crate) fn decide_once<T>(result: Result<T>) -> Decision<T> {
    match result {
        Ok(value) => Ok(Ok(value)),
        Err(error) => match error.downcast_ref::<Keep>().map(|keep| keep.blocker) {
            Some(blocker) => Ok(Err(Kept {
                reason: format!("{error:#}"),
                blocker,
            })),
            None => Err(error),
        },
    }
}

/// An idle candidate that needs the user's attention before it can go.
#[derive(Debug, Clone, PartialEq)]
pub struct Blocked {
    pub path: PathBuf,
    pub reason: String,
}

/// Outcomes of one cleanup category; failures fail the whole run.
#[derive(Default)]
pub(crate) struct Report {
    removed: usize,
    kept: usize,
    blocked: Vec<Blocked>,
    failures: Vec<String>,
}

impl Report {
    pub(crate) fn settle<T>(
        &mut self,
        label: &str,
        path: &Path,
        dry_run: bool,
        decision: Decision<T>,
        remove: impl FnOnce(&Path, T) -> Result<()>,
    ) {
        let path_text = path.display();
        match decision {
            Ok(Err(Kept { reason, blocker })) => {
                self.kept += 1;
                if blocker {
                    println!("Kept {label} {path_text}: {reason} [needs attention]");
                    self.blocked.push(Blocked {
                        path: path.to_owned(),
                        reason,
                    });
                } else {
                    println!("Kept {label} {path_text}: {reason}");
                }
            }
            Ok(Ok(_)) if dry_run => {
                self.removed += 1;
                println!("Would remove {label} {path_text}");
            }
            Ok(Ok(checked)) => match remove(path, checked) {
                Ok(()) => {
                    self.removed += 1;
                    println!("Removed {label} {path_text}");
                }
                Err(error) => self
                    .failures
                    .push(format!("remove {label} {path_text}: {error:#}")),
            },
            Err(error) => self
                .failures
                .push(format!("{label} {path_text}: {error:#}")),
        }
    }

    /// Print the totals, hand blockers to `blocked`, and fail on any failure.
    pub(crate) fn finish(
        self,
        label: &str,
        dry_run: bool,
        blocked: &mut Vec<Blocked>,
    ) -> Result<()> {
        println!(
            "{label}: {} {}, kept {} ({} need attention), failed {}",
            if dry_run { "would remove" } else { "removed" },
            self.removed,
            self.kept,
            self.blocked.len(),
            self.failures.len()
        );
        blocked.extend(self.blocked);
        ensure!(self.failures.is_empty(), "{}", self.failures.join("; "));
        Ok(())
    }
}

/// Real directories `depth` levels below a real, canonical root, with the
/// root's device; None when the root does not exist.
pub(crate) fn candidates(root: &Path, depth: usize) -> Result<Option<(u64, Vec<PathBuf>)>> {
    let metadata = match root.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("inspect {}", root.display())),
    };
    ensure!(
        metadata.is_dir() && root.canonicalize()? == root,
        "{} is not a real directory",
        root.display()
    );
    let mut level = vec![root.to_path_buf()];
    for _ in 0..depth {
        let mut next = Vec::new();
        for directory in level {
            for entry in
                fs::read_dir(&directory).with_context(|| format!("read {}", directory.display()))?
            {
                let entry = entry?;
                // DirEntry::file_type never follows a symlink.
                if entry.file_type()?.is_dir() {
                    next.push(entry.path());
                }
            }
        }
        level = next;
    }
    level.sort();
    Ok(Some((metadata.dev(), level)))
}

/// A Git checkout found inside a candidate.
#[derive(Debug, PartialEq)]
pub(crate) struct Nested {
    path: PathBuf,
    head: Option<String>,
    /// The repository that keeps its branches, or None when that repository
    /// goes with the candidate.
    survivor: Option<PathBuf>,
}

/// What must not change between the two passes over a candidate, and what
/// removing it takes.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Fingerprint {
    device: u64,
    inode: u64,
    modified: Option<SystemTime>,
    head: Option<String>,
    nested: Vec<Nested>,
    /// Work to save before removal.
    saves: Vec<Save>,
    /// Git discards local changes: saved first, or already on the default branch.
    force: bool,
    /// Some files are another user's, so removal goes through sudo.
    sealed: bool,
}

impl Fingerprint {
    /// What removing it saves first, for the status listing.
    pub(crate) fn plan(&self) -> Option<String> {
        let branches = self
            .saves
            .iter()
            .filter(|save| matches!(save, Save::Wip { .. }))
            .count();
        let bundles = self.saves.len() - branches;
        let mut parts = Vec::new();
        if branches > 0 {
            parts.push(format!(
                "{branches} WIP branch{}",
                if branches == 1 { "" } else { "es" }
            ));
        }
        if bundles > 0 {
            parts.push(format!(
                "{bundles} bundle{}",
                if bundles == 1 { "" } else { "s" }
            ));
        }
        if parts.is_empty() {
            self.force
                .then(|| "its uncommitted edits already match the default branch".into())
        } else {
            Some(format!("saves {} first", parts.join(" and ")))
        }
    }
}

fn mount_path(field: &[u8]) -> Result<PathBuf> {
    let mut path = Vec::new();
    let mut bytes = field.iter().copied();
    while let Some(byte) = bytes.next() {
        if byte == b'\\' {
            let digits = bytes.by_ref().take(3).collect::<Vec<_>>();
            ensure!(
                digits.len() == 3 && digits.iter().all(|digit| (b'0'..=b'7').contains(digit)),
                "invalid mountinfo path escape"
            );
            let byte = digits
                .into_iter()
                .fold(0u16, |n, d| n * 8 + u16::from(d - b'0'));
            path.push(u8::try_from(byte)?);
        } else {
            path.push(byte);
        }
    }
    Ok(PathBuf::from(OsStr::from_bytes(&path)))
}

fn check_in_use(folder: &Path) -> Result<()> {
    let paths = tree_scan::processes(process_paths)?;
    keep_unless!(
        !paths.iter().any(|path| path.starts_with(folder)),
        "a process uses it"
    );
    let mountinfo = fs::read("/proc/self/mountinfo").context("read local mounts")?;
    for line in mountinfo
        .split(|&b| b == b'\n')
        .filter(|line| !line.is_empty())
    {
        let field = line
            .split(|&b| b == b' ')
            .nth(4)
            .context("invalid mountinfo")?;
        let mount = mount_path(field)?;
        keep_unless!(
            !mount.starts_with(folder) && folder.parent() != Some(&mount),
            "it or its parent is a mount point"
        );
    }
    Ok(())
}

use salvage::git;

fn git_bytes(cwd: &Path, args: &[&str]) -> Result<Vec<u8>> {
    util::command_output(salvage::command(cwd).args(args))
}

/// The paths Git would lose: tracked changes and untracked files.
fn changes(checkout: &Path) -> Result<Vec<PathBuf>> {
    let output = git_bytes(
        checkout,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ],
    )?;
    let mut paths = Vec::new();
    let mut records = output
        .split(|&byte| byte == 0)
        .filter(|record| !record.is_empty());
    while let Some(record) = records.next() {
        let Some(path) = record.get(3..) else {
            continue;
        };
        // A rename or copy names its source next.
        if record[..2].iter().any(|code| matches!(code, b'R' | b'C')) {
            records.next();
        }
        paths.push(PathBuf::from(OsStr::from_bytes(path)));
    }
    Ok(paths)
}

/// One of `paths` changed since `cutoff`: work in progress, not forgotten work.
fn recently_changed(checkout: &Path, paths: &[PathBuf], cutoff: SystemTime) -> Option<PathBuf> {
    paths.iter().map(|path| checkout.join(path)).find(|path| {
        path.symlink_metadata()
            .and_then(|info| info.modified())
            .is_ok_and(|time| time >= cutoff)
    })
}

/// The first branch, tag or remote-tracking branch that contains HEAD.
fn kept_by(checkout: &Path) -> Result<String> {
    // A checked-out branch keeps HEAD itself; only a detached HEAD needs
    // the history walk.
    let head = git(checkout, &["rev-parse", "--symbolic-full-name", "HEAD"])?;
    if head.starts_with("refs/heads/") {
        return Ok(head);
    }
    git(
        checkout,
        &[
            "for-each-ref",
            "--count=1",
            "--contains",
            "HEAD",
            "--format=%(refname)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
    )
}

fn common_dir(checkout: &Path) -> Result<PathBuf> {
    let common = PathBuf::from(git(
        checkout,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?);
    Ok(common.canonicalize().unwrap_or(common))
}

fn locked(checkout: &Path) -> Result<bool> {
    let admin = git(checkout, &["rev-parse", "--absolute-git-dir"])?;
    Ok(Path::new(&admin).join("locked").exists())
}

/// Hold a checkout nested in `candidate` to the same rule as the candidate,
/// and plan to save what it still holds. A nested checkout may be broken or
/// foreign, so Git errors keep it.
fn check_nested(
    candidate: &Path,
    checkout: &Path,
    cutoff: SystemTime,
) -> Result<(Nested, Vec<Save>)> {
    let name = checkout.display();
    let unreadable =
        |error: anyhow::Error| blocked(format!("cannot inspect nested {name}: {error:#}"));
    let passing = |error: anyhow::Error| {
        if error.downcast_ref::<Keep>().is_some() {
            error
        } else {
            unreadable(error)
        }
    };
    // Git falls back to an enclosing repository when this one is broken.
    let top = git(checkout, &["rev-parse", "--show-toplevel"]).map_err(unreadable)?;
    block_unless!(
        Path::new(&top).canonicalize().ok().as_deref() == Some(checkout),
        "nested {name} is not a working Git checkout"
    );
    // A lock is deliberate, so it keeps the parent without asking for attention.
    keep_unless!(
        !locked(checkout).map_err(unreadable)?,
        "nested {name} is locked"
    );
    let status = changes(checkout).map_err(unreadable)?;
    if let Some(path) = recently_changed(checkout, &status, cutoff) {
        return Err(keep(format!("{} changed recently", path.display())));
    }
    let head = git(checkout, &["rev-parse", "--verify", "--quiet", "HEAD"]).ok();
    let common = common_dir(checkout).map_err(unreadable)?;
    let mut saves = Vec::new();
    if !status.is_empty() {
        saves.extend(salvage::plan_dirty(checkout, &status, &common).map_err(passing)?);
    }
    if !common.starts_with(candidate) {
        // Its repository stays; only a HEAD on no ref would be lost.
        if saves.is_empty() && head.is_some() && kept_by(checkout).map_err(unreadable)?.is_empty() {
            saves.push(Save::Wip {
                checkout: checkout.to_owned(),
                repo: common.clone(),
                dirty: false,
            });
        }
        return Ok((
            Nested {
                path: checkout.to_owned(),
                head,
                survivor: Some(common),
            },
            saves,
        ));
    }
    // Its repository goes too, with its branches, tags and stash: bundle it
    // when any of that exists nowhere else.
    let mut unpushed = vec!["rev-list", "--count", "--branches", "--tags"];
    if head.is_some() {
        unpushed.push("HEAD");
    }
    unpushed.extend(["--not", "--remotes"]);
    let unpushed = git(checkout, &unpushed).map_err(unreadable)?;
    let stash = git(checkout, &["for-each-ref", "--count=1", "refs/stash"]).map_err(unreadable)?;
    let listed = git(checkout, &["worktree", "list", "--porcelain"]).map_err(unreadable)?;
    if let Some(outside) = listed
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(Path::new)
        .find(|path| !path.starts_with(candidate))
    {
        return Err(blocked(format!(
            "nested {name} has a worktree outside at {}",
            outside.display()
        )));
    }
    if !saves.is_empty() || unpushed != "0" || !stash.is_empty() {
        saves.push(Save::Bundle { repo: common });
    }
    Ok((
        Nested {
            path: checkout.to_owned(),
            head,
            survivor: None,
        },
        saves,
    ))
}

/// Walk a candidate without following symlinks: nothing modified since
/// `cutoff` and no mount or process inside, then one filesystem, everything
/// readable and every nested Git checkout safe to lose. A linked worktree has
/// its own `.git` file on top. The walk finishes before anything is reported
/// as a blocker, so only an idle candidate asks for attention.
pub(crate) fn inspect_tree(
    folder: &Path,
    device: u64,
    cutoff: SystemTime,
    linked: bool,
) -> Result<Fingerprint> {
    let metadata = folder.symlink_metadata()?;
    keep_unless!(
        metadata.is_dir() && metadata.dev() == device,
        "not a local directory"
    );
    keep_unless!(metadata.modified()? < cutoff, "changed recently");
    check_in_use(folder)?;
    let own_git = linked.then(|| folder.join(".git"));
    let scan = if tree_scan::remembering() {
        tree_scan::complete(folder, device, own_git.as_deref(), false)?
    } else {
        // Most candidates changed recently: stop at the first sign of it.
        std::sync::Arc::new(tree_scan::scan(
            folder,
            device,
            own_git.as_deref(),
            Some(cutoff),
            false,
        )?)
    };
    keep_unless!(!scan.vanished, "changed while it was being read");
    keep_unless!(
        scan.newest < cutoff,
        "{} changed recently",
        scan.newest_path.display()
    );
    if let Some(first) = scan.problems.first() {
        return Err(blocked(match scan.problems.len() {
            1 => first.clone(),
            count => format!("{first} (and {} more)", count - 1),
        }));
    }
    let sealed = !scan.unreadable.is_empty();
    if let Some(first) = scan.unreadable.first()
        && !salvage::root_available()
    {
        return Err(blocked(match scan.unreadable.len() {
            1 => format!("{} is not readable", first.display()),
            count => format!(
                "{} is not readable (and {} more)",
                first.display(),
                count - 1
            ),
        }));
    }
    let mut nested = Vec::new();
    let mut saves = Vec::new();
    for checkout in &scan.checkouts {
        let (found, save) = check_nested(folder, checkout, cutoff)?;
        nested.push(found);
        saves.extend(save);
    }
    saves.sort();
    saves.dedup();
    Ok(Fingerprint {
        device: metadata.dev(),
        inode: metadata.ino(),
        modified: Some(metadata.modified()?),
        head: None,
        nested,
        saves,
        force: false,
        sealed,
    })
}

/// Git's verdict on a linked worktree: its HEAD, and what removing it saves.
#[derive(Debug, Default)]
pub(crate) struct Verdict {
    head: String,
    saves: Vec<Save>,
    force: bool,
}

/// What a linked worktree's removal must save, and its HEAD.
/// A worktree Git cannot read is broken, not removable.
pub(crate) fn check_git(worktree: &Path, cutoff: SystemTime) -> Result<Verdict> {
    check_git_unchecked(worktree, cutoff).map_err(|error| {
        if error.downcast_ref::<Keep>().is_some() {
            error
        } else {
            blocked(format!("Git cannot inspect it: {error:#}"))
        }
    })
}

fn check_git_unchecked(worktree: &Path, cutoff: SystemTime) -> Result<Verdict> {
    keep_unless!(!locked(worktree)?, "locked with git worktree lock");
    let status = changes(worktree)?;
    if let Some(path) = recently_changed(worktree, &status, cutoff) {
        return Err(keep(format!("{} changed recently", path.display())));
    }
    if worktree.join(".gitmodules").exists() {
        let modules = git(worktree, &["submodule", "status", "--recursive"])?;
        block_unless!(
            modules.lines().all(|line| line.starts_with('-')),
            "has initialized submodules"
        );
    }
    let head = git(worktree, &["rev-parse", "--verify", "HEAD"])?;
    let repo = common_dir(worktree)?;
    let mut saves = Vec::new();
    if !status.is_empty() {
        saves.extend(salvage::plan_dirty(worktree, &status, &repo)?);
    }
    // Branches, tags and remote-tracking branches live in the shared
    // repository and keep these commits; a HEAD on none of them needs saving.
    if saves.is_empty() && kept_by(worktree)?.is_empty() {
        saves.push(Save::Wip {
            checkout: worktree.to_owned(),
            repo,
            dirty: false,
        });
    }
    Ok(Verdict {
        head,
        saves,
        force: !status.is_empty(),
    })
}

/// What a worktree's check learns without its owner's state, so it can run
/// while that state loads.
pub(crate) struct Precheck {
    recent: bool,
    /// No `.git` file: a plain folder among the worktrees, checked like scratch.
    plain: bool,
    locked: bool,
    activity: Option<(SystemTime, PathBuf)>,
    /// Git's verdict, worked out only for a worktree idle by every other sign.
    git: Option<Result<Verdict>>,
}

pub(crate) fn precheck_worktree(worktree: &Path, cutoff: SystemTime) -> Result<Precheck> {
    let recent = worktree.symlink_metadata()?.modified()? >= cutoff;
    let plain = !worktree.join(".git").is_file();
    let locked = !plain && locked(worktree).unwrap_or(false);
    let activity = if plain { None } else { git_activity(worktree) };
    let idle = !recent && !locked && activity.as_ref().is_none_or(|(time, _)| *time < cutoff);
    let git = idle.then(|| {
        if plain {
            Ok(Verdict::default())
        } else {
            check_git(worktree, cutoff)
        }
    });
    Ok(Precheck {
        recent,
        plain,
        locked,
        activity,
        git,
    })
}

impl Precheck {
    /// Idle by every owner-free sign, and Git can account for everything.
    pub(crate) fn idle_and_clean(&self) -> bool {
        matches!(self.git, Some(Ok(_)))
    }
}

/// A linked worktree goes when nothing uses it, after saving what Git would
/// lose. Cheap signs come first and the tree walk last, only for a worktree
/// that is idle and unowned: it visits build outputs too. Waiting reasons win
/// over Git's verdict, so only an idle, unused worktree asks for attention.
pub(crate) fn check_worktree(
    worktree: &Path,
    device: u64,
    cutoff: SystemTime,
    owner: impl FnOnce() -> Result<()>,
    precheck: Option<Precheck>,
) -> Result<Fingerprint> {
    let precheck = match precheck {
        Some(precheck) => precheck,
        None => precheck_worktree(worktree, cutoff)?,
    };
    keep_unless!(!precheck.recent, "changed recently");
    owner()?;
    if precheck.plain {
        return inspect_tree(worktree, device, cutoff, false);
    }
    keep_unless!(!precheck.locked, "locked with git worktree lock");
    if let Some((time, path)) = &precheck.activity {
        keep_unless!(*time < cutoff, "{} changed recently", path.display());
    }
    let tree = inspect_tree(worktree, device, cutoff, true)?;
    let verdict = precheck
        .git
        .context("an idle worktree has Git's verdict")??;
    let mut saves = tree.saves;
    saves.extend(verdict.saves);
    saves.sort();
    saves.dedup();
    Ok(Fingerprint {
        head: Some(verdict.head),
        saves,
        force: verdict.force,
        ..tree
    })
}

/// When Git last worked in a linked worktree. Its HEAD, index and reflog
/// live in the repository's administrative folder, outside the tree walk.
pub(crate) fn git_activity(worktree: &Path) -> Option<(SystemTime, PathBuf)> {
    let text = fs::read_to_string(worktree.join(".git")).ok()?;
    let admin = worktree.join(text.trim().strip_prefix("gitdir:")?.trim());
    ["HEAD", "index", "logs/HEAD"]
        .into_iter()
        .filter_map(|name| {
            let path = admin.join(name);
            let time = path.symlink_metadata().ok()?.modified().ok()?;
            Some((time, path))
        })
        .max_by_key(|(time, _)| *time)
}

/// Drop the administrative entries of nested worktrees that went with a candidate.
fn prune_nested(checked: &Fingerprint) -> Result<()> {
    let survivors: BTreeSet<_> = checked
        .nested
        .iter()
        .filter_map(|nested| nested.survivor.as_ref())
        .collect();
    for common in survivors {
        util::command_output(
            Command::new("git")
                .arg("-C")
                .arg(common)
                .args(["worktree", "prune"]),
        )?;
    }
    Ok(())
}

/// What removing a workspace left behind on purpose: saved work, and the
/// branch to delete once merged.
#[derive(Debug, Default)]
pub(crate) struct Outcome {
    pub(crate) saved: Vec<Saved>,
    pub(crate) watched: Option<Watched>,
}

/// Remove a checked linked worktree after saving what it holds.
pub(crate) fn remove(worktree: &Path, checked: Fingerprint, store: &Store) -> Result<Outcome> {
    // Run from the shared Git directory: the worktree cannot remove itself.
    let common = common_dir(worktree)?;
    let saved = salvage::apply(worktree, &checked.saves, store)?;
    let watched = salvage::watch(worktree, &common);
    let mut command = Command::new("git");
    command.arg("-C").arg(&common).args(["worktree", "remove"]);
    if checked.force {
        command.arg("--force");
    }
    if let Err(error) = util::command_output(command.arg(worktree)) {
        // Another user's files: delete through sudo, then let Git forget it.
        ensure!(checked.sealed, "{error:#}");
        salvage::remove_tree(worktree)?;
        util::command_output(
            Command::new("git")
                .arg("-C")
                .arg(&common)
                .args(["worktree", "prune"]),
        )?;
    }
    prune_nested(&checked)?;
    Ok(Outcome { saved, watched })
}

/// Remove a checked plain folder after saving what it holds.
pub(crate) fn remove_folder(folder: &Path, checked: Fingerprint, store: &Store) -> Result<Outcome> {
    let saved = salvage::apply(folder, &checked.saves, store)?;
    salvage::remove_tree(folder)?;
    prune_nested(&checked)?;
    Ok(Outcome {
        saved,
        watched: None,
    })
}

/// Keep a folder an OpenCode session used since `since`; `sessions` holds
/// each session directory's latest update, from `session_activity`.
pub(crate) fn check_sessions(
    sessions: &[(PathBuf, SystemTime)],
    folder: &Path,
    since: SystemTime,
) -> Result<()> {
    keep_unless!(
        !sessions
            .iter()
            .any(|(directory, updated)| *updated >= since && directory.starts_with(folder)),
        "an OpenCode session used it recently"
    );
    Ok(())
}

/// Working directories and open files of every other process we can inspect.
/// Our own descriptors, such as a scan's open directories, do not count.
pub fn process_paths() -> Result<Vec<PathBuf>> {
    let own = std::process::id().to_string();
    let mut paths = Vec::new();
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let name = entry.file_name();
        if !name.as_encoded_bytes().iter().all(u8::is_ascii_digit) || name == own.as_str() {
            continue;
        }
        // Processes exit or belong to other users; both are expected.
        let process = entry.path();
        if let Ok(cwd) = fs::read_link(process.join("cwd")) {
            paths.push(cwd);
        }
        let Ok(descriptors) = fs::read_dir(process.join("fd")) else {
            continue;
        };
        for descriptor in descriptors.flatten() {
            if let Ok(target) = fs::read_link(descriptor.path())
                && target.is_absolute()
            {
                paths.push(target);
            }
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// Read running processes now, before checks run Git inside candidates.
pub(crate) fn preload() -> Result<()> {
    tree_scan::processes(process_paths).map(drop)
}

/// What a cleanup candidate is: it sets the retention and how removal works.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    TaskScratch,
    T3Scratch,
    T3Worktree,
    OpenCode,
    Dev,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::TaskScratch => "Task scratch",
            Self::T3Scratch => "T3 scratch",
            Self::T3Worktree => "T3 worktree",
            Self::OpenCode => "OpenCode worktree",
            Self::Dev => "~/dev worktree",
        }
    }

    pub fn plural(self) -> &'static str {
        match self {
            Self::TaskScratch => "Task scratch",
            Self::T3Scratch => "T3 scratch",
            Self::T3Worktree => "T3 worktrees",
            Self::OpenCode => "OpenCode worktrees",
            Self::Dev => "~/dev worktrees",
        }
    }

    pub(crate) fn linked(self) -> bool {
        matches!(self, Self::T3Worktree | Self::OpenCode | Self::Dev)
    }

    /// Whether checking it needs T3's state: T3 threads run in ~/dev too.
    pub(crate) fn needs_t3(self) -> bool {
        matches!(self, Self::T3Scratch | Self::T3Worktree | Self::Dev)
    }
}

/// A workspace the cleanup may remove.
#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub(crate) kind: Kind,
    pub(crate) path: PathBuf,
    pub(crate) device: u64,
}

pub(crate) fn remove_candidate(
    kind: Kind,
    path: &Path,
    checked: Fingerprint,
    store: &Store,
) -> Result<Outcome> {
    if kind.linked() && path.join(".git").is_file() {
        remove(path, checked, store)
    } else {
        remove_folder(path, checked, store)
    }
}

/// Folders `depth` levels below `root`, such as ~/dev/scratch's entries.
pub(crate) fn folder_candidates(kind: Kind, root: &Path, depth: usize) -> Result<Vec<Candidate>> {
    let Some((device, folders)) = candidates(root, depth)? else {
        return Ok(Vec::new());
    };
    Ok(folders
        .into_iter()
        .map(|path| Candidate { kind, path, device })
        .collect())
}

/// Drop OpenCode project folders its removed worktrees left empty.
pub(crate) fn tidy_opencode(root: &Path) -> Result<()> {
    if !root.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() && fs::read_dir(entry.path())?.next().is_none() {
            fs::remove_dir(entry.path())?;
        }
    }
    Ok(())
}

/// The first pass over every candidate: in parallel, sharing scans and the
/// process list. `held` names what the user holds back.
pub(crate) fn first_pass(
    candidates: &[Candidate],
    cutoff: &(dyn Fn(Kind) -> SystemTime + Sync),
    held: &(dyn Fn(&Path) -> Option<String> + Sync),
    check: &(dyn Fn(&Candidate, SystemTime) -> Result<Fingerprint> + Sync),
) -> Vec<Decision<Fingerprint>> {
    let _memory = tree_scan::remember();
    if let Err(error) = preload() {
        let reason = format!("{error:#}");
        return candidates
            .iter()
            .map(|_| Err(anyhow::anyhow!("read running processes: {reason}")))
            .collect();
    }
    tree_scan::parallel(candidates, |candidate| match held(&candidate.path) {
        Some(reason) => Ok(Err(Kept {
            reason,
            blocker: false,
        })),
        None => decide_once(check(candidate, cutoff(candidate.kind))),
    })
}

/// Check each candidate the first pass would remove again, against fresh
/// state, remove it when both agree, and record every outcome. Saved work
/// goes to `bundles` and notes `note(path)`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn confirm(
    candidates: &[Candidate],
    first: Vec<Decision<Fingerprint>>,
    cutoff: &dyn Fn(Kind) -> SystemTime,
    check: &dyn Fn(&Candidate, SystemTime) -> Result<Fingerprint>,
    dry_run: bool,
    reports: &mut BTreeMap<Kind, Report>,
    bundles: &Path,
    note: &dyn Fn(&Path) -> String,
    outcomes: &mut Vec<Outcome>,
) {
    for (candidate, decision) in candidates.iter().zip(first) {
        let decision = match decision {
            Ok(Ok(checked)) if !dry_run => agree(checked, check(candidate, cutoff(candidate.kind))),
            other => other,
        };
        let kind = candidate.kind;
        reports.entry(kind).or_default().settle(
            kind.label(),
            &candidate.path,
            dry_run,
            decision,
            |path, checked| {
                let store = Store {
                    bundles: bundles.to_owned(),
                    note: note(path),
                };
                outcomes.push(remove_candidate(kind, path, checked, &store)?);
                Ok(())
            },
        );
    }
}

/// Primary checkouts up to three levels below `dev`, skipping build outputs and scratch.
fn dev_repositories(dev: &Path) -> Result<Vec<PathBuf>> {
    let mut repositories = Vec::new();
    let mut level = vec![dev.to_path_buf()];
    for _ in 0..3 {
        let mut next = Vec::new();
        for directory in level {
            let entries = match fs::read_dir(&directory) {
                Ok(entries) => entries,
                // Another user's directory cannot hold our checkouts.
                Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => continue,
                Err(error) => {
                    return Err(error).with_context(|| format!("read {}", directory.display()));
                }
            };
            for entry in entries {
                let entry = entry?;
                let name = entry.file_name();
                // Hidden directories hold caches and tool state, not projects.
                if !entry.file_type()?.is_dir()
                    || name.as_encoded_bytes().starts_with(b".")
                    || matches!(name.to_str(), Some("node_modules" | "target" | "scratch"))
                {
                    continue;
                }
                let path = entry.path();
                match fs::symlink_metadata(path.join(".git")) {
                    Ok(info) if info.is_dir() => repositories.push(path),
                    // A linked worktree; its primary checkout lists it.
                    Ok(_) => {}
                    Err(_) => next.push(path),
                }
            }
        }
        level = next;
    }
    Ok(repositories)
}

/// Linked worktrees of `dev`'s repositories that live under `dev`, outside ~/dev/scratch.
fn dev_worktrees(dev: &Path) -> Result<Vec<PathBuf>> {
    let repositories = dev_repositories(dev)?;
    let listings = tree_scan::parallel(&repositories, |repository| {
        Ok::<_, anyhow::Error>((
            common_dir(repository)?,
            git(repository, &["worktree", "list", "--porcelain"])?,
        ))
    });
    let mut commons = BTreeSet::new();
    let mut worktrees = BTreeSet::new();
    for (repository, listing) in repositories.iter().zip(listings) {
        let listed = match listing {
            Ok((common, listed)) if commons.insert(common.clone()) => listed,
            Ok(_) => continue,
            // A broken checkout is not ours to clean up; its worktrees stay.
            Err(error) => {
                println!("Skipped repository {}: {error:#}", repository.display());
                continue;
            }
        };
        // The first record is the primary checkout itself.
        for record in listed.split("\n\n").skip(1) {
            let Some(path) = record
                .lines()
                .find_map(|line| line.strip_prefix("worktree "))
            else {
                continue;
            };
            let Ok(path) = Path::new(path).canonicalize() else {
                continue;
            };
            if path.starts_with(dev) && !path.starts_with(dev.join("scratch")) && path.is_dir() {
                worktrees.insert(path);
            }
        }
    }
    Ok(worktrees.into_iter().collect())
}

/// Linked worktrees under `dev`.
pub(crate) fn dev_candidates(dev: &Path) -> Result<Vec<Candidate>> {
    if !dev.is_dir() {
        return Ok(Vec::new());
    }
    dev_worktrees(dev)?
        .into_iter()
        .map(|path| {
            Ok(Candidate {
                kind: Kind::Dev,
                device: fs::symlink_metadata(&path)?.dev(),
                path,
            })
        })
        .collect()
}

/// Newest session update per OpenCode session directory.
pub(crate) fn session_activity(database: &Path) -> Result<Vec<(PathBuf, SystemTime)>> {
    if !database.exists() {
        return Ok(Vec::new());
    }
    let output = util::command_output(
        Command::new("sqlite3")
            .args(["-readonly", "-json", "-cmd", ".timeout 3000"])
            .arg(database)
            .arg(
                "SELECT directory, MAX(time_updated) AS updated FROM session_v2 GROUP BY directory",
            ),
    )
    .with_context(|| format!("read OpenCode sessions from {}", database.display()))?;
    if output.is_empty() {
        return Ok(Vec::new());
    }
    #[derive(Deserialize)]
    struct Row {
        directory: PathBuf,
        updated: u64,
    }
    let rows: Vec<Row> = serde_json::from_slice(&output).context("invalid session result")?;
    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.directory,
                UNIX_EPOCH + std::time::Duration::from_millis(row.updated),
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    type TestCheck<'a> = dyn Fn(&Candidate, SystemTime) -> Result<Fingerprint> + Sync + 'a;

    thread_local! {
        /// Where a test's removals put bundles, and what they saved.
        static BUNDLES: PathBuf = std::env::temp_dir().join(format!(
            "sweep-test-bundles-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        static OUTCOMES: std::cell::RefCell<Vec<Outcome>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    fn run(
        candidates: Vec<Candidate>,
        cutoff: SystemTime,
        check: &TestCheck<'_>,
        dry_run: bool,
        blocked: &mut Vec<Blocked>,
    ) -> Result<()> {
        let first = first_pass(&candidates, &|_| cutoff, &|_| None, check);
        let mut reports = BTreeMap::new();
        let bundles = BUNDLES.with(|dir| dir.clone());
        let mut outcomes = Vec::new();
        confirm(
            &candidates,
            first,
            &|_| cutoff,
            check,
            dry_run,
            &mut reports,
            &bundles,
            &|path| format!("test of {}", path.display()),
            &mut outcomes,
        );
        OUTCOMES.with(|all| all.borrow_mut().extend(outcomes));
        let mut failures = Vec::new();
        for (kind, report) in reports {
            if let Err(error) = report.finish(kind.plural(), dry_run, blocked) {
                failures.push(format!("{error:#}"));
            }
        }
        ensure!(failures.is_empty(), "{}", failures.join("; "));
        Ok(())
    }

    fn prune_folders(
        _label: &str,
        root: &Path,
        cutoff: SystemTime,
        dry_run: bool,
        blocked: &mut Vec<Blocked>,
    ) -> Result<()> {
        let candidates = folder_candidates(Kind::TaskScratch, root, 1)?;
        run(
            candidates,
            cutoff,
            &|candidate, cutoff| inspect_tree(&candidate.path, candidate.device, cutoff, false),
            dry_run,
            blocked,
        )
    }

    fn prune_opencode(
        root: &Path,
        cutoff: SystemTime,
        database: &Path,
        dry_run: bool,
        blocked: &mut Vec<Blocked>,
    ) -> Result<()> {
        let sessions = session_activity(database)?;
        let candidates = folder_candidates(Kind::OpenCode, root, 2)?;
        let result = run(
            candidates,
            cutoff,
            &|candidate, cutoff| {
                check_worktree(
                    &candidate.path,
                    candidate.device,
                    cutoff,
                    || check_sessions(&sessions, &candidate.path, cutoff),
                    None,
                )
            },
            dry_run,
            blocked,
        );
        if !dry_run {
            tidy_opencode(root)?;
        }
        result
    }

    fn prune_dev(
        dev: &Path,
        cutoff: SystemTime,
        owner: impl Fn(&Path) -> Result<()> + Sync,
        dry_run: bool,
        blocked: &mut Vec<Blocked>,
    ) -> Result<()> {
        let candidates = dev_candidates(dev)?;
        run(
            candidates,
            cutoff,
            &|candidate, cutoff| {
                check_worktree(
                    &candidate.path,
                    candidate.device,
                    cutoff,
                    || owner(&candidate.path),
                    None,
                )
            },
            dry_run,
            blocked,
        )
    }
    use std::time::Duration;

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
                    "-c",
                    "protocol.file.allow=always",
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

    /// A pushed repository: `remote.git` and its clone `checkout`, ignoring `.scratch/`.
    fn repository(base: &Path) -> Result<(PathBuf, PathBuf)> {
        let remote = base.join("remote.git");
        let checkout = base.join("checkout");
        run_git(
            base,
            &["init", "--quiet", "--bare", "-b", "main", "remote.git"],
        )?;
        run_git(
            base,
            &["clone", "--quiet", remote.to_str().unwrap(), "checkout"],
        )?;
        run_git(
            &checkout,
            &["commit", "--quiet", "--allow-empty", "-m", "init"],
        )?;
        run_git(&checkout, &["push", "--quiet", "origin", "main"])?;
        fs::write(checkout.join(".git/info/exclude"), ".scratch/\n")?;
        Ok((remote, checkout))
    }

    fn add_worktree(checkout: &Path, path: &Path, branch: Option<&str>) -> Result<()> {
        let path = path.to_str().unwrap();
        match branch {
            Some(branch) => run_git(
                checkout,
                &["worktree", "add", "--quiet", "-b", branch, path, "main"],
            ),
            None => run_git(
                checkout,
                &["worktree", "add", "--quiet", "--detach", path, "main"],
            ),
        }
    }

    #[test]
    fn prune_removes_only_safe_idle_worktrees() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let base = fs::canonicalize(temp.path())?;
        let (remote, checkout) = repository(&base)?;
        let root = base.join("worktrees");
        let project = root.join("abc123");
        fs::create_dir_all(&project)?;
        let add = |name: &str, branch: Option<&str>| -> Result<PathBuf> {
            let path = project.join(name);
            add_worktree(&checkout, &path, branch)?;
            Ok(path)
        };
        let landed = add("landed", None)?;
        // A local branch keeps unpushed commits after its worktree goes.
        let branched = add("branched", Some("feature"))?;
        run_git(
            &branched,
            &["commit", "--quiet", "--allow-empty", "-m", "local"],
        )?;
        let dirty = add("dirty", None)?;
        fs::write(dirty.join("wip.txt"), "wip")?;
        let detached = add("detached", None)?;
        run_git(
            &detached,
            &["commit", "--quiet", "--allow-empty", "-m", "orphan"],
        )?;
        let busy = add("busy", None)?;
        let locked = add("locked", None)?;
        run_git(&checkout, &["worktree", "lock", locked.to_str().unwrap()])?;
        // Committed nested worktrees and pushed clones go with their parent.
        let nesting = add("nesting", None)?;
        let nested = nesting.join(".scratch/worktrees/inner");
        add_worktree(&checkout, &nested, Some("inner"))?;
        run_git(
            &nested,
            &["commit", "--quiet", "--allow-empty", "-m", "nested"],
        )?;
        let pushed_clone = nesting.join(".scratch/clone");
        run_git(
            &base,
            &[
                "clone",
                "--quiet",
                remote.to_str().unwrap(),
                pushed_clone.to_str().unwrap(),
            ],
        )?;
        // Unsaved work, nested or not, is saved before its workspace goes.
        let nested_dirty = add("nested-dirty", None)?;
        let inner_dirty = nested_dirty.join(".scratch/worktrees/inner");
        add_worktree(&checkout, &inner_dirty, None)?;
        fs::write(inner_dirty.join("wip.txt"), "wip")?;
        let unpushed = add("unpushed", None)?;
        let clone = unpushed.join(".scratch/clone");
        run_git(
            &base,
            &[
                "clone",
                "--quiet",
                remote.to_str().unwrap(),
                clone.to_str().unwrap(),
            ],
        )?;
        run_git(
            &clone,
            &["commit", "--quiet", "--allow-empty", "-m", "local"],
        )?;
        let broken = add("broken", None)?;
        fs::create_dir_all(broken.join(".scratch/clone/.git"))?;

        let now = SystemTime::now();
        let max_age = Duration::from_secs(8 * 86_400);
        age(&project, now - 2 * max_age)?;
        // Git's administrative files for the worktrees count as activity too.
        age(&checkout.join(".git/worktrees"), now - 2 * max_age)?;
        let recent = add("recent", None)?;
        let mut process = Command::new("sleep").arg("30").current_dir(&busy).spawn()?;

        let database = base.join("missing.db");
        let mut blocked = Vec::new();
        let result = (|| -> Result<()> {
            prune_opencode(&root, now - max_age, &database, true, &mut Vec::new())?;
            assert!(landed.exists() && branched.exists() && nesting.exists());
            prune_opencode(&root, now - max_age, &database, false, &mut blocked)?;
            Ok(())
        })();
        process.kill()?;
        process.wait()?;
        result?;
        // Only what cannot be saved asks for attention.
        let needs: Vec<_> = blocked.iter().map(|entry| entry.path.clone()).collect();
        assert_eq!(needs, vec![broken.clone()], "{blocked:#?}");
        for gone in [
            &landed,
            &branched,
            &nesting,
            &dirty,
            &detached,
            &nested_dirty,
            &unpushed,
        ] {
            assert!(!gone.exists(), "{} was kept", gone.display());
        }
        for kept in [&busy, &locked, &broken, &recent] {
            assert!(kept.exists(), "{} was removed", kept.display());
        }
        run_git(&checkout, &["rev-parse", "--verify", "feature"])?;
        run_git(&checkout, &["rev-parse", "--verify", "inner"])?;
        // The nested worktree's entry is pruned, so its branch is free again.
        add_worktree(&checkout, &base.join("again"), None)?;
        run_git(&base.join("again"), &["switch", "--quiet", "inner"])?;
        // Uncommitted files and a detached commit became local WIP branches.
        let wip = git(
            &checkout,
            &[
                "for-each-ref",
                "--format=%(refname:short)",
                "refs/heads/sweep/wip/",
            ],
        )?;
        let wip: Vec<_> = wip.lines().collect();
        assert_eq!(wip.len(), 3, "{wip:?}");
        let saved_dirty = wip
            .iter()
            .find(|branch| branch.ends_with("-dirty"))
            .context("no WIP branch for the dirty worktree")?;
        assert_eq!(
            git(&checkout, &["show", &format!("{saved_dirty}:wip.txt")])?,
            "wip"
        );
        assert!(wip.iter().any(|branch| branch.ends_with("-detached")));
        assert!(
            wip.iter()
                .any(|branch| branch.ends_with("-nested-dirty-inner"))
        );
        // The unpushed clone left a bundle with its commit.
        let outcomes = OUTCOMES.with(|all| std::mem::take(&mut *all.borrow_mut()));
        let bundle = outcomes
            .iter()
            .flat_map(|outcome| &outcome.saved)
            .find_map(|saved| match &saved.place {
                salvage::Place::Bundle { file } => Some(file.clone()),
                salvage::Place::Branch { .. } => None,
            })
            .context("no bundle for the unpushed clone")?;
        let heads = util::output("git", &["bundle", "list-heads", bundle.to_str().unwrap()])?;
        assert!(heads.contains("refs/heads/main"), "{heads}");
        let restored = base.join("restored");
        run_git(
            &base,
            &[
                "clone",
                "--quiet",
                bundle.to_str().unwrap(),
                restored.to_str().unwrap(),
            ],
        )?;
        assert_eq!(git(&restored, &["log", "-1", "--format=%s"])?, "local");
        // Removing a worktree on a branch watches that branch for deletion.
        assert!(outcomes.iter().any(|outcome| {
            outcome
                .watched
                .as_ref()
                .is_some_and(|watched| watched.branch == "feature")
        }));
        let _ = fs::remove_dir_all(BUNDLES.with(|dir| dir.clone()));
        Ok(())
    }

    #[test]
    fn dev_worktrees_go_once_idle_and_committed() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let base = fs::canonicalize(temp.path())?;
        let dev = base.join("dev");
        fs::create_dir_all(dev.join("group"))?;
        let (_, checkout) = repository(&base)?;
        let primary = dev.join("group/project");
        fs::rename(&checkout, &primary)?;
        let feature = dev.join("project-feature");
        let locked = dev.join("project-locked");
        let elsewhere = base.join("elsewhere");
        let scratch = dev.join("scratch/task");
        for (path, branch) in [
            (&feature, "feature"),
            (&locked, "locked"),
            (&elsewhere, "elsewhere"),
            (&scratch, "scratch"),
        ] {
            fs::create_dir_all(path.parent().unwrap())?;
            add_worktree(&primary, path, Some(branch))?;
            run_git(path, &["commit", "--quiet", "--allow-empty", "-m", branch])?;
        }
        run_git(&primary, &["worktree", "lock", locked.to_str().unwrap()])?;
        let now = SystemTime::now();
        let max_age = Duration::from_secs(7 * 86_400);
        for path in [&base, &dev] {
            age(path, now - 2 * max_age)?;
        }
        let owners = Mutex::new(Vec::new());
        prune_dev(
            &dev,
            now - max_age,
            |worktree| {
                owners
                    .lock()
                    .expect("owners poisoned")
                    .push(worktree.to_owned());
                Ok(())
            },
            false,
            &mut Vec::new(),
        )?;
        assert!(!feature.exists() && primary.exists());
        run_git(&primary, &["rev-parse", "--verify", "feature"])?;
        for kept in [&locked, &elsewhere, &scratch] {
            assert!(kept.exists(), "{} was removed", kept.display());
        }
        let owners = owners.into_inner().expect("owners poisoned");
        assert!(owners.contains(&feature) && !owners.contains(&elsewhere));
        Ok(())
    }

    #[test]
    fn decide_separates_keep_reasons_from_failures() {
        fn waiting<T>(reason: &str) -> std::result::Result<T, Kept> {
            Err(Kept {
                reason: reason.to_owned(),
                blocker: false,
            })
        }
        assert_eq!(agree(1, Ok(1)).unwrap(), Ok(1));
        assert_eq!(
            agree(0, Ok(1)).unwrap(),
            waiting("changed during inspection")
        );
        let kept = decide_once(Err::<(), _>(keep("busy")).context("thread a")).unwrap();
        assert_eq!(kept, waiting("thread a: busy"));
        let kept = decide_once::<()>(Err(blocked("unpushed"))).unwrap();
        assert!(kept.is_err_and(|kept| kept.blocker));
        assert!(decide_once::<()>(Err(anyhow::anyhow!("format changed"))).is_err());
    }

    #[test]
    fn unreadable_directories_need_root() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        // Root reads through permission bits, so there is nothing to test.
        if unsafe { libc::geteuid() } == 0 {
            return Ok(());
        }
        let root = tempfile::tempdir()?;
        let folder = root.path().join("candidate");
        let locked = folder.join("locked");
        fs::create_dir_all(&locked)?;
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000))?;
        let device = folder.metadata()?.dev();
        let later = SystemTime::now() + Duration::from_secs(60);
        let result = inspect_tree(&folder, device, later, false);
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o700))?;
        // With passwordless sudo the folder goes through it; else it needs you.
        if salvage::root_available() {
            assert!(result?.sealed);
        } else {
            let error = result
                .err()
                .context("an unreadable folder passed inspection")?;
            assert!(
                error
                    .downcast_ref::<Keep>()
                    .is_some_and(|keep| keep.blocker),
                "{error:#}"
            );
        }
        Ok(())
    }

    #[test]
    fn prune_folders_removes_only_stale_directories() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = fs::canonicalize(temp.path())?;
        let now = SystemTime::now();
        let old = now - Duration::from_secs(10 * 86_400);
        let outside = tempfile::tempdir()?;
        fs::write(outside.path().join("keep"), b"x")?;
        let expired = root.join("expired");
        fs::create_dir_all(expired.join("deep"))?;
        fs::write(expired.join("deep/artifact"), b"x")?;
        std::os::unix::fs::symlink(outside.path(), expired.join("link"))?;
        let active = root.join("active");
        fs::create_dir_all(active.join("deep"))?;
        fs::write(active.join("deep/artifact"), b"x")?;
        age(&expired, old)?;
        age(&active, old)?;
        // A file created deep inside changes its directory: activity. An
        // in-place edit of an old file elsewhere is not looked at.
        fs::write(active.join("deep/new"), b"x")?;
        fs::write(expired.join("deep/artifact"), b"edited in place")?;
        fs::File::open(expired.join("deep/artifact"))?.set_modified(now)?;
        fs::create_dir(root.join("recent"))?;
        std::os::unix::fs::symlink("missing", root.join("broken"))?;
        fs::write(root.join("notes.txt"), b"x")?;
        let cutoff = now - Duration::from_secs(2 * 86_400);

        prune_folders("scratch", &root, cutoff, true, &mut Vec::new())?;
        assert!(expired.exists());
        prune_folders("scratch", &root, cutoff, false, &mut Vec::new())?;
        assert!(!expired.exists());
        for name in ["active", "recent", "broken", "notes.txt"] {
            assert!(
                fs::symlink_metadata(root.join(name)).is_ok(),
                "{name} was removed"
            );
        }
        assert!(outside.path().join("keep").exists());
        // A symlinked root is refused outright.
        let link = root.join("link-root");
        std::os::unix::fs::symlink(&active, &link)?;
        assert!(prune_folders("scratch", &link, cutoff, false, &mut Vec::new()).is_err());
        Ok(())
    }

    #[test]
    fn recent_sessions_keep_their_folders() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let database = temp.path().join("opencode.db");
        util::command_output(Command::new("sqlite3").arg(&database).arg(
            "CREATE TABLE session_v2 (directory TEXT, time_updated INTEGER);
             INSERT INTO session_v2 VALUES ('/old', 1000), ('/new/sub', 5000), ('/new/sub', 6000);",
        ))?;
        let sessions = session_activity(&database)?;
        let since = UNIX_EPOCH + Duration::from_millis(5500);
        assert!(check_sessions(&sessions, Path::new("/new"), since).is_err());
        assert!(check_sessions(&sessions, Path::new("/old"), since).is_ok());
        let later = UNIX_EPOCH + Duration::from_secs(10);
        assert!(check_sessions(&sessions, Path::new("/new"), later).is_ok());
        Ok(())
    }
}
