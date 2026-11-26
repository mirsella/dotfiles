//! `sweep`: the agent workspace cleanup. The hourly `sweep` user timer runs
//! `sweep auto`; the other commands inspect and steer it by hand.
//!
//! It covers task scratch under ~/dev/scratch, T3's scratch folders and
//! worktrees, OpenCode's worktrees and linked worktrees under ~/dev. T3's own
//! worktree cleanup stays disabled so this is the only owner; T3 still prunes
//! its logs and browser artifacts.
//!
//! Every command shares one engine. `Sources` lists the candidates and checks
//! them all in parallel, the ones that need no T3 state while it loads.
//! Activity is read cheaply: directory changes, Git's own files, changed
//! files Git would lose, and owner state; never every file's time. A cleanup
//! rechecks what passed against fresh state before removing it; `status`
//! classifies everything, estimates when it goes and caches sizes. An idle
//! workspace that cannot go without losing work raises one notification when
//! it first blocks, and a failed run one per distinct failure; reported.json in
//! the state directory remembers what was said.
use crate::{
    attention, monitoring,
    salvage::{self, Place, Saved, Watched},
    t3_cleanup, tree_scan,
    util::{load_json, store_json},
    worktree_prune::{
        self, Blocked, Candidate, Decision, Fingerprint, Kept, Kind, Precheck, decide_once,
    },
};
use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    io::{BufRead, IsTerminal, Write},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Idle days before agent workspaces and before ~/dev worktrees go.
pub(crate) const AGENT_RETENTION_DAYS: u64 = 3;
pub(crate) const DEV_RETENTION_DAYS: u64 = 7;
/// T3 threads listed under each workspace in the attention notification.
const THREAD_LINKS: usize = 5;

fn cutoff(now: SystemTime, days: u64) -> Result<SystemTime> {
    now.checked_sub(Duration::from_secs(days.saturating_mul(86_400)))
        .context("invalid retention period")
}

/// The timer's state directory, shared with every command: keep.json,
/// reported.json and the lock.
pub(crate) fn state_dir(home: &Path) -> PathBuf {
    std::env::var_os("STATE_DIRECTORY")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/state/sweep"))
}

/// A path relative to HOME, as `sweep` prints it.
fn relative(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(relative) => relative.display().to_string(),
        Err(_) => path.display().to_string(),
    }
}

/// Resolve the parent, but keep the final directory entry (not a symlink target).
fn entry_path(path: &Path) -> Result<PathBuf> {
    match path.file_name() {
        Some(name) => {
            let parent = path.parent().filter(|path| !path.as_os_str().is_empty());
            Ok(parent.unwrap_or(Path::new(".")).canonicalize()?.join(name))
        }
        None => Ok(path.canonicalize()?),
    }
}

fn input_paths(home: &Path, path: &Path) -> impl Iterator<Item = PathBuf> {
    std::iter::once(path.to_owned()).chain(path.is_relative().then(|| home.join(path)))
}

/// What the notifications already covered.
#[derive(Default, Deserialize, Serialize)]
struct Reported {
    blocked: BTreeSet<PathBuf>,
    failure: Option<String>,
}

/// Notify about newly blocked workspaces and new failures, then remember them.
/// `links` lists the T3 threads behind each workspace, for the T3 Code thread.
fn report(
    home: &Path,
    blocked: &[Blocked],
    links: &BTreeMap<PathBuf, Vec<String>>,
    failure: Option<String>,
) -> Result<()> {
    let dir = state_dir(home);
    let file = dir.join("reported.json");
    // A damaged record only repeats notifications.
    let previous: Reported = load_json(&file).unwrap_or_else(|error| {
        eprintln!("Ignoring {error:#}");
        Reported::default()
    });
    let current: BTreeSet<_> = blocked.iter().map(|entry| entry.path.clone()).collect();
    let new: Vec<_> = blocked
        .iter()
        .filter(|entry| !previous.blocked.contains(&entry.path))
        .collect();
    let mut reported = Reported {
        // Unblocked paths are forgotten, so they notify again if they block again.
        blocked: previous.blocked.intersection(&current).cloned().collect(),
        failure: None,
    };
    if !new.is_empty() {
        let line = |entry: &Blocked| {
            let reason = entry.reason.replace(&format!("{}/", home.display()), "~/");
            format!("~/{}: {reason}", relative(&entry.path, home))
        };
        let mut body: Vec<_> = new.iter().take(8).map(|entry| line(entry)).collect();
        if new.len() > 8 {
            body.push(format!("…and {} more", new.len() - 8));
        }
        body.push(format!(
            "\n{} idle in total. Save or discard the work, or lock a worktree to keep it.",
            current.len()
        ));
        let details = blocked
            .iter()
            .map(|entry| {
                let mut text = format!("- {}", line(entry));
                if let Some(links) = links.get(&entry.path) {
                    text.push_str(&format!("\n  T3 threads: {}", links.join(", ")));
                }
                text
            })
            .collect::<Vec<_>>()
            .join("\n");
        let request = attention::Request {
            summary: match new.len() {
                1 => "An idle workspace needs attention".to_owned(),
                count => format!("{count} idle workspaces need attention"),
            },
            body: body.join("\n"),
            details,
        };
        match attention::post(&dir, &request) {
            Ok(()) => reported.blocked = current,
            Err(error) => eprintln!("Could not show the attention notification: {error:#}"),
        }
    }
    if let Some(failure) = failure {
        reported.failure = previous.failure.clone();
        if previous.failure.as_ref() != Some(&failure) {
            let details: String = failure.chars().take(600).collect();
            let body = format!("{details}\n\nSee journalctl --user -u sweep");
            // Headless hosts have no notification daemon; the journal keeps the details.
            match monitoring::storage_alert("Agent workspace cleanup failed", &body, true) {
                Ok(()) => reported.failure = Some(failure),
                Err(error) => eprintln!("Could not show the notification: {error:#}"),
            }
        }
    }
    store_json(&file, &reported)
}

/// The hourly run: remove what is due, then notify about what needs attention.
fn auto(home: &Path) -> Result<()> {
    let mut blocked = Vec::new();
    let mut links = BTreeMap::new();
    let result = (|| -> Result<()> {
        let dir = state_dir(home);
        // A manual `sweep clean` and the hourly run never remove concurrently.
        let _lock = lock(&dir)?;
        let keeps = Keeps::load(&dir)?;
        let policy = Policy::standard(SystemTime::now())?;
        let (sources, result) = cleanup(home, policy, &keeps, None, false, &mut blocked);
        links = sources.thread_links(&blocked);
        result
    })();
    for entry in &blocked {
        println!(
            "Needs attention: {}: {}",
            entry.path.display(),
            entry.reason
        );
        if let Some(links) = links.get(&entry.path) {
            println!("  T3 threads: {}", links.join(", "));
        }
    }
    let failure = result.as_ref().err().map(|error| format!("{error:#}"));
    if let Err(error) = report(home, &blocked, &links, failure) {
        eprintln!("Could not record the notifications: {error:#}");
    }
    result
}

