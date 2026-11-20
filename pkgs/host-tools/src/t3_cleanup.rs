//! T3's workspaces: projectless scratch folders, worktrees, and the thread
//! state that guards every workspace removal.
//!
//! T3's own worktree cleanup stays disabled; this timer owns it instead. T3
//! still prunes its own logs and browser artifacts. T3 offers no workspace
//! lease, so each removal re-reads T3 state right before deleting. Thread data
//! in a shape this code does not know fails the run instead of being guessed at.
use crate::{
    t3_settings::Session,
    util,
    worktree_prune::{self, Fingerprint, keep, keep_unless},
};
use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

/// Thread timestamps that count as activity. A thread resumed or touched
/// within the retention period keeps its workspace.
const ACTIVITY: &[&str] = &[
    "settledAt",
    "unsettledAt",
    "createdAt",
    "latestUserMessageAt",
    "latestRunRequestedAt",
    "latestRunStartedAt",
    "latestRunCompletedAt",
];
const IDLE: &[&str] = &[
    "idle",
    "completed",
    "interrupted",
    "failed",
    "cancelled",
    "rolled_back",
];
const BUSY: &[&str] = &["preparing", "queued", "starting", "running", "waiting"];

#[derive(Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CatalogEntry {
    Thread { id: String, path: PathBuf },
    Schedule { id: String },
    Project { path: PathBuf },
    Session { path: PathBuf },
}

/// The HTTP shell omits some threads, such as archived ones in some T3
/// versions, so the database catalog lists every binding the shell must vouch for.
const CATALOG: &str = "
    SELECT 'thread' AS kind, thread_id AS id, json_extract(payload_json, '$.worktreePath') AS path
    FROM orchestration_v2_projection_threads
    WHERE deleted_at IS NULL AND json_extract(payload_json, '$.worktreePath') IS NOT NULL
    UNION ALL SELECT 'schedule', thread_id, NULL
    FROM scheduled_tasks WHERE enabled = 1 AND thread_id IS NOT NULL
    UNION ALL SELECT 'project', NULL, workspace_root
    FROM projection_projects WHERE deleted_at IS NULL
    UNION ALL SELECT 'session', NULL, json_extract(payload_json, '$.cwd')
    FROM orchestration_v2_projection_provider_sessions
    WHERE status != 'stopped' AND json_extract(payload_json, '$.cwd') IS NOT NULL";

/// The catalog, or None when T3 has no state database yet.
fn read_catalog(database: &Path) -> Result<Option<Vec<CatalogEntry>>> {
    match fs::symlink_metadata(database) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("Inspect T3 state database"),
    }
    let output = util::command_output(
        Command::new("sqlite3")
            .args(["-readonly", "-json", "-cmd", ".timeout 3000"])
            .arg(database)
            .arg(CATALOG),
    )
    .context("Read T3 workspace catalog")?;
    // sqlite3 emits no bytes for an empty result set.
    if output.is_empty() {
        return Ok(Some(Vec::new()));
    }
    serde_json::from_slice(&output)
        .map(Some)
        .context("Parse T3 workspace catalog")
}

struct Binding {
    id: String,
    raw: PathBuf,
    resolved: PathBuf,
}

/// T3's view of which workspaces are in use.
pub struct State {
    base: PathBuf,
    scratch: PathBuf,
    bindings: Vec<Binding>,
    scheduled: BTreeSet<String>,
    projects: Vec<PathBuf>,
    sessions: Vec<PathBuf>,
    /// Thread shells by ID, or None while no T3 server runs.
    threads: Option<BTreeMap<String, Value>>,
    /// Without its database T3's bindings are unknown, so nothing is removable.
    cataloged: bool,
    revoking: Option<std::thread::JoinHandle<()>>,
}

impl Drop for State {
    fn drop(&mut self) {
        if let Some(revoking) = self.revoking.take() {
            let _ = revoking.join();
        }
    }
}

