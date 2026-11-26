//! Saving what a workspace still holds before the cleanup removes it, and the
//! upkeep of what was saved. Uncommitted changes become a local
//! `sweep/wip/...` branch that is never pushed; a nested clone with unsaved
//! work becomes a Git bundle. Edits that already match the default branch
//! landed some other way and need no saving.
use crate::{util, worktree_prune::blocked};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs,
    io::Write as _,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

/// Uncommitted files larger than this in total stay with the user rather
/// than go into Git.
pub(crate) const SAVE_LIMIT: u64 = 100 << 20;
/// Saved work is dropped this long after its workspace went.
pub(crate) const KEEP_SAVED_DAYS: u64 = 30;
/// A removed worktree's branch is forgotten if it is not merged by then.
const WATCH_BRANCH_DAYS: u64 = 60;
const DAY: u64 = 86_400;

/// What to save from a checkout before its workspace goes. Branches come
/// before bundles, so a bundle carries the WIP branches made in its repository.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Save {
    /// Commit the checkout's uncommitted state, or only its HEAD, to a
    /// `sweep/wip/` branch of `repo`.
    Wip {
        checkout: PathBuf,
        repo: PathBuf,
        dirty: bool,
    },
    /// Bundle every ref of a repository that goes with the workspace.
    Bundle { repo: PathBuf },
}

/// Where bundles go, and what saved commits note about their workspace.
pub(crate) struct Store {
    pub(crate) bundles: PathBuf,
    pub(crate) note: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Place {
    Branch {
        repo: PathBuf,
        branch: String,
        commit: String,
    },
    Bundle {
        file: PathBuf,
    },
}

/// Work saved from a removed workspace.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Saved {
    pub(crate) at: u64,
    pub(crate) workspace: PathBuf,
    pub(crate) place: Place,
}

/// A removed worktree's branch, deleted once the default branch contains it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Watched {
    pub(crate) at: u64,
    pub(crate) repo: PathBuf,
    pub(crate) branch: String,
    pub(crate) tip: String,
}

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Git in `cwd` that never refreshes an index: a nested repository's index
/// lives in the candidate, and rewriting it would look like activity.
pub(crate) fn command(cwd: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .args(["--no-optional-locks", "-c", "core.fsmonitor=false", "-C"])
        .arg(cwd);
    command
}

fn run(command: &mut Command) -> Result<String> {
    Ok(String::from_utf8(util::command_output(command)?)?
        .trim_end()
        .to_owned())
}

pub(crate) fn git(cwd: &Path, args: &[&str]) -> Result<String> {
    run(command(cwd).args(args))
}