/// The cutoff each kind of workspace must have been idle since.
#[derive(Clone, Copy)]
pub(crate) struct Policy {
    pub(crate) agent: SystemTime,
    pub(crate) dev: SystemTime,
}

impl Policy {
    pub(crate) fn uniform(cutoff: SystemTime) -> Self {
        Self {
            agent: cutoff,
            dev: cutoff,
        }
    }

    pub(crate) fn cutoff(&self, kind: Kind) -> SystemTime {
        if kind == Kind::Dev {
            self.dev
        } else {
            self.agent
        }
    }

    fn standard(now: SystemTime) -> Result<Self> {
        Ok(Self {
            agent: cutoff(now, AGENT_RETENTION_DAYS)?,
            dev: cutoff(now, DEV_RETENTION_DAYS)?,
        })
    }
}

/// Paths `sweep keep` protects in place, with an optional end in Unix seconds.
#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Keeps(BTreeMap<PathBuf, Option<u64>>);

/// Removing a workspace would remove this path or part of its contents.
fn overlaps(workspace: &Path, kept: &Path) -> bool {
    kept.starts_with(workspace) || workspace.starts_with(kept)
}

/// The outermost removal unit that contains an item. A hold protects its
/// contents too, even when sweep lists a nested workspace separately.
fn protection_root<'a>(path: &Path, candidates: &'a [Candidate]) -> Option<&'a Path> {
    candidates
        .iter()
        .map(|candidate| candidate.path.as_path())
        .filter(|workspace| path.starts_with(workspace))
        .min_by_key(|workspace| workspace.as_os_str().len())
}

fn unix(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

fn local(seconds: u64) -> String {
    DateTime::<Local>::from(UNIX_EPOCH + Duration::from_secs(seconds))
        .format("%a %d %b %H:%M")
        .to_string()
}

impl Keeps {
    pub(crate) fn load(dir: &Path) -> Result<Self> {
        load_json(&dir.join("keep.json"))
    }

    fn drop_expired(&mut self, now: SystemTime) {
        self.0
            .retain(|_, until| until.is_none_or(|until| until > unix(now)));
    }

    fn save(&mut self, dir: &Path, now: SystemTime) -> Result<()> {
        self.drop_expired(now);
        store_json(&dir.join("keep.json"), self)
    }

    fn holds<'a>(
        &'a self,
        candidates: &[Candidate],
        now: SystemTime,
    ) -> impl Fn(&Path) -> Option<String> + 'a {
        let now = unix(now);
        let active: Vec<_> = self
            .0
            .iter()
            .filter(|(_, until)| until.is_none_or(|at| at > now))
            .map(|(path, until)| {
                let root = protection_root(path, candidates).unwrap_or(path);
                (root.to_owned(), path, *until)
            })
            .collect();
        move |workspace| {
            let (_, path, until) = active
                .iter()
                .filter(|(root, _, _)| overlaps(workspace, root))
                .max_by_key(|(_, _, until)| until.unwrap_or(u64::MAX))?;
            let mut reason = "kept with `sweep keep`".to_owned();
            if *path != workspace {
                reason.push_str(&format!(" for {}", path.display()));
            }
            if let Some(until) = until {
                reason.push_str(&format!(" until {}", local(*until)));
            }
            Some(reason)
        }
    }
}

/// Discover the removal units without loading session, process, or owner state.
fn discover(home: &Path) -> (Vec<Candidate>, Vec<String>) {
    let t3 = home.join(".t3");
    let lists = [
        (
            "task scratch",
            worktree_prune::folder_candidates(Kind::TaskScratch, &home.join("dev/scratch"), 1),
        ),
        (
            "T3 scratch",
            worktree_prune::folder_candidates(Kind::T3Scratch, &t3.join("scratch"), 1),
        ),
        (
            "T3 worktrees",
            worktree_prune::folder_candidates(Kind::T3Worktree, &t3.join("worktrees"), 2),
        ),
        (
            "OpenCode worktrees",
            worktree_prune::folder_candidates(
                Kind::OpenCode,
                &home.join(".local/share/opencode/worktree"),
                2,
            ),
        ),
        (
            "~/dev worktrees",
            worktree_prune::dev_candidates(&home.join("dev")),
        ),
    ];
    let (mut candidates, mut failures) = (Vec::new(), Vec::new());
    for (label, list) in lists {
        match list {
            Ok(found) => candidates.extend(found),
            Err(error) => failures.push(format!("{label}: {error:#}")),
        }
    }
    (candidates, failures)
}

/// One cleanup at a time: the hourly run and a manual `sweep clean` must not race.
pub(crate) fn lock(dir: &Path) -> Result<fs::File> {
    fs::create_dir_all(dir)?;
    let file = fs::File::create(dir.join("lock"))?;
    // SAFETY: flock only reads the descriptor, which `file` keeps open.
    let status = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
    ensure!(
        status == 0,
        "lock {}: {}",
        dir.display(),
        std::io::Error::last_os_error()
    );
    Ok(file)
}

/// Check every candidate (only those in `only`, when given), recheck what
/// passes against fresh owner state and remove it.
fn cleanup(
    home: &Path,
    policy: Policy,
    keeps: &Keeps,
    only: Option<&BTreeSet<PathBuf>>,
    dry_run: bool,
    blocked: &mut Vec<Blocked>,
) -> (Sources, Result<()>) {
    let now = SystemTime::now();
    let mut sources = Sources::list(home);
    let held = keeps.holds(&sources.candidates, now);
    if let Some(only) = only {
        sources
            .candidates
            .retain(|candidate| only.contains(&candidate.path));
    }
    let cutoff = |kind| policy.cutoff(kind);
    let first = sources.first_pass(&cutoff, &held);
    if !dry_run && first.iter().any(|decision| matches!(decision, Ok(Ok(_)))) {
        sources.refresh();
    }
    let candidates = std::mem::take(&mut sources.candidates);
    let mut reports = BTreeMap::new();
    let mut outcomes = Vec::new();
    worktree_prune::confirm(
        &candidates,
        first,
        &cutoff,
        &|candidate, cutoff| sources.check(candidate, cutoff),
        dry_run,
        &mut reports,
        &salvage::bundles(home),
        &|path| sources.note(path),
        &mut outcomes,
    );
    sources.candidates = candidates;
    let mut failures = std::mem::take(&mut sources.failures);
    if !dry_run && let Err(error) = upkeep(home, outcomes) {
        failures.push(format!("saved work: {error:#}"));
    }
    if !dry_run
        && let Err(error) =
            worktree_prune::tidy_opencode(&home.join(".local/share/opencode/worktree"))
    {
        failures.push(format!("OpenCode worktrees: {error:#}"));
    }
    for (kind, report) in reports {
        if let Err(error) = report.finish(kind.plural(), dry_run, blocked) {
            failures.push(format!("{error:#}"));
        }
    }
    let result = if failures.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("{}", failures.join("\n")))
    };
    (sources, result)
}