impl State {
    pub fn read(base: &Path) -> Result<Self> {
        // Read the shell first: a binding created in between then shows up only
        // in the catalog, which keeps its workspace.
        let (shell, revoking) = match Session::open_for_cleanup(base)? {
            Some(session) => {
                let shell = session.shell();
                // Revoking the short-lived session is another CLI round trip;
                // it finishes alongside the checks, and dropping the state waits.
                (
                    Some(shell?),
                    Some(std::thread::spawn(move || drop(session))),
                )
            }
            None => (None, None),
        };
        let catalog = read_catalog(&base.join("userdata/statev2.sqlite"))?;
        let mut state = Self::new(base, catalog, shell)?;
        state.revoking = revoking;
        Ok(state)
    }

    fn new(base: &Path, catalog: Option<Vec<CatalogEntry>>, shell: Option<Value>) -> Result<Self> {
        let mut state = Self {
            base: base.to_owned(),
            scratch: resolve(&base.join("scratch"))?,
            bindings: Vec::new(),
            scheduled: BTreeSet::new(),
            projects: Vec::new(),
            sessions: Vec::new(),
            threads: None,
            cataloged: catalog.is_some(),
            revoking: None,
        };
        let catalog = catalog.unwrap_or_default();
        let mut bound = BTreeSet::new();
        if let Some(mut shell) = shell {
            ensure!(shell["schemaVersion"] == 2, "Unsupported T3 shell version");
            let mut threads = BTreeMap::new();
            for key in ["threads", "archivedThreads"] {
                let Value::Array(list) = shell[key].take() else {
                    bail!("T3 shell has no {key} list");
                };
                for thread in list {
                    let id = thread["id"]
                        .as_str()
                        .context("T3 thread without an ID")?
                        .to_owned();
                    match thread.get("worktreePath") {
                        Some(Value::Null) => {}
                        Some(Value::String(path)) => {
                            bound.insert((id.clone(), PathBuf::from(path)));
                        }
                        _ => bail!("T3 thread {id} has an invalid workspace path"),
                    }
                    ensure!(
                        threads.insert(id.clone(), thread).is_none(),
                        "T3 shell lists thread {id} twice"
                    );
                }
            }
            state.threads = Some(threads);
        }
        for entry in catalog {
            match entry {
                CatalogEntry::Thread { id, path } => {
                    bound.insert((id, path));
                }
                CatalogEntry::Schedule { id } => {
                    state.scheduled.insert(id);
                }
                CatalogEntry::Project { path } => {
                    let resolved = resolve(&path)?;
                    // T3 registers the scratch root itself as the "No project" project.
                    if resolved != state.scratch {
                        state.projects.extend([path, resolved]);
                    }
                }
                CatalogEntry::Session { path } => {
                    let resolved = resolve(&path)?;
                    state.sessions.extend([path, resolved]);
                }
            }
        }
        for (id, raw) in bound {
            let resolved = resolve(&raw)?;
            state.bindings.push(Binding { id, raw, resolved });
        }
        Ok(state)
    }

    /// Re-read T3 right before a removal.
    pub fn refresh(&mut self) -> Result<()> {
        *self = Self::read(&self.base)?;
        Ok(())
    }