fn succeeds(cwd: &Path, args: &[&str]) -> bool {
    command(cwd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// The remote default branch, such as `origin/main`.
pub(crate) fn default_ref(checkout: &Path) -> Option<String> {
    if let Ok(head) = git(checkout, &["rev-parse", "--abbrev-ref", "origin/HEAD"])
        && head != "origin/HEAD"
    {
        return Some(head);
    }
    ["origin/main", "origin/master"]
        .into_iter()
        .find(|name| {
            succeeds(
                checkout,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("refs/remotes/{name}"),
                ],
            )
        })
        .map(str::to_owned)
}

/// Whether every changed path already holds what the default branch has.
fn landed(checkout: &Path, paths: &[PathBuf]) -> Result<bool> {
    let Some(base) = default_ref(checkout) else {
        return Ok(false);
    };
    let mut present = Vec::new();
    for path in paths {
        match checkout.join(path).symlink_metadata() {
            Ok(info) if info.is_file() => present.push(path),
            // Directories (nested repositories) and symlinks: compare nothing.
            Ok(_) => return Ok(false),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let mut on_base = BTreeMap::new();
    for chunk in paths.chunks(256) {
        let output = util::command_output(
            command(checkout)
                .args(["ls-tree", "-z", "--full-tree", &base, "--"])
                .args(chunk),
        )?;
        for record in output.split(|&byte| byte == 0).filter(|r| !r.is_empty()) {
            let (meta, path) = record
                .iter()
                .position(|&byte| byte == b'\t')
                .map(|tab| record.split_at(tab))
                .context("invalid ls-tree output")?;
            let fields: Vec<_> = meta.split(|&byte| byte == b' ').collect();
            if fields.get(1) != Some(&&b"blob"[..]) {
                return Ok(false);
            }
            on_base.insert(
                PathBuf::from(OsStr::from_bytes(&path[1..])),
                String::from_utf8_lossy(fields[2]).into_owned(),
            );
        }
    }
    let mut child = command(checkout)
        .args(["hash-object", "--stdin-paths"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut input = Vec::new();
    for path in &present {
        input.extend_from_slice(path.as_os_str().as_bytes());
        input.push(b'\n');
    }
    child
        .stdin
        .take()
        .context("hash-object stdin")?
        .write_all(&input)?;
    let output = child.wait_with_output()?;
    ensure!(output.status.success(), "git hash-object failed");
    let hashes: Vec<_> = String::from_utf8(output.stdout)?
        .lines()
        .map(str::to_owned)
        .collect();
    ensure!(
        hashes.len() == present.len(),
        "git hash-object skipped paths"
    );
    for path in paths {
        let mine = present
            .iter()
            .position(|candidate| *candidate == path)
            .map(|index| &hashes[index]);
        if mine != on_base.get(path) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// What saving a checkout's uncommitted `paths` takes: nothing when they
/// already landed, else a WIP branch of `repo`. Too much stays with the user.
pub(crate) fn plan_dirty(checkout: &Path, paths: &[PathBuf], repo: &Path) -> Result<Option<Save>> {
    if landed(checkout, paths)? {
        return Ok(None);
    }
    let bytes: u64 = paths
        .iter()
        .filter_map(|path| checkout.join(path).symlink_metadata().ok())
        .filter(|info| info.is_file())
        .map(|info| info.len())
        .sum();
    if bytes > SAVE_LIMIT {
        return Err(blocked(format!(
            "{} has {} MiB of uncommitted files, too much to save in Git",
            checkout.display(),
            bytes >> 20
        )));
    }
    Ok(Some(Save::Wip {
        checkout: checkout.to_owned(),
        repo: repo.to_owned(),
        dirty: true,
    }))
}

fn slug(path: &Path) -> String {
    let name = path
        .file_name()
        .map(OsStr::to_string_lossy)
        .unwrap_or_default();
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "workspace".into()
    } else {
        slug.into()
    }
}

fn label(workspace: &Path, checkout: &Path) -> String {
    let date = chrono::Local::now().format("%Y-%m-%d");
    if checkout == workspace {
        format!("{date}-{}", slug(workspace))
    } else {
        format!("{date}-{}-{}", slug(workspace), slug(checkout))
    }
}

/// Commit a checkout's state through a scratch index, leaving its own index,
/// branch and files alone, then point a new `sweep/wip/` branch at it.
fn save_wip(
    workspace: &Path,
    checkout: &Path,
    dirty: bool,
    note: &str,
    discard_scratch: bool,
) -> Result<(String, String)> {
    let head = git(checkout, &["rev-parse", "--verify", "--quiet", "HEAD"]).ok();
    let commit = if dirty {
        let scratch = tempfile::tempdir()?;
        let index = scratch.path().join("index");
        let indexed =
            |args: &[&str]| run(command(checkout).env("GIT_INDEX_FILE", &index).args(args));
        match &head {
            Some(head) => indexed(&["read-tree", head])?,
            None => indexed(&["read-tree", "--empty"])?,
        };
        // A linked worktree's root .scratch is disposable even if Git
        // would stage it. A nested checkout is still saved in full. `git add`
        // fails on a literal exclude naming an ignored path, and a global
        // ignore of .scratch is common, so the excludes are globs.
        indexed(if discard_scratch {
            &[
                "add",
                "--all",
                "--",
                ".",
                ":(top,exclude,glob).scratc[h]",
                ":(top,exclude,glob).scratc[h]/**",
            ]
        } else {
            &["add", "--all", "--", "."]
        })?;
        let tree = indexed(&["write-tree"])?;
        let message = format!(
            "WIP saved by sweep before removing {}\n\nCheckout: {}\n{note}",
            workspace.display(),
            checkout.display()
        );
        let mut commit = command(checkout);
        commit
            .args(["commit-tree", "--no-gpg-sign", &tree, "-m", &message])
            .envs([
                ("GIT_AUTHOR_NAME", "sweep"),
                ("GIT_AUTHOR_EMAIL", "sweep@localhost"),
                ("GIT_COMMITTER_NAME", "sweep"),
                ("GIT_COMMITTER_EMAIL", "sweep@localhost"),
            ]);
        if let Some(head) = &head {
            commit.args(["-p", head]);
        }
        run(&mut commit)?
    } else {
        head.context("a checkout without commits has nothing to keep")?
    };
    let base = format!("sweep/wip/{}", label(workspace, checkout));
    let mut branch = base.clone();
    let mut number = 2;
    while succeeds(
        checkout,
        &[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    ) {
        branch = format!("{base}-{number}");
        number += 1;
    }
    git(
        checkout,
        &["update-ref", &format!("refs/heads/{branch}"), &commit, ""],
    )?;
    Ok((branch, commit))
}

/// Save everything `saves` lists from `workspace`; return what outlives it.
pub(crate) fn apply(
    workspace: &Path,
    saves: &[Save],
    store: &Store,
    discard_scratch: bool,
) -> Result<Vec<Saved>> {
    let at = now();
    let mut saves = saves.to_vec();
    saves.sort();
    saves.dedup();
    let mut saved = Vec::new();
    for save in &saves {
        match save {
            Save::Wip {
                checkout,
                repo,
                dirty,
            } => {
                let (branch, commit) = save_wip(
                    workspace,
                    checkout,
                    *dirty,
                    &store.note,
                    discard_scratch && checkout == workspace,
                )
                .with_context(|| format!("save {}", checkout.display()))?;
                println!("Saved {} as branch {branch}", checkout.display());
                // A branch in a repository that goes too travels in its bundle.
                if !repo.starts_with(workspace) {
                    saved.push(Saved {
                        at,
                        workspace: workspace.to_owned(),
                        place: Place::Branch {
                            repo: repo.clone(),
                            branch,
                            commit,
                        },
                    });
                }
            }
            Save::Bundle { repo } => {
                fs::create_dir_all(&store.bundles)?;
                let checkout = repo.parent().unwrap_or(repo);
                let stem = label(workspace, checkout);
                let mut file = store.bundles.join(format!("{stem}.bundle"));
                let mut number = 2;
                while file.exists() {
                    file = store.bundles.join(format!("{stem}-{number}.bundle"));
                    number += 1;
                }
                util::command_output(
                    Command::new("git")
                        .arg("--git-dir")
                        .arg(repo)
                        .args(["bundle", "create"])
                        .arg(&file)
                        .arg("--all"),
                )
                .with_context(|| format!("bundle {}", repo.display()))?;
                println!("Saved {} as bundle {}", checkout.display(), file.display());
                saved.push(Saved {
                    at,
                    workspace: workspace.to_owned(),
                    place: Place::Bundle { file },
                });
            }
        }
    }
    Ok(saved)
}

/// sudo without a password, when this account has it.
fn sudo() -> Option<&'static Path> {
    static SUDO: OnceLock<Option<PathBuf>> = OnceLock::new();
    SUDO.get_or_init(|| {
        ["/run/wrappers/bin/sudo", "/usr/bin/sudo"]
            .into_iter()
            .map(PathBuf::from)
            .find(|sudo| {
                sudo.exists()
                    && Command::new(sudo)
                        .args(["-n", "true"])
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()
                        .is_ok_and(|status| status.success())
            })
    })
    .as_deref()
}

/// Whether files another user owns can be removed.
pub(crate) fn root_available() -> bool {
    sudo().is_some()
}

/// `fs::remove_dir_all` over subtrees in parallel. Unlinking dominates on
/// btrfs, and threads overlap it: about 1.6 times faster on 200k files.
/// The top levels are split until there are enough subtrees to share; their
/// directories go last, deepest first. Each split directory is opened without
/// following symlinks and then named through its descriptor, so swapping it
/// for a symlink mid-walk cannot redirect the removal outside `root`.
fn remove_dir_all_parallel(root: &Path) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    const SUBTREES: usize = 64;
    const SPLIT_DEPTH: usize = 4;
    if !root.symlink_metadata()?.is_dir() {
        return fs::remove_file(root);
    }
    let open = |path: &Path| {
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(path)
    };
    let held =
        |directory: &fs::File| PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()));
    let mut opened = vec![open(root)?];
    // Split directories, as (parent in `opened`, name), in walk order.
    let mut split = Vec::new();
    let mut listing = vec![0];
    let mut level = Vec::new();
    for depth in 0.. {
        for parent in listing.drain(..) {
            for entry in fs::read_dir(held(&opened[parent]))? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    level.push((parent, entry.file_name()));
                } else {
                    fs::remove_file(held(&opened[parent]).join(entry.file_name()))?;
                }
            }
        }
        if level.is_empty() || level.len() >= SUBTREES || depth == SPLIT_DEPTH {
            break;
        }
        for (parent, name) in level.drain(..) {
            listing.push(opened.len());
            opened.push(open(&held(&opened[parent]).join(&name))?);
            split.push((parent, name));
        }
    }
    let subtree = |(parent, name): &(usize, std::ffi::OsString)| held(&opened[*parent]).join(name);
    for result in crate::tree_scan::parallel(&level, |entry| fs::remove_dir_all(subtree(entry))) {
        result?;
    }
    for entry in split.iter().rev() {
        fs::remove_dir(subtree(entry))?;
    }
    fs::remove_dir(root)
}

/// Remove a workspace folder; files another user owns go through sudo.
pub(crate) fn remove_tree(path: &Path) -> Result<()> {
    match remove_dir_all_parallel(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            let sudo = sudo().with_context(|| format!("remove {}: {error}", path.display()))?;
            let home = PathBuf::from(std::env::var_os("HOME").context("HOME is unset")?);
            ensure!(
                path.is_absolute()
                    && path.starts_with(&home)
                    && path.components().count() >= home.components().count() + 2
                    && path.canonicalize()? == path,
                "refusing to remove {} as root",
                path.display()
            );
            util::command_output(
                Command::new(sudo)
                    .args(["-n", "rm", "-rf", "--one-file-system", "--"])
                    .arg(path),
            )?;
            ensure!(!path.exists(), "{} survived sudo rm", path.display());
            Ok(())
        }
        Err(error) => Err(error).with_context(|| format!("remove {}", path.display())),
    }
}