/// Record what removals saved and which branches to watch, drop saved work
/// past its time, and delete watched branches the default branch contains.
fn upkeep(home: &Path, outcomes: Vec<worktree_prune::Outcome>) -> Result<()> {
    let now = salvage::now();
    let (saved_file, branches_file) = (
        state_dir(home).join("saved.json"),
        state_dir(home).join("branches.json"),
    );
    let mut saved: Vec<Saved> = load_json(&saved_file)?;
    let mut watched: Vec<Watched> = load_json(&branches_file)?;
    for outcome in outcomes {
        saved.extend(outcome.saved);
        watched.extend(outcome.watched);
    }
    store_json(&saved_file, &salvage::expire(saved, now))?;
    store_json(&branches_file, &salvage::prune_branches(watched, now))
}

/// When a workspace goes, as `status` sees it.
#[derive(Debug)]
pub(crate) enum When {
    /// It passes every check now.
    Now,
    /// It goes once idle long enough; `blocker` says what will hold it then.
    Later {
        blocker: Option<String>,
    },
    /// It is idle but holds work the cleanup will not lose.
    Blocked(String),
    /// Something uses or holds it; no estimate.
    Waiting(String),
    Failed(String),
}

pub(crate) struct Row {
    kind: Kind,
    path: PathBuf,
    device: u64,
    when: When,
    size: Option<u64>,
    due: Option<SystemTime>,
    branch: Option<String>,
    /// What removing it saves first.
    plan: Option<String>,
}

/// How the first check's verdict turns into a status. A waiting workspace
/// is checked again as if idle long enough, for an estimate.
fn classify(
    sources: &Sources,
    candidate: &Candidate,
    first: &Decision<Fingerprint>,
    held: bool,
    now: SystemTime,
) -> (When, Option<String>) {
    match first {
        Ok(Ok(checked)) => (When::Now, checked.plan()),
        Ok(Err(found)) if found.blocker => (When::Blocked(found.reason.clone()), None),
        Ok(Err(found)) if held => (When::Waiting(found.reason.clone()), None),
        Ok(Err(_)) => match decide_once(sources.check(candidate, now)) {
            Ok(Ok(checked)) => (When::Later { blocker: None }, checked.plan()),
            Ok(Err(Kept {
                reason,
                blocker: true,
            })) => (
                When::Later {
                    blocker: Some(reason),
                },
                None,
            ),
            Ok(Err(found)) => (When::Waiting(found.reason), None),
            Err(error) => (When::Failed(format!("{error:#}")), None),
        },
        Err(error) => (When::Failed(format!("{error:#}")), None),
    }
}

/// Sizes from earlier runs, valid while a workspace's activity signal holds.
#[derive(Default, Serialize, Deserialize)]
struct Sizes(BTreeMap<PathBuf, (String, u64)>);

/// Estimate when each workspace that waits only on idleness goes, and
/// measure sizes. Estimates use cheap signs: the owner's last activity, Git's
/// files and the workspace's own time, or the newest change a check's walk
/// found. A size is reused while those signs hold; a busy owner moves them.
/// Waiting workspaces are not walked just for their size: the busy ones hold
/// the largest build outputs.
fn measure(
    rows: &mut [Row],
    policy: Policy,
    now: SystemTime,
    activity: &[(PathBuf, SystemTime)],
    dir: &Path,
) {
    let file = dir.join("sizes.json");
    // A damaged cache only costs a rescan.
    let cached: Sizes = load_json(&file).unwrap_or_default();
    let mut sizes = Sizes::default();
    let nanos = |time: Option<SystemTime>| {
        time.and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |since| since.as_nanos())
    };
    let mut walks = Vec::new();
    for (index, row) in rows.iter_mut().enumerate() {
        if matches!(row.when, When::Failed(_)) {
            continue;
        }
        let scan = tree_scan::remembered(&row.path);
        let own = row
            .path
            .symlink_metadata()
            .and_then(|info| info.modified())
            .ok();
        let git = row
            .kind
            .linked()
            .then(|| worktree_prune::git_activity(&row.path))
            .flatten()
            .map(|(time, _)| time);
        let owner = activity
            .iter()
            .filter(|(path, _)| path.starts_with(&row.path))
            .map(|(_, time)| *time)
            .max();
        if matches!(row.when, When::Later { .. }) {
            let newest = scan.as_ref().map(|scan| scan.newest);
            let idle_since = [owner, newest, own, git]
                .into_iter()
                .flatten()
                .max()
                .unwrap_or(now);
            let retention = now
                .duration_since(policy.cutoff(row.kind))
                .unwrap_or_default();
            row.due = Some(idle_since + retention);
        }
        let signal = format!("{}:{}:{}", nanos(own), nanos(git), nanos(owner));
        row.size = scan.and_then(|scan| scan.size).or_else(|| {
            cached
                .0
                .get(&row.path)
                .filter(|(known, _)| *known == signal)
                .map(|(_, size)| *size)
        });
        match row.size {
            Some(size) => {
                sizes.0.insert(row.path.clone(), (signal, size));
            }
            None if !matches!(row.when, When::Waiting(_)) => walks.push((index, signal)),
            None => {}
        }
    }
    let measured = tree_scan::parallel(&walks, |(index, _)| {
        let row = &rows[*index];
        let own_git = row.kind.linked().then(|| row.path.join(".git"));
        tree_scan::complete(&row.path, row.device, own_git.as_deref(), true)
            .ok()
            .and_then(|scan| scan.size)
    });
    for ((index, signal), size) in walks.into_iter().zip(measured) {
        if let Some(size) = size {
            rows[index].size = Some(size);
            sizes.0.insert(rows[index].path.clone(), (signal, size));
        }
    }
    if let Err(error) = store_json(&file, &sizes) {
        eprintln!("warning: could not cache sizes: {error:#}");
    }
}