    /// Keep `folder` while T3 needs it: a project or open provider session in
    /// it, or a thread bound at or inside it (or above it, for scratch) that is
    /// not settled and idle since `cutoff`. Projectless scratch must belong to
    /// such threads.
    pub fn check(&self, folder: &Path, cutoff: DateTime<Utc>, scratch: bool) -> Result<()> {
        let overlaps =
            |path: &Path| path.starts_with(folder) || (scratch && folder.starts_with(path));
        keep_unless!(self.cataloged, "T3 has no state database to check");
        keep_unless!(
            !self.projects.iter().any(|path| overlaps(path)),
            "overlaps a registered T3 project"
        );
        keep_unless!(
            !self.sessions.iter().any(|path| path.starts_with(folder)),
            "an open T3 provider session runs in it"
        );
        let bindings: Vec<_> = self
            .bindings
            .iter()
            .filter(|binding| overlaps(&binding.raw) || overlaps(&binding.resolved))
            .collect();
        if bindings.is_empty() {
            keep_unless!(!scratch, "no T3 thread uses it");
            return Ok(());
        }
        let Some(threads) = &self.threads else {
            return Err(keep("T3 is not running, so its threads cannot be checked"));
        };
        for Binding { id, raw, resolved } in bindings {
            keep_unless!(
                raw == folder && resolved == folder,
                "thread {id} uses a path inside or above it"
            );
            let thread = threads
                .get(id)
                .ok_or_else(|| keep(format!("thread {id} is missing from the T3 shell")))?;
            keep_unless!(
                thread["worktreePath"].as_str().map(Path::new) == Some(raw.as_path()),
                "thread {id} moved while T3 state was read"
            );
            if scratch {
                keep_unless!(
                    field(thread, "branch")?.is_null(),
                    "thread {id} has a branch"
                );
            }
            keep_unless!(
                !self.scheduled.contains(id),
                "an enabled T3 schedule uses thread {id}"
            );
            check_thread(threads, thread, cutoff).with_context(|| format!("thread {id}"))?;
        }
        Ok(())
    }
}

fn resolve(path: &Path) -> Result<PathBuf> {
    ensure!(path.is_absolute(), "Non-absolute T3 workspace path");
    // Keep an absent descendant associated with its nearest existing ancestor.
    for ancestor in path.ancestors() {
        match ancestor.canonicalize() {
            Ok(canonical) => return Ok(canonical.join(path.strip_prefix(ancestor)?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error).context("Resolve T3 workspace path"),
        }
    }
    bail!("Could not resolve T3 workspace path")
}

fn field<'a>(thread: &'a Value, key: &str) -> Result<&'a Value> {
    thread.get(key).with_context(|| format!("missing {key}"))
}

fn timestamp(thread: &Value, key: &str) -> Result<Option<DateTime<Utc>>> {
    match field(thread, key)? {
        Value::Null => Ok(None),
        Value::String(time) => Ok(Some(
            DateTime::parse_from_rfc3339(time)
                .with_context(|| format!("invalid {key}"))?
                .with_timezone(&Utc),
        )),
        _ => bail!("invalid {key}"),
    }
}

/// Whether the thread runs, waits on something, or may still resume on its own.
fn busy(thread: &Value, cutoff: DateTime<Utc>) -> Result<bool> {
    let status = field(thread, "status")?
        .as_str()
        .context("invalid status")?;
    let idle = if IDLE.contains(&status) {
        true
    } else if BUSY.contains(&status) {
        false
    } else {
        bail!("unknown status {status:?}");
    };
    for key in ["activeRunId", "activityRunStatus", "pendingRuntimeRequest"] {
        if !field(thread, key)?.is_null() {
            return Ok(true);
        }
    }
    let tasks = field(thread, "pendingBackgroundTasks")?
        .as_array()
        .context("invalid pendingBackgroundTasks")?;
    let guests = field(thread, "pendingGuestMessageCount")?
        .as_u64()
        .context("invalid pendingGuestMessageCount")?;
    // A run stopped by a usage limit may resume once the limit resets.
    let limited = field(thread, "lastErrorClass")?.as_str() == Some("usage_limit")
        && field(thread, "settledOverride")?.as_str() != Some("settled")
        && timestamp(thread, "usageLimitResetAt")?.is_some_and(|reset| reset >= cutoff);
    Ok(!idle || limited || !tasks.is_empty() || guests > 0)
}

