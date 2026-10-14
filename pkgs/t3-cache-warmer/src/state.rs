use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

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
    /// Newest activity that was not one of our keepalives.
    pub last_activity_at: Option<Timestamp>,
    /// Newest activity believed to have written the prompt cache.
    pub last_cache_at: Option<Timestamp>,
    /// `updatedAt` as last observed, used to spot activity we did not cause.
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct State {
    pub threads: BTreeMap<String, ThreadState>,
}

impl State {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read state {}", path.display()))?;
        let state: Self = serde_json::from_str(&text)
            .with_context(|| format!("parse state {}", path.display()))?;
        Ok(state)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self)?;
        let directory = path.parent().context("state path has no parent")?;
        std::fs::create_dir_all(directory)
            .with_context(|| format!("create {}", directory.display()))?;
        let temporary = directory.join(".state.json.tmp");
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
    let temporary = directory.join(".credential.json.tmp");
    std::fs::write(&temporary, bytes).with_context(|| format!("write {}", temporary.display()))?;
    set_mode_600(&temporary)?;
    std::fs::rename(&temporary, path).with_context(|| format!("replace {}", path.display()))?;
    Ok(())
}

fn set_mode_600(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(0o600);
    std::fs::set_permissions(path, permissions)
        .with_context(|| format!("chmod 600 {}", path.display()))?;
    Ok(())
}