/// The branch a linked worktree has checked out, to delete once merged.
pub(crate) fn watch(worktree: &Path, repo: &Path) -> Option<Watched> {
    let head = git(worktree, &["rev-parse", "--symbolic-full-name", "HEAD"]).ok()?;
    let branch = head.strip_prefix("refs/heads/")?;
    if branch.starts_with("sweep/wip/") || matches!(branch, "main" | "master") {
        return None;
    }
    Some(Watched {
        at: now(),
        repo: repo.to_owned(),
        branch: branch.to_owned(),
        tip: git(worktree, &["rev-parse", "HEAD"]).ok()?,
    })
}

/// Delete watched branches the default branch contains; keep watching the
/// rest a while. A branch someone moved or checked out is theirs again.
pub(crate) fn prune_branches(watched: Vec<Watched>, now: u64) -> Vec<Watched> {
    watched
        .into_iter()
        .filter(|entry| {
            if now.saturating_sub(entry.at) > WATCH_BRANCH_DAYS * DAY {
                return false;
            }
            let reference = format!("refs/heads/{}", entry.branch);
            match git(
                &entry.repo,
                &["rev-parse", "--verify", "--quiet", &reference],
            ) {
                Ok(tip) if tip == entry.tip => {}
                _ => return false,
            }
            let checked_out =
                git(&entry.repo, &["worktree", "list", "--porcelain"]).is_ok_and(|list| {
                    list.lines()
                        .any(|line| line == format!("branch {reference}"))
                });
            if checked_out {
                return false;
            }
            let Some(base) = default_ref(&entry.repo) else {
                return true;
            };
            if !succeeds(
                &entry.repo,
                &["merge-base", "--is-ancestor", &entry.tip, &base],
            ) {
                return true;
            }
            match git(&entry.repo, &["branch", "-D", &entry.branch]) {
                Ok(_) => println!(
                    "Deleted branch {} of {}: {base} contains it",
                    entry.branch,
                    entry.repo.display()
                ),
                Err(error) => eprintln!("delete branch {}: {error:#}", entry.branch),
            }
            false
        })
        .collect()
}