/// The branch a checkout or worktree has checked out, read from its HEAD.
fn branch(path: &Path) -> Option<String> {
    let dot_git = path.join(".git");
    let git_dir = match fs::symlink_metadata(&dot_git).ok()? {
        info if info.is_dir() => dot_git,
        info if info.is_file() => {
            let text = fs::read_to_string(&dot_git).ok()?;
            let dir = Path::new(text.trim().strip_prefix("gitdir:")?.trim());
            path.join(dir)
        }
        _ => return None,
    };
    let head = fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    Some(match head.strip_prefix("ref: refs/heads/") {
        Some(branch) => branch.to_owned(),
        None => format!("detached {}", head.get(..8).unwrap_or(head)),
    })
}

type Loading = std::thread::JoinHandle<Result<t3_cleanup::State>>;

/// Every candidate, and the owner state that guards them.
pub(crate) struct Sources {
    database: PathBuf,
    /// T3's state reads in the background: it waits on T3's server.
    loading: Option<Loading>,
    t3: Option<t3_cleanup::State>,
    t3_error: Option<String>,
    sessions: std::result::Result<Vec<(PathBuf, SystemTime)>, String>,
    candidates: Vec<Candidate>,
    failures: Vec<String>,
    /// Owner-free checks of linked worktrees, run while T3's state loads.
    prechecked: std::sync::Mutex<HashMap<PathBuf, (SystemTime, Precheck)>>,
}

impl Sources {
    /// List every candidate while T3's state starts loading.
    pub(crate) fn list(home: &Path) -> Self {
        let base = home.join(".t3");
        let loading = base.is_dir().then(|| {
            let base = base.clone();
            std::thread::spawn(move || t3_cleanup::State::read(&base))
        });
        let database = home.join(".local/share/opencode/opencode.db");
        let sessions =
            worktree_prune::session_activity(&database).map_err(|error| format!("{error:#}"));
        let (mut candidates, failures) = discover(home);
        // Candidates that need T3 state last, so `first_pass` checks the rest while it loads.
        candidates.sort_by_key(|candidate| candidate.kind.needs_t3());
        Self {
            sessions,
            database,
            loading,
            t3: None,
            t3_error: None,
            candidates,
            failures,
            prechecked: Default::default(),
        }
    }

    /// Check every candidate once, in parallel, sharing scans. Candidates that
    /// need no T3 state go first, then the owner-free part of the rest runs
    /// while it loads: Git's verdict, but no tree walk.
    pub(crate) fn first_pass(
        &mut self,
        cutoff: &(dyn Fn(Kind) -> SystemTime + Sync),
        held: &(dyn Fn(&Path) -> Option<String> + Sync),
    ) -> Vec<Decision<Fingerprint>> {
        let _memory = tree_scan::remember();
        // `list` orders the candidates that need T3 state last.
        let split = self
            .candidates
            .partition_point(|candidate| !candidate.kind.needs_t3());
        let mut decisions = {
            let this = &*self;
            let (early, late) = this.candidates.split_at(split);
            let decisions =
                worktree_prune::first_pass(early, cutoff, held, &|candidate, cutoff| {
                    this.check(candidate, cutoff)
                });
            let prechecks = tree_scan::parallel(late, |candidate| {
                if held(&candidate.path).is_some() {
                    return None;
                }
                let cutoff = cutoff(candidate.kind);
                let linked = candidate.kind.linked();
                let precheck = linked
                    .then(|| worktree_prune::precheck_worktree(&candidate.path, cutoff).ok())
                    .flatten();
                // Walk ahead only what nothing but its owner can still keep.
                let idle = match &precheck {
                    Some(precheck) => precheck.idle_and_clean(),
                    None => {
                        !linked
                            && candidate
                                .path
                                .symlink_metadata()
                                .and_then(|info| info.modified())
                                .is_ok_and(|time| time < cutoff)
                    }
                };
                if idle {
                    let own_git = linked.then(|| candidate.path.join(".git"));
                    let _ = tree_scan::complete(
                        &candidate.path,
                        candidate.device,
                        own_git.as_deref(),
                        false,
                    );
                }
                precheck.map(|precheck| (cutoff, precheck))
            });
            this.prechecked.lock().expect("prechecks poisoned").extend(
                late.iter()
                    .zip(prechecks)
                    .filter_map(|(candidate, precheck)| Some((candidate.path.clone(), precheck?))),
            );
            decisions
        };
        if let Some(loading) = self.loading.take() {
            match loading.join() {
                Ok(Ok(state)) => self.t3 = Some(state),
                Ok(Err(error)) => self.t3_error = Some(format!("{error:#}")),
                Err(_) => self.t3_error = Some("reading T3 state panicked".into()),
            }
        }
        if let Some(error) = &self.t3_error {
            // Keep everything T3 might use when its state cannot be read.
            self.failures.push(format!("T3 state: {error}"));
            decisions.extend(self.candidates[split..].iter().map(|candidate| {
                Ok(Err(Kept {
                    reason: held(&candidate.path)
                        .unwrap_or_else(|| "T3's state is unreadable".into()),
                    blocker: false,
                }))
            }));
        } else {
            let this = &*self;
            decisions.extend(worktree_prune::first_pass(
                &this.candidates[split..],
                cutoff,
                held,
                &|candidate, cutoff| this.check(candidate, cutoff),
            ));
        }
        decisions
    }

    /// Every check of one candidate against `cutoff`.
    pub(crate) fn check(&self, candidate: &Candidate, cutoff: SystemTime) -> Result<Fingerprint> {
        let Candidate {
            kind, path, device, ..
        } = candidate;
        let t3 = || match (&self.t3, &self.t3_error) {
            (_, Some(error)) => bail!("T3 state is unreadable: {error}"),
            (state, None) => Ok(state.as_ref()),
        };
        match kind {
            Kind::TaskScratch => worktree_prune::inspect_tree(path, *device, cutoff, false),
            Kind::T3Scratch => t3()?
                .context("T3 state was not read")?
                .check_scratch(path, *device, cutoff),
            Kind::T3Worktree | Kind::OpenCode | Kind::Dev => {
                // A precheck holds only for the cutoff it was made against.
                let precheck = self
                    .prechecked
                    .lock()
                    .expect("prechecks poisoned")
                    .remove(path)
                    .filter(|(at, _)| *at == cutoff)
                    .map(|(_, precheck)| precheck);
                worktree_prune::check_worktree(
                    path,
                    *device,
                    cutoff,
                    || {
                        let mut required = cutoff;
                        if kind.needs_t3() {
                            match t3()? {
                                Some(state) => {
                                    required = state.check(path, cutoff.into(), false)?.into();
                                }
                                // Without ~/.t3 no T3 thread can use a ~/dev worktree.
                                None => ensure!(*kind == Kind::Dev, "T3 state was not read"),
                            }
                        }
                        // T3 runs OpenCode threads as OpenCode sessions in the worktree.
                        let sessions = self
                            .sessions
                            .as_ref()
                            .map_err(|error| anyhow::anyhow!("read OpenCode sessions: {error}"))?;
                        worktree_prune::check_sessions(sessions, path, cutoff)?;
                        Ok(required)
                    },
                    precheck,
                )
            }
        }
    }

