use std::collections::BTreeMap;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::t3::CompletedRun;

/// The MCP OAuth credential. Lives in its own file so state stays shareable.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credential {
    pub access_token: String,
    pub expires_at: Timestamp,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: String,
}

impl Credential {
    pub fn load(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read credential {}", path.display()))?;
        let credential: Self = serde_json::from_str(&text)
            .with_context(|| format!("parse credential {}", path.display()))?;
        Ok(Some(credential))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        write_private(path, &serde_json::to_vec_pretty(self)?)
    }
}

/// What we believe about one thread's cache.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThreadState {
    pub title: String,
    pub provider: String,
    pub model: String,
    pub status: String,
    /// Newest activity that was not one of our keepalives.
    pub last_activity_at: Option<Timestamp>,
    /// Legacy activity is untrusted until a different run is observed.
    pub awaiting_user_activity: bool,
    /// Reuse this completion while T3 reports the same run ID.
    pub last_run: Option<CompletedRun>,
    /// Thread metadata timestamp, used only for pruning, not cache freshness.
    pub seen_updated_at: Option<Timestamp>,
    /// Set while a keepalive turn is expected to settle into the thread.
    pub pending_warm_at: Option<Timestamp>,
    /// Our keepalive's run was observed while pending.
    pub pending_seen_running: bool,
    /// Last-seen parking flags, so the scheduler can skip threads that no
    /// pass would warm without re-listing them first.
    pub settled: bool,
    pub snoozed: bool,
    pub subagent: bool,
    pub refreshes: u32,
    pub last_error: Option<String>,
}

impl ThreadState {
    pub fn has_run(&self, run_id: &str) -> bool {
        self.last_run
            .as_ref()
            .is_some_and(|run| run.run_id == run_id)
    }

    pub fn last_cache_at(&self) -> Option<Timestamp> {
        self.last_run.as_ref().and_then(|run| run.started_at)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct State {
    #[serde(default)]
    pub schema_version: u32,
    pub threads: BTreeMap<String, ThreadState>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            schema_version: 1,
            threads: BTreeMap::new(),
        }
    }
}

impl State {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read state {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("parse state {}", path.display()))
    }

    fn parse(text: &str) -> Result<Self> {
        // Missing version identifies both metadata-based and completion-based
        // files. Never manufacture fresh user activity from their first read.
        let mut state: Self = serde_json::from_str(text)?;
        anyhow::ensure!(
            state.schema_version <= 1,
            "unsupported warmer state version"
        );
        if state.schema_version == 0 {
            for thread in state.threads.values_mut() {
                thread.last_run = None;
                thread.last_activity_at = None;
                thread.awaiting_user_activity = true;
            }
            eprintln!(
                "t3-cache-warmer: upgraded {} legacy entries; preserved budgets, awaiting new user activity",
                state.threads.len()
            );
        }
        state.schema_version = 1;
        Ok(state)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self)?;
        let directory = path.parent().context("state path has no parent")?;
        std::fs::create_dir_all(directory)
            .with_context(|| format!("create {}", directory.display()))?;
        let temporary = temp_path(path);
        std::fs::write(&temporary, bytes)
            .with_context(|| format!("write {}", temporary.display()))?;
        std::fs::rename(&temporary, path).with_context(|| format!("replace {}", path.display()))?;
        Ok(())
    }

    pub fn prune(&mut self, now: Timestamp, keep_days: u64) {
        let cutoff = now.as_second().saturating_sub(keep_days as i64 * 86_400);
        self.threads.retain(|_, thread| {
            thread
                .seen_updated_at
                .or(thread.last_activity_at)
                .is_none_or(|seen| seen.as_second() >= cutoff)
        });
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let directory = path.parent().context("path has no parent")?;
    std::fs::create_dir_all(directory)
        .with_context(|| format!("create {}", directory.display()))?;
    let temporary = temp_path(path);
    // The mode is set at creation: a plain write would leave the bearer
    // token world-readable until a later chmod, or forever on a crash.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .with_context(|| format!("write {}", temporary.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("write {}", temporary.display()))?;
    drop(file);
    std::fs::rename(&temporary, path).with_context(|| format!("replace {}", path.display()))?;
    // Rename preserves the temp file's mode, but re-assert it in case the
    // filesystem ignored the creation mode.
    set_mode_600(path)?;
    Ok(())
}

/// Sibling temp path unique to this write, so a concurrent `login`/`once`
/// sharing the directory can never publish the other's partial bytes.
fn temp_path(path: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let stem = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("tmp");
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(".{stem}.tmp-{}-{count}", std::process::id()))
}

fn set_mode_600(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(0o600);
    std::fs::set_permissions(path, permissions)
        .with_context(|| format!("chmod 600 {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn legacy_state_preserves_budgets_and_waits_for_a_different_run() {
        use crate::policy::observe_run;
        for old_run in [
            serde_json::Value::Null,
            serde_json::json!({
                "runId": "keepalive", "completedAt": "2026-10-20T10:00:00Z"
            }),
        ] {
            let mut state = State::parse(
                &serde_json::json!({ "threads": { "thread-1": {
                    "lastActivityAt": "2026-10-20T09:00:00Z", "lastCacheAt": "2026-10-20T10:00:00Z",
                    "lastRun": old_run, "refreshes": 3
                }}})
                .to_string(),
            )
            .unwrap();
            assert_eq!(state.schema_version, 1);
            let entry = state.threads.get_mut("thread-1").unwrap();
            assert!(entry.awaiting_user_activity);
            assert!(entry.last_activity_at.is_none());
            let run = CompletedRun {
                run_id: "keepalive".to_owned(),
                started_at: Some("2026-10-20T09:59:00Z".parse().unwrap()),
                completed_at: "2026-10-20T10:00:00Z".parse().unwrap(),
            };
            observe_run(entry, run.clone(), 300);
            assert_eq!(entry.refreshes, 3);
            assert!(entry.awaiting_user_activity);
            let mut restored = State::parse(&serde_json::to_string(&state).unwrap()).unwrap();
            let entry = restored.threads.get_mut("thread-1").unwrap();
            observe_run(entry, run.clone(), 300);
            assert_eq!(entry.refreshes, 3);
            let mut next = run;
            next.run_id = "new-user-run".to_owned();
            observe_run(entry, next, 300);
            assert!(!entry.awaiting_user_activity);
            assert_eq!(entry.refreshes, 0);
            assert!(entry.last_activity_at.is_some());
        }
        assert!(State::parse(r#"{"schemaVersion":2,"threads":{}}"#).is_err());
    }

    #[test]
    fn private_files_are_created_mode_600() {
        let dir = std::env::temp_dir().join(format!("warmer-test-{}", std::process::id()));
        let path = dir.join("credential.json");
        write_private(&path, b"secret").expect("write");
        let mode = std::fs::metadata(&path).expect("stat").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn temp_names_are_unique_per_write() {
        let path = Path::new("/nonexistent/state.json");
        assert_ne!(temp_path(path), temp_path(path));
    }
}