/// Drop saved work older than `KEEP_SAVED_DAYS`, and records of work gone.
pub(crate) fn expire(saved: Vec<Saved>, now: u64) -> Vec<Saved> {
    saved
        .into_iter()
        .filter(|entry| {
            let old = now.saturating_sub(entry.at) > KEEP_SAVED_DAYS * DAY;
            match &entry.place {
                Place::Branch {
                    repo,
                    branch,
                    commit,
                } => {
                    let reference = format!("refs/heads/{branch}");
                    let current = git(repo, &["rev-parse", "--verify", "--quiet", &reference]);
                    if current.as_ref().ok() != Some(commit) {
                        return false;
                    }
                    if old {
                        match git(repo, &["branch", "-D", branch]) {
                            Ok(_) => {
                                println!("Dropped saved branch {branch} of {}", repo.display())
                            }
                            Err(error) => eprintln!("drop {branch}: {error:#}"),
                        }
                    }
                    !old
                }
                Place::Bundle { file } => {
                    if !file.exists() {
                        return false;
                    }
                    if old {
                        match fs::remove_file(file) {
                            Ok(()) => println!("Dropped saved bundle {}", file.display()),
                            Err(error) => eprintln!("drop {}: {error}", file.display()),
                        }
                    }
                    !old
                }
            }
        })
        .collect()
}