    /// Re-read owner state right before removals.
    pub(crate) fn refresh(&mut self) {
        self.sessions =
            worktree_prune::session_activity(&self.database).map_err(|error| format!("{error:#}"));
        if let Some(state) = &mut self.t3
            && let Err(error) = state.refresh()
        {
            self.t3 = None;
            self.t3_error = Some(format!("{error:#}"));
        }
    }

    /// When owners last used each workspace, for estimates.
    fn activity(&self) -> Vec<(PathBuf, SystemTime)> {
        let mut activity = self
            .t3
            .as_ref()
            .map(t3_cleanup::State::activity)
            .unwrap_or_default();
        if let Ok(sessions) = &self.sessions {
            activity.extend(sessions.iter().cloned());
        }
        activity
    }

    /// What a saved WIP commit notes about its workspace: the T3 threads behind it.
    fn note(&self, path: &Path) -> String {
        self.t3
            .as_ref()
            .map(|state| state.thread_links(path, THREAD_LINKS))
            .filter(|links| !links.is_empty())
            .map_or_else(String::new, |links| {
                format!("T3 threads: {}", links.join(", "))
            })
    }

    /// The T3 threads behind each blocked workspace that has any.
    fn thread_links(&self, blocked: &[Blocked]) -> BTreeMap<PathBuf, Vec<String>> {
        let Some(state) = &self.t3 else {
            return BTreeMap::new();
        };
        blocked
            .iter()
            .map(|entry| {
                (
                    entry.path.clone(),
                    state.thread_links(&entry.path, THREAD_LINKS),
                )
            })
            .filter(|(_, links)| !links.is_empty())
            .collect()
    }
}

#[derive(clap::Subcommand)]
pub enum Command {
    /// List each workspace's estimated cleanup time or the reason it stays.
    ///
    /// This is the default when no subcommand is given. The listing shows
    /// what the hourly timer would do under the standard retention policy.
    /// It does not remove any workspaces. For a different idle threshold, use
    /// `sweep clean AGE --dry-run`.
    ///
    /// JSON states:
    ///   due      Eligible now.
    ///   later    Not idle long enough yet.
    ///   held     Not idle long enough yet, with an issue that would block cleanup.
    ///   blocked  Idle long enough, but needs attention.
    ///   waiting  Kept by an active owner, a lock, or a hold.
    ///   error    Could not check the workspace.
    #[command(verbatim_doc_comment)]
    Status {
        /// Emit a JSON array instead of the human-readable table.
        ///
        /// Paths are relative to HOME. Sizes are bytes and due times are
        /// estimated RFC 3339 timestamps. Unavailable values are null.
        /// Check stderr for discovery warnings; a listing can be incomplete.
        #[arg(long)]
        json: bool,
    },
    /// Preview or remove eligible idle workspaces.
    ///
    /// With no AGE, use the same retention as the hourly timer. Supplying AGE
    /// changes the idle threshold for this run only. Cleanup still respects
    /// active use, Git worktree locks, and `sweep keep` holds.
    ///
    /// Sweep looks at workspace directory timestamps, Git activity, and agent
    /// session activity. Editing a file in place may not reset the idle timer.
    ///
    /// Preview first with `sweep clean 1h --dry-run`. Otherwise, sweep shows
    /// what it plans to remove and asks before proceeding, unless you pass
    /// --yes. Sweep rechecks workspaces before removal.
    ///
    /// Git branches do not have to be merged. Sweep saves recoverable Git work
    /// locally before removing a workspace, or leaves the workspace alone if it
    /// cannot save that work safely. A linked worktree's root `.scratch/` is
    /// disposable, including any Git repositories inside it. Plain scratch
    /// files and Git-ignored files are not backed up either.
    #[command(verbatim_doc_comment)]
    Clean {
        /// Minimum idle time, such as 30m, 2d, 1w or 1d12h. Use `today` for
        /// workspaces idle since before local midnight.
        age: Option<String>,
        /// Remove the eligible workspaces without prompting.
        #[arg(short, long)]
        yes: bool,
        /// Show what would go without removing anything.
        #[arg(short = 'n', long)]
        dry_run: bool,
    },
    /// Hold a workspace in place, or list current holds.
    ///
    /// PATH must exist and be a managed workspace or a file or directory inside
    /// one. A symlink hold protects the link, not its target. Holding a file or
    /// subdirectory protects the whole workspace from both `sweep clean` and
    /// hourly cleanup. Nothing is copied or moved.
    ///
    /// Run `sweep keep` with no PATH to list the held paths and their expiry.
    /// A hold lasts until `sweep unkeep PATH` unless you pass --for. Running
    /// `sweep keep` again for the same path replaces that hold. When a hold
    /// expires, the next cleanup may remove the workspace if it is eligible.
    ///
    /// Agents should use this only when the user explicitly asks to keep an
    /// item. Otherwise, leave temporary artifacts in scratch.
    #[command(
        verbatim_doc_comment,
        after_long_help = "\
Examples:
  sweep keep dev/scratch/my-task/report.md --for 2d
  sweep unkeep dev/scratch/my-task/report.md"
    )]
    Keep {
        /// Path to keep. Relative paths resolve from the current directory first,
        /// then from HOME, as printed by `sweep status`.
        path: Option<PathBuf>,
        /// Time before the hold expires, such as 4h, 2d or 1w.
        #[arg(long = "for", value_name = "DURATION", requires = "path")]
        duration: Option<String>,
    },
    /// Release a hold created by `sweep keep`.
    ///
    /// This does not delete files or reset the idle timer. The next cleanup may
    /// remove the workspace if it is otherwise eligible. Releasing a hold on
    /// a workspace does not release separate holds on the files inside it.
    /// This command does not unlock Git worktrees.
    #[command(verbatim_doc_comment)]
    Unkeep {
        /// Use the original path listed by `sweep keep`, even if that file or
        /// directory no longer exists. Relative paths can be based at HOME or
        /// the current directory.
        path: PathBuf,
    },
    /// List recoverable Git work saved during cleanup.
    ///
    /// Lists local Git branches and bundles saved by sweep, with commands to
    /// recover them. It also drops expired copies after 30 days. This command
    /// cannot recover ignored files or ordinary scratch artifacts.
    #[command(verbatim_doc_comment)]
    Wip,
    /// Run the same cleanup as the hourly timer, without prompting.
    ///
    /// Removes workspaces eligible under the standard retention policy and
    /// reports new blockers. To preview the same policy without removing
    /// anything, run `sweep clean --dry-run` instead.
    #[command(verbatim_doc_comment)]
    Auto,
    /// Show one attention notification and run its buttons; `auto` starts it.
    #[command(hide = true)]
    Notify { request: PathBuf },
}