/// When the thread settled. Like T3, a subagent inherits the settlement of an
/// idle parent in the same project and thread tree.
fn settled_at(
    threads: &BTreeMap<String, Value>,
    thread: &Value,
    cutoff: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>> {
    let mut current = thread;
    let mut seen = BTreeSet::new();
    loop {
        if let Some(settled) = timestamp(current, "settledAt")? {
            let resumed = timestamp(current, "unsettledAt")?.is_some_and(|time| time >= settled);
            return Ok((!resumed).then_some(settled));
        }
        let lineage = field(current, "lineage")?;
        if lineage["relationshipToParent"].as_str() != Some("subagent")
            || field(current, "settledOverride")?.as_str() == Some("active")
            || !field(current, "pinnedAt")?.is_null()
        {
            return Ok(None);
        }
        let Some((id, parent)) = lineage["parentThreadId"]
            .as_str()
            .and_then(|id| threads.get_key_value(id))
        else {
            return Ok(None);
        };
        if !seen.insert(id)
            || parent["projectId"] != thread["projectId"]
            || parent["lineage"]["rootThreadId"] != thread["lineage"]["rootThreadId"]
            || busy(parent, cutoff)?
        {
            return Ok(None);
        }
        current = parent;
    }
}

fn check_thread(
    threads: &BTreeMap<String, Value>,
    thread: &Value,
    cutoff: DateTime<Utc>,
) -> Result<()> {
    keep_unless!(!busy(thread, cutoff)?, "is running or has pending work");
    keep_unless!(field(thread, "pinnedAt")?.is_null(), "is pinned");
    match field(thread, "settledOverride")? {
        Value::Null => {}
        Value::String(value) if value == "settled" => {}
        Value::String(value) if value == "active" => return Err(keep("is marked active")),
        other => bail!("unknown settledOverride {other}"),
    }
    keep_unless!(field(thread, "deletedAt")?.is_null(), "is being deleted");
    // A snoozed thread comes back on its own when the snooze ends.
    keep_unless!(
        timestamp(thread, "snoozedUntil")?.is_none_or(|until| until < cutoff),
        "is snoozed"
    );
    let settled = settled_at(threads, thread, cutoff)?.ok_or_else(|| keep("is not settled"))?;
    keep_unless!(settled < cutoff, "settled within the retention period");
    for &key in ACTIVITY {
        let time = timestamp(thread, key)?;
        ensure!(
            key != "createdAt" || time.is_some(),
            "missing creation time"
        );
        keep_unless!(time.is_none_or(|time| time < cutoff), "has a recent {key}");
    }
    Ok(())
}

impl State {
    /// A projectless scratch folder goes once every T3 thread using it
    /// settled before the cutoff and nothing touched it since.
    pub fn check_scratch(
        &self,
        folder: &Path,
        device: u64,
        cutoff: SystemTime,
    ) -> Result<Fingerprint> {
        // Most folders are recent; skip scanning them.
        keep_unless!(
            folder.symlink_metadata()?.modified()? < cutoff,
            "changed recently"
        );
        self.check(folder, cutoff.into(), true)?;
        worktree_prune::inspect_tree(folder, device, cutoff, false)
    }
}

impl State {
    /// Newest activity of every thread binding, for estimating when a
    /// workspace becomes idle. Unparseable timestamps are skipped.
    pub fn activity(&self) -> Vec<(PathBuf, SystemTime)> {
        let Some(threads) = &self.threads else {
            return Vec::new();
        };
        let mut activity = Vec::new();
        for binding in &self.bindings {
            let Some(thread) = threads.get(&binding.id) else {
                continue;
            };
            let newest = ACTIVITY
                .iter()
                .filter_map(|key| timestamp(thread, key).ok().flatten())
                .max();
            if let Some(newest) = newest {
                for path in [&binding.raw, &binding.resolved] {
                    activity.push((path.clone(), newest.into()));
                }
            }
        }
        activity
    }

    /// Markdown links to the T3 threads that use `path` or a folder inside or
    /// above it: threads first, then subagents, each newest first; at most
    /// `limit`, then how many more there are.
    pub fn thread_links(&self, path: &Path, limit: usize) -> Vec<String> {
        let Some(threads) = &self.threads else {
            return Vec::new();
        };
        let environment = fs::read_to_string(self.base.join("userdata/environment-id"))
            .ok()
            .map(|id| id.trim().to_owned())
            .filter(|id| !id.is_empty());
        let overlaps = |bound: &Path| bound.starts_with(path) || path.starts_with(bound);
        let mut seen = BTreeSet::new();
        let mut found = Vec::new();
        for binding in &self.bindings {
            if !(overlaps(&binding.raw) || overlaps(&binding.resolved))
                || !seen.insert(binding.id.as_str())
            {
                continue;
            }
            let Some(thread) = threads.get(&binding.id) else {
                continue;
            };
            // A subagent runs in its parent's workspace; its parent is the thread to open.
            let subagent = thread["lineage"]["parentThreadId"]
                .as_str()
                .and_then(|parent| threads.get(parent))
                .is_some_and(|parent| parent["worktreePath"] == thread["worktreePath"]);
            let newest = ACTIVITY
                .iter()
                .filter_map(|key| timestamp(thread, key).ok().flatten())
                .max();
            found.push((
                subagent,
                std::cmp::Reverse(newest),
                binding.id.as_str(),
                thread,
            ));
        }
        found.sort_by_key(|entry| (entry.0, entry.1));
        let mut links: Vec<_> = found
            .iter()
            .take(limit)
            .map(|(_, _, id, thread)| {
                let title = thread["title"]
                    .as_str()
                    .unwrap_or("Untitled thread")
                    .replace(['[', ']', '\\', '\r', '\n'], " ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                match &environment {
                    Some(environment) => {
                        let href = format!("t3-thread://v2/{environment}/{id}");
                        if href.contains(['(', ')']) {
                            format!("[{title}](<{href}>)")
                        } else {
                            format!("[{title}]({href})")
                        }
                    }
                    None => format!("{title} (thread {id})"),
                }
            })
            .collect();
        if found.len() > limit {
            links.push(format!("{} more", found.len() - limit));
        }
        links
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worktree_prune::Keep;
    use serde_json::json;
    use std::os::unix::fs::symlink;

    fn thread(path: &Path, id: &str) -> Value {
        json!({
            "id": id, "worktreePath": path, "branch": null, "projectId": "project",
            "lineage": {"parentThreadId": null, "relationshipToParent": null, "rootThreadId": id},
            "status": "completed", "activeRunId": null, "activityRunStatus": null,
            "pendingRuntimeRequest": null, "pendingBackgroundTasks": [],
            "pendingGuestMessageCount": 0, "pinnedAt": null, "settledOverride": null,
            "deletedAt": null, "unsettledAt": null, "lastErrorClass": null,
            "usageLimitResetAt": null, "snoozedUntil": null,
            "createdAt": "2026-01-01T00:00:00Z", "settledAt": "2026-01-02T00:00:00Z",
            "latestUserMessageAt": null, "latestRunRequestedAt": null,
            "latestRunStartedAt": null, "latestRunCompletedAt": null,
        })
    }

    fn shell(threads: Vec<Value>) -> Value {
        json!({"schemaVersion": 2, "threads": threads, "archivedThreads": []})
    }

    fn bound(id: &str, path: &Path) -> CatalogEntry {
        CatalogEntry::Thread {
            id: id.into(),
            path: path.into(),
        }
    }

    fn cutoff() -> DateTime<Utc> {
        "2026-01-05T00:00:00Z".parse().unwrap()
    }

    /// Whether a check kept the folder; any other failure fails the test.
    fn kept(result: Result<()>) -> bool {
        match result {
            Ok(()) => false,
            Err(error) if error.downcast_ref::<Keep>().is_some() => true,
            Err(error) => panic!("unexpected failure: {error:#}"),
        }
    }

    /// A T3 base directory with a scratch root and one folder in it.
    fn fixture() -> Result<(tempfile::TempDir, PathBuf)> {
        let base = tempfile::tempdir()?;
        let folder = fs::canonicalize(base.path())?.join("scratch/folder");
        fs::create_dir_all(&folder)?;
        Ok((base, folder))
    }

    #[test]
    fn thread_links_put_parents_first_and_cap_the_list() -> Result<()> {
        let (base, folder) = fixture()?;
        fs::create_dir_all(base.path().join("userdata"))?;
        fs::write(base.path().join("userdata/environment-id"), "env-1\n")?;
        let mut parent = thread(&folder, "parent");
        parent["title"] = json!("Fix [the] build");
        parent["latestRunCompletedAt"] = json!("2026-01-03T00:00:00Z");
        let mut child = thread(&folder, "thread:subagent:1");
        child["title"] = json!("Child");
        child["latestRunCompletedAt"] = json!("2026-01-04T00:00:00Z");
        child["lineage"]["parentThreadId"] = json!("parent");
        let mut nested = thread(&folder.join("inner"), "nested");
        nested["title"] = json!("Nested");
        let elsewhere = thread(&folder.with_file_name("other"), "other");
        let live = shell(vec![child, parent, nested, elsewhere]);
        let state = State::new(base.path(), Some(vec![]), Some(live))?;
        assert_eq!(
            state.thread_links(&folder, 5),
            [
                "[Fix the build](t3-thread://v2/env-1/parent)",
                "[Nested](t3-thread://v2/env-1/nested)",
                "[Child](t3-thread://v2/env-1/thread:subagent:1)",
            ]
        );
        assert_eq!(state.thread_links(&folder, 1)[1], "2 more");
        Ok(())
    }

    #[test]
    fn every_live_and_archived_binding_must_be_eligible() -> Result<()> {
        let (base, folder) = fixture()?;
        let mut live = json!({
            "schemaVersion": 2,
            "threads": [thread(&folder, "a")],
            "archivedThreads": [thread(&folder, "b")],
        });
        let catalog = vec![bound("a", &folder), bound("b", &folder)];
        let state = State::new(base.path(), Some(catalog.clone()), Some(live.clone()))?;
        assert!(!kept(state.check(&folder, cutoff(), true)));
        let mut scheduled = catalog.clone();
        scheduled.push(CatalogEntry::Schedule { id: "b".into() });
        let state = State::new(base.path(), Some(scheduled), Some(live.clone()))?;
        assert!(kept(state.check(&folder, cutoff(), true)));
        live["archivedThreads"][0]["settledAt"] = Value::Null;
        let state = State::new(base.path(), Some(catalog), Some(live))?;
        assert!(kept(state.check(&folder, cutoff(), true)));
        Ok(())
    }

    #[test]
    fn keeps_unknown_aliased_and_unverifiable_workspaces() -> Result<()> {
        let (base, folder) = fixture()?;
        let alias = folder.with_file_name("alias");
        symlink(&folder, &alias)?;
        let threads = vec![thread(&folder, "a"), thread(&alias, "b")];
        let state = State::new(base.path(), Some(vec![]), Some(shell(threads)))?;
        assert!(kept(state.check(&folder, cutoff(), true)));
        let unknown = folder.with_file_name("unknown");
        assert!(kept(state.check(&unknown, cutoff(), true)));
        // Outside scratch an unbound folder is left to the Git checks.
        assert!(!kept(state.check(&unknown, cutoff(), false)));
        // With T3 stopped, a bound folder cannot be checked.
        let offline = State::new(base.path(), Some(vec![bound("a", &folder)]), None)?;
        assert!(kept(offline.check(&folder, cutoff(), false)));
        // Without T3's database no binding can be ruled out, even with T3 running.
        let missing = State::new(base.path(), None, Some(shell(vec![])))?;
        assert!(kept(missing.check(&unknown, cutoff(), false)));
        assert!(State::new(base.path(), Some(vec![]), Some(json!({"schemaVersion": 2}))).is_err());
        Ok(())
    }

    #[test]
    fn protects_catalog_only_threads_projects_and_sessions() -> Result<()> {
        let (base, folder) = fixture()?;
        let live = shell(vec![thread(&folder, "a")]);
        let future = folder.join("not-created");
        for entry in [
            bound("archived", &folder),
            bound("missing-child", &future),
            CatalogEntry::Project {
                path: future.clone(),
            },
            CatalogEntry::Session {
                path: future.clone(),
            },
        ] {
            let catalog = vec![bound("a", &folder), entry];
            let state = State::new(base.path(), Some(catalog), Some(live.clone()))?;
            assert!(kept(state.check(&folder, cutoff(), true)));
        }
        // The scratch root itself is T3's "No project" project.
        let root = CatalogEntry::Project {
            path: folder.parent().unwrap().into(),
        };
        let state = State::new(
            base.path(),
            Some(vec![bound("a", &folder), root]),
            Some(live),
        )?;
        assert!(!kept(state.check(&folder, cutoff(), true)));
        Ok(())
    }

    #[test]
    fn keeps_recent_or_protected_threads_and_rejects_unknown_shapes() {
        let entry = thread(Path::new("/scratch/folder"), "id");
        let threads = BTreeMap::new();
        assert!(!kept(check_thread(&threads, &entry, cutoff())));
        for (key, value) in [
            ("settledAt", Value::Null),
            ("settledAt", json!("2026-01-05T00:00:00Z")),
            ("latestRunCompletedAt", json!("2026-01-06T00:00:00Z")),
            ("unsettledAt", json!("2026-01-03T00:00:00Z")),
            ("pinnedAt", json!("2026-01-01T00:00:00Z")),
            ("snoozedUntil", json!("2026-01-09T00:00:00Z")),
            ("settledOverride", json!("active")),
            ("activeRunId", json!("active")),
            ("pendingBackgroundTasks", json!([{}])),
            ("pendingGuestMessageCount", json!(1)),
            ("pendingRuntimeRequest", json!({})),
            ("status", json!("queued")),
        ] {
            let mut entry = entry.clone();
            entry[key] = value;
            assert!(
                kept(check_thread(&threads, &entry, cutoff())),
                "removable with {key}"
            );
        }
        // A usage limit that reset within the retention period may still resume.
        let mut limited = entry.clone();
        limited["status"] = json!("failed");
        limited["lastErrorClass"] = json!("usage_limit");
        limited["usageLimitResetAt"] = json!("2026-01-06T00:00:00Z");
        assert!(kept(check_thread(&threads, &limited, cutoff())));
        limited["usageLimitResetAt"] = json!("2026-01-02T00:00:00Z");
        assert!(!kept(check_thread(&threads, &limited, cutoff())));
        for (key, value) in [
            ("status", json!("new-server-status")),
            ("settledOverride", json!("new-override")),
            ("settledAt", json!("unknown")),
        ] {
            let mut entry = entry.clone();
            entry[key] = value;
            assert!(
                check_thread(&threads, &entry, cutoff())
                    .is_err_and(|error| error.downcast_ref::<Keep>().is_none())
            );
        }
        let mut entry = entry;
        entry.as_object_mut().unwrap().remove("activeRunId");
        assert!(check_thread(&threads, &entry, cutoff()).is_err());
    }

    #[test]
    fn subagents_inherit_an_idle_parents_settlement() {
        let path = Path::new("/worktrees/project/shared");
        let parent = thread(path, "parent");
        let mut child = thread(path, "child");
        child["settledAt"] = Value::Null;
        child["lineage"] = json!({
            "parentThreadId": "parent", "relationshipToParent": "subagent", "rootThreadId": "parent",
        });
        let mut threads = BTreeMap::from([
            ("parent".to_owned(), parent.clone()),
            ("child".to_owned(), child.clone()),
        ]);
        assert!(!kept(check_thread(&threads, &child, cutoff())));
        let mut fork = child.clone();
        fork["lineage"]["relationshipToParent"] = json!("fork");
        assert!(kept(check_thread(&threads, &fork, cutoff())));
        threads.get_mut("parent").unwrap()["status"] = json!("running");
        assert!(kept(check_thread(&threads, &child, cutoff())));
    }
}
