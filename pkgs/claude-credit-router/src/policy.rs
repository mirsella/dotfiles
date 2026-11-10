use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Months, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Route {
    #[default]
    Promotional,
    Subscription,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Auto,
    Subscription,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub at: DateTime<Utc>,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    version: u32,
    pub mode: Mode,
    pub active: Route,
    pub next_probe_at: Option<DateTime<Utc>>,
    pub last_promotional_success_at: Option<DateTime<Utc>>,
    pub history: Vec<Event>,
    #[serde(default)]
    audit_started_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pending_audit: Vec<crate::audit::Record>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            version: 1,
            mode: Mode::Auto,
            active: Route::Promotional,
            next_probe_at: None,
            last_promotional_success_at: None,
            history: vec![],
            audit_started_at: None,
            pending_audit: Vec::new(),
        }
    }
}

impl State {
    pub fn route(&self, now: DateTime<Utc>) -> Route {
        if self.mode == Mode::Subscription {
            return Route::Subscription;
        }
        if self.active == Route::Promotional || self.next_probe_at.is_some_and(|at| at <= now) {
            return Route::Promotional;
        }
        Route::Subscription
    }

    fn event(&mut self, now: DateTime<Utc>, reason: &str) {
        self.history.push(Event {
            at: now,
            reason: reason.into(),
        });
        if self.history.len() > 32 {
            self.history.remove(0);
        }
    }

    pub fn exhausted(&mut self, now: DateTime<Utc>, renewal: DateTime<Utc>) -> Result<bool> {
        let changed = self.active != Route::Subscription;
        // A grant can arrive after its billing timestamp. Stay on subscription
        // between six-hour probes rather than declaring it renewed prematurely.
        let awaiting_new_cycle = match self.last_promotional_success_at {
            Some(last) => next_renewal(renewal, last)? <= now,
            None => now >= renewal,
        };
        let next = if changed && !awaiting_new_cycle {
            next_renewal(renewal, now)?
        } else {
            now + chrono::Duration::hours(6)
        };
        self.active = Route::Subscription;
        self.next_probe_at = Some(next);
        self.event(
            now,
            if changed {
                "promotional_credit_balance_exhausted"
            } else {
                "renewal_grant_not_available_yet"
            },
        );
        Ok(changed)
    }

    pub fn available(&mut self, now: DateTime<Utc>) -> bool {
        let changed = self.active != Route::Promotional;
        self.active = Route::Promotional;
        self.next_probe_at = None;
        self.last_promotional_success_at = Some(now);
        if changed {
            self.event(now, "promotional_renewal_probe_succeeded");
        }
        changed
    }

    pub fn defer(&mut self, now: DateTime<Utc>, until: DateTime<Utc>, reason: &str) -> bool {
        let changed = self.active != Route::Subscription;
        if changed || self.next_probe_at != Some(until) {
            self.active = Route::Subscription;
            self.next_probe_at = Some(until);
            self.event(now, reason);
        }
        changed
    }

    pub fn set_mode(&mut self, mode: Mode, now: DateTime<Utc>) {
        self.mode = mode;
        self.event(
            now,
            match mode {
                Mode::Auto => "manual_auto",
                Mode::Subscription => "manual_subscription",
            },
        );
    }

    pub fn note(&mut self, now: DateTime<Utc>, reason: &str) {
        if self
            .history
            .last()
            .is_none_or(|event| event.reason != reason)
        {
            self.event(now, reason);
        }
    }

    pub fn recheck(&mut self, now: DateTime<Utc>) {
        self.mode = Mode::Auto;
        self.next_probe_at = Some(now);
        self.event(now, "manual_promotional_recheck");
    }
}

pub fn next_renewal(anchor: DateTime<Utc>, now: DateTime<Utc>) -> Result<DateTime<Utc>> {
    use chrono::Datelike;
    let months = ((now.year() - anchor.year()) * 12 + now.month() as i32 - anchor.month() as i32)
        .max(0) as u32;
    for offset in [months, months + 1] {
        let candidate = anchor
            .checked_add_months(Months::new(offset))
            .context("renewal date overflow")?;
        if candidate > now {
            return Ok(candidate);
        }
    }
    bail!("cannot calculate the next promotional renewal")
}