/// A duration such as `30m`, `1h`, `2d`, `1w` or `1d12h`.
pub(crate) fn parse_duration(text: &str) -> Result<Duration> {
    let text = text.trim().to_ascii_lowercase();
    let mut total = 0u64;
    let mut rest = text.as_str();
    ensure!(!rest.is_empty(), "empty duration");
    while !rest.is_empty() {
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        ensure!(
            digits > 0,
            "{text:?} is not a duration such as 30m, 1h, 2d or 1w"
        );
        let count: u64 = rest[..digits].parse()?;
        rest = rest[digits..].trim_start();
        let letters = rest.len()
            - rest
                .trim_start_matches(|c: char| c.is_ascii_alphabetic())
                .len();
        let seconds = match &rest[..letters] {
            "" => bail!("{text:?} needs a unit: {count}m, {count}h or {count}d"),
            "s" | "sec" | "secs" | "second" | "seconds" => 1,
            "m" | "min" | "mins" | "minute" | "minutes" => 60,
            "h" | "hr" | "hrs" | "hour" | "hours" => 3_600,
            "d" | "day" | "days" => 86_400,
            "w" | "week" | "weeks" => 604_800,
            unit => bail!("unknown unit {unit:?} in {text:?}; use m, h, d or w"),
        };
        total = count
            .checked_mul(seconds)
            .and_then(|part| total.checked_add(part))
            .context("duration too long")?;
        rest = rest[letters..].trim_start();
    }
    Ok(Duration::from_secs(total))
}

/// The cutoff for `sweep clean AGE`.
fn parse_age(text: &str, now: SystemTime) -> Result<SystemTime> {
    if text.trim().eq_ignore_ascii_case("today") {
        let midnight = Local::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .and_then(|time| time.and_local_timezone(Local).earliest())
            .context("no local midnight today")?;
        return Ok(midnight.into());
    }
    now.checked_sub(parse_duration(text)?)
        .context("duration reaches before 1970")
}

fn human_size(bytes: u64) -> String {
    let units = ["B", "K", "M", "G", "T"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < units.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes}B")
    } else {
        format!("{value:.1}{}", units[unit])
    }
}

fn human_span(span: Duration) -> String {
    let minutes = span.as_secs().div_ceil(60);
    let (days, hours, minutes) = (minutes / 1_440, minutes / 60 % 24, minutes % 60);
    match (days, hours) {
        (0, 0) => format!("{minutes}m"),
        (0, _) => format!("{hours}h {minutes:02}m"),
        _ => format!("{days}d {hours}h"),
    }
}

/// The hourly timer's first run at or after `due`.
fn next_run(due: SystemTime) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(unix(due).div_ceil(3_600) * 3_600)
}

fn when_label(row: &Row, now: SystemTime) -> String {
    match &row.when {
        When::Now => "next run".into(),
        When::Later { blocker } => {
            let span = row
                .due
                .map(|due| next_run(due).duration_since(now).unwrap_or_default())
                .filter(|span| !span.is_zero());
            match (blocker, span) {
                (None, Some(span)) => format!("in {}", human_span(span)),
                (None, None) => "next run".into(),
                (Some(_), Some(span)) => format!("held in {}", human_span(span)),
                (Some(_), None) => "blocked".into(),
            }
        }
        When::Blocked(_) => "blocked".into(),
        When::Waiting(_) => "waiting".into(),
        When::Failed(_) => "error".into(),
    }
}

/// Why a row stays, or what will hold it once due, with `home/` stripped.
fn row_reason(row: &Row, home: &Path) -> Option<String> {
    let reason = match &row.when {
        When::Later {
            blocker: Some(reason),
        }
        | When::Blocked(reason)
        | When::Waiting(reason)
        | When::Failed(reason) => reason,
        _ => return None,
    };
    Some(reason.replace(&format!("{}/", home.display()), ""))
}

/// The rows as JSON for `sweep status --json`: sizes in bytes, `due` as the
/// RFC 3339 time of the hourly run that removes it, when there is one.
fn json_rows(rows: &[&Row], home: &Path, now: SystemTime) -> Value {
    let rows = rows
        .iter()
        .map(|row| {
            let state = match &row.when {
                When::Now => "due",
                When::Later { blocker: None } => "later",
                When::Later { blocker: Some(_) } => "held",
                When::Blocked(_) => "blocked",
                When::Waiting(_) => "waiting",
                When::Failed(_) => "error",
            };
            let due = match &row.when {
                When::Now => Some(next_run(now)),
                When::Later { blocker: None } => {
                    row.due.map(|due| next_run(due).max(next_run(now)))
                }
                _ => None,
            };
            json!({
                "state": state,
                "due": due.map(|due| DateTime::<Local>::from(due).to_rfc3339()),
                "size": row.size,
                "kind": row.kind.label(),
                "branch": row.branch,
                "path": relative(&row.path, home),
                "reason": row_reason(row, home),
                "saves": row.plan,
            })
        })
        .collect();
    Value::Array(rows)
}