/// Where bundles of removed clones go.
pub(crate) fn bundles(home: &Path) -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"))
        .join("sweep/bundles")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_git(cwd: &Path, args: &[&str]) -> Result<String> {
        run(Command::new("git")
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
            .env("GIT_CONFIG_NOSYSTEM", "1"))
    }

    /// A clone of a repository with one commit on `main`, pushed.
    #[test]
    fn parallel_removal_empties_deep_and_wide_trees_without_following_symlinks() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let outside = temp.path().join("outside");
        fs::create_dir(&outside)?;
        fs::write(outside.join("keep"), "")?;
        let root = temp.path().join("root");
        let deep = root.join("a/b/c/d/e/f");
        fs::create_dir_all(&deep)?;
        fs::write(deep.join("file"), "")?;
        for wide in 0..100 {
            fs::create_dir_all(root.join(format!("wide/{wide}/inner")))?;
            fs::write(root.join(format!("wide/{wide}/file")), "")?;
        }
        std::os::unix::fs::symlink(&outside, root.join("a/b/link"))?;
        std::os::unix::fs::symlink(&outside, root.join("a/b/c/d/e/link"))?;
        fs::write(root.join("top"), "")?;
        remove_dir_all_parallel(&root)?;
        assert!(!root.exists());
        assert!(outside.join("keep").exists());
        // A symlink root is unlinked, not followed.
        std::os::unix::fs::symlink(&outside, &root)?;
        remove_dir_all_parallel(&root)?;
        assert!(root.symlink_metadata().is_err() && outside.join("keep").exists());
        Ok(())
    }

    fn clone() -> Result<(tempfile::TempDir, PathBuf)> {
        let temp = tempfile::tempdir()?;
        let base = temp.path().canonicalize()?;
        run_git(
            &base,
            &["init", "--quiet", "--bare", "-b", "main", "remote.git"],
        )?;
        let checkout = base.join("checkout");
        run_git(&base, &["clone", "--quiet", "remote.git", "checkout"])?;
        fs::write(checkout.join("file.txt"), "one\n")?;
        run_git(&checkout, &["add", "file.txt"])?;
        run_git(&checkout, &["commit", "--quiet", "-m", "init"])?;
        run_git(&checkout, &["push", "--quiet", "origin", "main"])?;
        Ok((temp, checkout))
    }

    #[test]
    fn edits_already_on_the_default_branch_need_no_saving() -> Result<()> {
        let (_temp, checkout) = clone()?;
        // The same edit landed upstream some other way, such as a squash.
        let other = checkout.with_file_name("other");
        run_git(
            checkout.parent().unwrap(),
            &["clone", "--quiet", "remote.git", "other"],
        )?;
        fs::write(other.join("file.txt"), "two\n")?;
        run_git(&other, &["commit", "--quiet", "-am", "landed"])?;
        run_git(&other, &["push", "--quiet", "origin", "main"])?;
        run_git(&checkout, &["fetch", "--quiet"])?;
        fs::write(checkout.join("file.txt"), "two\n")?;
        let repo = checkout.join(".git");
        let paths = [PathBuf::from("file.txt")];
        assert_eq!(plan_dirty(&checkout, &paths, &repo)?, None);
        fs::write(checkout.join("file.txt"), "three\n")?;
        assert!(matches!(
            plan_dirty(&checkout, &paths, &repo)?,
            Some(Save::Wip { dirty: true, .. })
        ));
        // A new file the default branch lacks is work.
        fs::write(checkout.join("file.txt"), "two\n")?;
        fs::write(checkout.join("new.txt"), "new\n")?;
        let paths = [PathBuf::from("file.txt"), PathBuf::from("new.txt")];
        assert!(plan_dirty(&checkout, &paths, &repo)?.is_some());
        Ok(())
    }

    #[test]
    fn wip_commits_leave_the_checkout_alone() -> Result<()> {
        let (_temp, checkout) = clone()?;
        fs::write(checkout.join("file.txt"), "edited\n")?;
        fs::write(checkout.join("untracked.txt"), "new\n")?;
        let saves = [Save::Wip {
            checkout: checkout.clone(),
            repo: checkout.join(".git"),
            dirty: true,
        }];
        let store = Store {
            bundles: checkout.with_file_name("bundles"),
            note: "T3 threads: none".into(),
        };
        let saved = apply(&checkout.with_file_name("workspace"), &saves, &store, false)?;
        let Place::Branch { branch, .. } = &saved[0].place else {
            panic!("expected a branch");
        };
        assert!(branch.starts_with("sweep/wip/"), "{branch}");
        assert_eq!(
            run_git(&checkout, &["show", &format!("{branch}:file.txt")])?,
            "edited"
        );
        assert_eq!(
            run_git(&checkout, &["show", &format!("{branch}:untracked.txt")])?,
            "new"
        );
        // The checkout's branch and index are untouched.
        assert_eq!(run_git(&checkout, &["branch", "--show-current"])?, "main");
        assert_eq!(
            run_git(&checkout, &["status", "--porcelain"])?,
            " M file.txt\n?? untracked.txt"
        );
        Ok(())
    }

    #[test]
    fn merged_watched_branches_go_and_saved_work_expires() -> Result<()> {
        let (_temp, checkout) = clone()?;
        let tip = run_git(&checkout, &["rev-parse", "HEAD"])?;
        run_git(&checkout, &["branch", "merged"])?;
        run_git(&checkout, &["switch", "--quiet", "-c", "open"])?;
        run_git(
            &checkout,
            &["commit", "--quiet", "--allow-empty", "-m", "open"],
        )?;
        let open_tip = run_git(&checkout, &["rev-parse", "HEAD"])?;
        run_git(&checkout, &["switch", "--quiet", "main"])?;
        let watched = |branch: &str, tip: &str| Watched {
            at: now(),
            repo: checkout.clone(),
            branch: branch.into(),
            tip: tip.into(),
        };
        let left = prune_branches(
            vec![watched("merged", &tip), watched("open", &open_tip)],
            now(),
        );
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].branch, "open");
        assert!(run_git(&checkout, &["rev-parse", "--verify", "--quiet", "merged"]).is_err());
        // Saved work goes after its retention; a branch moved since is the user's.
        run_git(&checkout, &["branch", "sweep/wip/old", &open_tip])?;
        let saved = |branch: &str, commit: &str, at| Saved {
            at,
            workspace: checkout.clone(),
            place: Place::Branch {
                repo: checkout.join(".git"),
                branch: branch.into(),
                commit: commit.into(),
            },
        };
        let later = now() + (KEEP_SAVED_DAYS + 1) * DAY;
        let kept = expire(vec![saved("sweep/wip/old", &open_tip, now())], now());
        assert_eq!(kept.len(), 1);
        assert!(expire(vec![saved("sweep/wip/old", &tip, now())], now()).is_empty());
        assert!(expire(kept, later).is_empty());
        assert!(
            run_git(
                &checkout,
                &["rev-parse", "--verify", "--quiet", "sweep/wip/old"]
            )
            .is_err()
        );
        Ok(())
    }
}