pub fn cycle_start(anchor: DateTime<Utc>, now: DateTime<Utc>) -> Result<DateTime<Utc>> {
    use chrono::Datelike;
    let months = (now.year() - anchor.year()) * 12 + now.month() as i32 - anchor.month() as i32;
    let shift = |months: i32| {
        if months >= 0 {
            anchor.checked_add_months(Months::new(months as u32))
        } else {
            anchor.checked_sub_months(Months::new(months.unsigned_abs()))
        }
    };
    let candidate = shift(months).context("grant cycle overflow")?;
    if candidate <= now {
        Ok(candidate)
    } else {
        shift(months - 1).context("grant cycle overflow")
    }
}

#[derive(Clone)]
pub struct Store(pub PathBuf);

impl Store {
    pub fn audit(&self) -> crate::audit::Audit {
        crate::audit::Audit::beside(&self.0)
    }

    pub fn update<T>(&self, action: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
        let directory = self.0.parent().context("state path has no parent")?;
        fs::create_dir_all(directory)?;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(directory.join("state.lock"))?;
        lock.lock_exclusive()?;
        let mut state: State = match fs::read(&self.0) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .context("invalid routing state; refusing to reset it")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => State::default(),
            Err(error) => return Err(error.into()),
        };
        ensure!(
            state.version == 1,
            "unsupported routing state version {}",
            state.version
        );
        ensure!(
            state.active != Route::Subscription || state.next_probe_at.is_some(),
            "exhausted route is missing its renewal date"
        );
        let before = serde_json::to_vec(&state)?;
        let history = state.history.clone();
        let result = action(&mut state)?;
        let importing = state.audit_started_at.is_none();
        if importing {
            state.audit_started_at = Some(Utc::now());
        }
        let audit = self.audit();
        if importing {
            let mut record = crate::audit::Record::new("audit_started", "policy");
            record.id = "audit_started".into();
            record.reason = Some("history_before_audit_is_incomplete".into());
            state.pending_audit.push(record);
        }
        for event in state.history.clone() {
            if importing || !history.contains(&event) {
                let mut record = crate::audit::Record::new("policy", "policy");
                record.id = format!(
                    "policy:{}:{}",
                    event.at.timestamp_nanos_opt().unwrap_or_default(),
                    event.reason
                );
                record.at = event.at;
                record.reason = Some(event.reason.clone());
                if !history.contains(&event) {
                    record.route = Some(state.route(event.at));
                }
                state.pending_audit.push(record);
            }
        }
        if before != serde_json::to_vec(&state)? || !self.0.exists() {
            self.persist(&state)?;
        }
        // Transactional outbox: the route and unacknowledged audit records are
        // durable together. A crash can duplicate an ID, never erase a switch.
        if !state.pending_audit.is_empty() {
            let exported = state
                .pending_audit
                .iter()
                .try_for_each(|record| audit.spool(record));
            match exported {
                Ok(()) => {
                    state.pending_audit.clear();
                    if let Err(error) = self.persist(&state) {
                        eprintln!(
                            "credit-router: AUDIT ACKNOWLEDGEMENT FAILED: {error}; exports will be retried"
                        );
                    }
                }
                Err(error) => eprintln!(
                    "credit-router: AUDIT EXPORT FAILED: {error}; pending records retained in state"
                ),
            }
        }
        Ok(result)
    }

    fn persist(&self, state: &State) -> Result<()> {
        let directory = self.0.parent().context("state path has no parent")?;
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
        temporary.write_all(&serde_json::to_vec_pretty(state)?)?;
        temporary.as_file().sync_all()?;
        temporary.persist(&self.0)?;
        File::open(directory)?.sync_all()?;
        Ok(())
    }

    pub fn read(&self) -> Result<State> {
        self.update(|state| Ok(state.clone()))
    }
}

pub fn private_key(path: &Path, validate: fn(&str) -> Result<String>) -> Result<String> {
    let metadata = fs::metadata(path).context("read API key metadata")?;
    ensure!(
        metadata.is_file() && metadata.permissions().mode() & 0o077 == 0,
        "API key must be a private regular file (0600 or 0400)"
    );
    let key = fs::read_to_string(path).context("read API key file")?;
    validate(&key)
}

pub fn validate_key(key: &str) -> Result<String> {
    let key = key.trim();
    ensure!(
        key.starts_with("sk-ant-")
            && !["sk-ant-oat", "sk-ant-ort", "sk-ant-sid", "sk-ant-admin"]
                .iter()
                .any(|prefix| key.starts_with(prefix))
            && !key.contains(char::is_whitespace),
        "expected an Anthropic API key, not subscription authentication"
    );
    Ok(key.into())
}