fn print_rows(rows: &[&Row], home: &Path, now: SystemTime) {
    let branches: Vec<String> = rows
        .iter()
        .map(|row| row.branch.clone().unwrap_or_else(|| "-".into()))
        .collect();
    let width = branches
        .iter()
        .map(|branch| branch.chars().count())
        .max()
        .unwrap_or(0)
        .max("BRANCH".len());
    println!(
        "{:<16} {:>7}  {:<18} {:<width$}  PATH (from ~)",
        "GOES", "SIZE", "KIND", "BRANCH"
    );
    for (row, branch) in rows.iter().zip(&branches) {
        let size = row.size.map_or("-".into(), human_size);
        println!(
            "{:<16} {size:>7}  {:<18} {branch:<width$}  {}",
            when_label(row, now),
            row.kind.label(),
            relative(&row.path, home)
        );
        let prefix = match &row.when {
            When::Later { .. } => "then needs attention: ",
            When::Blocked(_) => "needs attention: ",
            _ => "",
        };
        if let Some(reason) = row_reason(row, home) {
            println!("{:<45}↳ {prefix}{reason}", "");
        } else if let Some(plan) = &row.plan {
            println!("{:<45}↳ {plan}", "");
        }
    }
}

/// Order: due now, then by due time, then needing attention, then waiting.
fn sorted(rows: &[Row]) -> Vec<&Row> {
    let mut rows: Vec<_> = rows.iter().collect();
    rows.sort_by_key(|row| {
        let rank = match row.when {
            When::Now => 0,
            When::Later { .. } => 1,
            When::Blocked(_) => 2,
            When::Waiting(_) => 3,
            When::Failed(_) => 4,
        };
        (rank, row.due, row.path.clone())
    });
    rows
}

fn confirm(question: &str) -> Result<bool> {
    ensure!(
        std::io::stdin().is_terminal(),
        "not asking without a terminal; pass --yes"
    );
    print!("{question} [y/N] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

/// Classify every candidate under `policy`, sizes and estimates included.
fn survey(home: &Path, policy: Policy, keeps: &Keeps) -> Result<Vec<Row>> {
    let now = SystemTime::now();
    // Checks, estimates and sizes share one scan per workspace.
    let _scans = tree_scan::remember();
    let mut sources = Sources::list(home);
    let cutoff = |kind| policy.cutoff(kind);
    let held = keeps.holds(&sources.candidates, now);
    let first = sources.first_pass(&cutoff, &held);
    let checked: Vec<_> = sources.candidates.iter().zip(&first).collect();
    let whens = tree_scan::parallel(&checked, |(candidate, decision)| {
        classify(
            &sources,
            candidate,
            decision,
            held(&candidate.path).is_some(),
            now,
        )
    });
    let mut rows: Vec<Row> = sources
        .candidates
        .iter()
        .zip(whens)
        .map(|(candidate, (when, plan))| Row {
            kind: candidate.kind,
            path: candidate.path.clone(),
            device: candidate.device,
            when,
            size: None,
            due: None,
            branch: branch(&candidate.path),
            plan,
        })
        .collect();
    measure(
        &mut rows,
        policy,
        now,
        &sources.activity(),
        &state_dir(home),
    );
    for failure in &sources.failures {
        eprintln!("warning: {failure}");
    }
    Ok(rows)
}

fn status(home: &Path, json: bool) -> Result<()> {
    let now = SystemTime::now();
    let keeps = Keeps::load(&state_dir(home))?;
    let rows = survey(home, Policy::standard(now)?, &keeps)?;
    let rows = sorted(&rows);
    if json {
        println!("{}", serde_json::to_string(&json_rows(&rows, home, now))?);
        return Ok(());
    }
    print_rows(&rows, home, now);
    let total = |within: Option<Duration>| {
        let chosen: Vec<_> = rows
            .iter()
            .filter(|row| match (&row.when, within) {
                (When::Now, _) => true,
                (When::Later { blocker: None }, Some(within)) => {
                    row.due.is_some_and(|due| next_run(due) <= now + within)
                }
                _ => false,
            })
            .collect();
        let bytes: u64 = chosen.iter().filter_map(|row| row.size).sum();
        format!("{} ({})", chosen.len(), human_size(bytes))
    };
    let count = |test: fn(&When) -> bool| rows.iter().filter(|row| test(&row.when)).count();
    println!();
    println!(
        "Goes at the next hourly run: {}. Within a day: {}, 3 days: {}, a week: {}.",
        total(None),
        total(Some(Duration::from_secs(86_400))),
        total(Some(Duration::from_secs(3 * 86_400))),
        total(Some(Duration::from_secs(7 * 86_400))),
    );
    println!(
        "Needs attention: {}. Waiting on an owner, a lock or `sweep keep`: {}.",
        count(|when| matches!(when, When::Blocked(_) | When::Later { blocker: Some(_) })),
        count(|when| matches!(when, When::Waiting(_))),
    );
    println!(
        "Retention: {AGENT_RETENTION_DAYS}d idle for agent workspaces, {DEV_RETENTION_DAYS}d for ~/dev worktrees. \
         Preview with `sweep clean --dry-run`; use `sweep keep PATH` to hold the workspace containing PATH."
    );
    Ok(())
}

fn clean(home: &Path, age: Option<&str>, yes: bool, dry_run: bool) -> Result<()> {
    let now = SystemTime::now();
    let policy = match age {
        Some(age) => Policy::uniform(parse_age(age, now)?),
        None => Policy::standard(now)?,
    };
    let dir = state_dir(home);
    let _lock = lock(&dir)?;
    let keeps = Keeps::load(&dir)?;
    let rows = survey(home, policy, &keeps)?;
    let due: Vec<_> = sorted(&rows)
        .into_iter()
        .filter(|row| matches!(row.when, When::Now))
        .collect();
    if due.is_empty() {
        println!("Nothing idle that long can go.");
        return Ok(());
    }
    print_rows(&due, home, now);
    let bytes: u64 = due.iter().filter_map(|row| row.size).sum();
    let question = format!(
        "Remove these {} workspaces ({})?",
        due.len(),
        human_size(bytes)
    );
    if dry_run {
        println!("\n{question} Not removing: dry run.");
        return Ok(());
    }
    if !yes && !confirm(&format!("\n{question}"))? {
        println!("Nothing removed.");
        return Ok(());
    }
    let paths = due.iter().map(|row| row.path.clone()).collect();
    // Remove exactly these, each rechecked under the same policy.
    let mut blocked = Vec::new();
    let (_, result) = cleanup(home, policy, &keeps, Some(&paths), false, &mut blocked);
    for entry in &blocked {
        println!("Kept {}: {}", entry.path.display(), entry.reason);
    }
    result
}

fn keep(home: &Path, path: Option<&Path>, duration: Option<&str>) -> Result<()> {
    let dir = state_dir(home);
    let _lock = lock(&dir)?;
    let mut keeps = Keeps::load(&dir)?;
    let Some(path) = path else {
        keeps.drop_expired(SystemTime::now());
        if keeps.0.is_empty() {
            println!("Nothing is kept. `git worktree lock` also keeps a worktree.");
        }
        for (path, until) in &keeps.0 {
            let until = until.map_or("until `sweep unkeep`".into(), |until| {
                format!("until {}", local(until))
            });
            println!("{}  {until}", relative(path, home));
        }
        return Ok(());
    };
    let path = input_paths(home, path)
        .filter_map(|path| entry_path(&path).ok())
        .find(|path| path.symlink_metadata().is_ok())
        .with_context(|| format!("find {}", path.display()))?;
    let (candidates, failures) = discover(home);
    let Some(root) = protection_root(&path, &candidates) else {
        ensure!(
            failures.is_empty(),
            "could not check where to hold {}:\n{}",
            path.display(),
            failures.join("\n")
        );
        bail!("{} is not in a workspace managed by sweep", path.display());
    };
    let now = SystemTime::now();
    let until = duration
        .map(|text| {
            let span = parse_duration(text)?;
            ensure!(!span.is_zero(), "hold duration must be positive");
            unix(now)
                .checked_add(span.as_secs())
                .filter(|&until| until <= DateTime::<chrono::Utc>::MAX_UTC.timestamp() as u64)
                .context("hold duration too long")
        })
        .transpose()?;
    keeps.0.insert(path.clone(), until);
    keeps.save(&dir, now)?;
    match until {
        Some(until) => println!("Keeping {} until {}.", relative(&path, home), local(until)),
        None => println!("Keeping {} until `sweep unkeep`.", relative(&path, home)),
    }
    if root != path {
        println!("Holding workspace {}.", relative(root, home));
    }
    Ok(())
}

fn wip(home: &Path) -> Result<()> {
    let now = salvage::now();
    let file = state_dir(home).join("saved.json");
    let saved: Vec<Saved> = load_json(&file)?;
    let saved = salvage::expire(saved, now);
    if saved.is_empty() {
        println!("No saved work. Removals save uncommitted changes here first.");
        return Ok(());
    }
    for entry in &saved {
        let age = Duration::from_secs(now.saturating_sub(entry.at));
        let left =
            Duration::from_secs((entry.at + salvage::KEEP_SAVED_DAYS * 86_400).saturating_sub(now));
        println!(
            "{}  (saved {} ago, dropped in {})",
            relative(&entry.workspace, home),
            human_span(age),
            human_span(left)
        );
        match &entry.place {
            Place::Branch { repo, branch, .. } => {
                let checkout = repo.parent().unwrap_or(repo);
                println!("    git -C {} switch {branch}", relative(checkout, home));
            }
            Place::Bundle { file } => println!("    git clone {}", file.display()),
        }
    }
    store_json(&file, &saved)
}

fn unkeep(home: &Path, path: &Path) -> Result<()> {
    let dir = state_dir(home);
    let _lock = lock(&dir)?;
    let mut keeps = Keeps::load(&dir)?;
    // A deleted file may still have a hold. Accept paths from the current
    // directory and the paths relative to HOME that `sweep keep` lists.
    let path = input_paths(home, path)
        .find_map(|path| {
            entry_path(&path)
                .ok()
                .into_iter()
                .chain(std::path::absolute(path).ok())
                .find(|path| keeps.0.remove(path).is_some())
        })
        .with_context(|| format!("{} is not kept with `sweep keep`", path.display()))?;
    keeps.save(&dir, SystemTime::now())?;
    println!("Removed the hold on {}.", relative(&path, home));
    Ok(())
}

pub fn run(command: Option<Command>) -> Result<()> {
    let home = fs::canonicalize(std::env::var_os("HOME").context("HOME is unset")?)?;
    match command.unwrap_or(Command::Status { json: false }) {
        Command::Status { json } => status(&home, json),
        Command::Clean { age, yes, dry_run } => clean(&home, age.as_deref(), yes, dry_run),
        Command::Keep { path, duration } => keep(&home, path.as_deref(), duration.as_deref()),
        Command::Unkeep { path } => unkeep(&home, &path),
        Command::Wip => wip(&home),
        Command::Auto => auto(&home),
        Command::Notify { request } => attention::wait(&request),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_need_units() -> Result<()> {
        assert_eq!(parse_duration("30m")?, Duration::from_secs(1_800));
        assert_eq!(parse_duration("1h")?, Duration::from_secs(3_600));
        assert_eq!(parse_duration("1d12h")?, Duration::from_secs(129_600));
        assert_eq!(parse_duration("2 weeks")?, Duration::from_secs(1_209_600));
        for text in ["40", "", "h", "3x", "-1d"] {
            assert!(parse_duration(text).is_err(), "accepted {text:?}");
        }
        let now = SystemTime::now();
        assert!(parse_age("today", now)? <= now);
        Ok(())
    }

    #[test]
    fn keeps_expire() {
        let now = SystemTime::now();
        let mut keeps = Keeps::default();
        keeps.0.insert("/forever".into(), None);
        keeps.0.insert("/later".into(), Some(unix(now) + 3_600));
        keeps.0.insert("/expired".into(), Some(unix(now) - 1));
        let held = keeps.holds(&[], now);
        assert!(held(Path::new("/forever")).is_some());
        assert!(held(Path::new("/later")).unwrap().contains("until"));
        assert!(held(Path::new("/expired")).is_none());
        assert!(held(Path::new("/other")).is_none());
    }

    #[test]
    fn file_hold_protects_nested_removal_units() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let outer = temp.path().canonicalize()?.join("outer");
        let nested = outer.join("nested");
        fs::create_dir_all(&nested)?;
        let mut candidates = worktree_prune::folder_candidates(Kind::TaskScratch, temp.path(), 1)?;
        candidates.extend(worktree_prune::folder_candidates(
            Kind::TaskScratch,
            &outer,
            1,
        )?);
        let keeps = Keeps(BTreeMap::from([(outer.join("report.txt"), None)]));
        let held = keeps.holds(&candidates, SystemTime::now());
        assert!(held(&outer).is_some());
        assert!(held(&nested).is_some());
        assert!(held(&outer.with_file_name("outer-other")).is_none());
        Ok(())
    }

    #[test]
    fn estimates_round_to_the_hourly_run() {
        let due = UNIX_EPOCH + Duration::from_secs(10 * 3_600 + 1);
        assert_eq!(unix(next_run(due)), 11 * 3_600);
        assert_eq!(human_span(Duration::from_secs(26 * 3_600)), "1d 2h");
        assert_eq!(human_span(Duration::from_secs(90 * 60)), "1h 30m");
        assert_eq!(human_size(1_536), "1.5K");
    }
}
