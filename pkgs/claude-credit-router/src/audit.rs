//! Private, append-only metadata. Never accepts arbitrary request/event payloads.
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextIds {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_request_id: Option<String>,
    pub request_id: Option<String>,
    pub session_id: Option<String>,
    pub parent_session_id: Option<String>,
    pub message_id: Option<String>,
    pub native_session_id: Option<String>,
    pub response_id: Option<String>,
    pub model: Option<String>,
    pub agent: Option<String>,
    pub kind: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub reasoning: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Rates {
    pub input: f64,
    pub output: f64,
    pub cache_read: Option<f64>,
    pub cache_write: Option<f64>,
    #[serde(default)]
    pub tiers: Vec<Tier>,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Tier {
    pub above: u64,
    pub input: f64,
    pub output: f64,
    pub cache_read: Option<f64>,
    pub cache_write: Option<f64>,
}

impl Rates {
    pub fn estimate(&self, tokens: &Tokens) -> Option<f64> {
        let size = tokens
            .input
            .checked_add(tokens.cache_read)?
            .checked_add(tokens.cache_write)?;
        let tier = self
            .tiers
            .iter()
            .filter(|tier| size > tier.above)
            .max_by_key(|tier| tier.above);
        let (input, output, read, write) = tier.map_or(
            (self.input, self.output, self.cache_read, self.cache_write),
            |tier| (tier.input, tier.output, tier.cache_read, tier.cache_write),
        );
        let components = [
            (tokens.input, Some(input)),
            (tokens.output.checked_add(tokens.reasoning)?, Some(output)),
            (tokens.cache_read, read),
            (tokens.cache_write, write),
        ];
        let mut total = 0.0;
        for (count, rate) in components {
            if count == 0 {
                continue;
            }
            let rate = rate.filter(|rate| rate.is_finite() && *rate >= 0.0)?;
            total += count as f64 * rate / 1_000_000.0;
        }
        total.is_finite().then_some(total)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_unavailable_reason: Option<String>,
    pub id: String,
    pub at: DateTime<Utc>,
    pub event: String,
    pub source: String,
    pub machine: String,
    #[serde(default)]
    pub context: ContextIds,
    pub route: Option<crate::policy::Route>,
    pub reason: Option<String>,
    pub status: Option<u16>,
    pub duration_ms: Option<u64>,
    pub outcome: Option<String>,
    pub tokens: Option<Tokens>,
    pub opencode_cost_usd: Option<f64>,
    pub rates: Option<Rates>,
}

impl Record {
    pub fn new(event: &str, source: &str) -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let at = Utc::now();
        Self {
            id: format!(
                "{}-{}-{}",
                std::process::id(),
                at.timestamp_nanos_opt().unwrap_or_default(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ),
            at,
            event: event.into(),
            source: source.into(),
            machine: fs::read_to_string("/proc/sys/kernel/hostname")
                .unwrap_or_else(|_| "unknown".into())
                .trim()
                .into(),
            context: ContextIds::default(),
            route: None,
            reason: None,
            status: None,
            duration_ms: None,
            outcome: None,
            tokens: None,
            opencode_cost_usd: None,
            rates: None,
            cost_unavailable_reason: None,
        }
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            matches!(
                self.event.as_str(),
                "request_started"
                    | "request_finished"
                    | "opencode_request"
                    | "opencode_usage"
                    | "policy"
                    | "audit_started"
                    | "pricing"
                    | "opencode_binding"
            ),
            "unsupported audit event"
        );
        ensure!(
            matches!(self.source.as_str(), "router" | "opencode" | "policy"),
            "unsupported audit source"
        );
        let ids = &self.context;
        for value in [
            Some(&self.id),
            Some(&self.machine),
            ids.request_id.as_ref(),
            ids.session_id.as_ref(),
            ids.parent_session_id.as_ref(),
            ids.message_id.as_ref(),
            ids.native_session_id.as_ref(),
            ids.response_id.as_ref(),
            ids.upstream_request_id.as_ref(),
            ids.model.as_ref(),
            ids.agent.as_ref(),
            ids.kind.as_ref(),
            self.reason.as_ref(),
            self.outcome.as_ref(),
            self.cost_unavailable_reason.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            ensure!(
                value.len() <= 256
                    && !value.contains(char::is_control)
                    && !value.starts_with("sk-"),
                "invalid audit identifier"
            );
        }
        ensure!(
            self.opencode_cost_usd
                .is_none_or(|cost| cost.is_finite() && cost >= 0.0),
            "invalid reported cost"
        );
        Ok(())
    }
}

#[derive(Clone)]
pub struct Audit(pub PathBuf);

impl Audit {
    pub fn beside(state: &Path) -> Self {
        Self(state.with_file_name("audit.sqlite3"))
    }

    fn open(&self) -> Result<Connection> {
        let parent = self.0.parent().context("audit path has no parent")?;
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&self.0)?;
        ensure!(
            file.metadata()?.is_file(),
            "audit database is not a regular file"
        );
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        let db = Connection::open(&self.0)?;
        db.busy_timeout(std::time::Duration::from_secs(2))?;
        // DELETE keeps SQLite sidecars transient; no credential material is ever stored.
        db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS records(id TEXT PRIMARY KEY, at TEXT NOT NULL, event TEXT NOT NULL, data TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS records_at ON records(at);
            CREATE TABLE IF NOT EXISTS spools(name TEXT PRIMARY KEY, offset INTEGER NOT NULL);")?;
        Ok(db)
    }

    pub fn append(&self, record: &Record) -> Result<()> {
        record.validate()?;
        let db = self.open()?;
        insert(&db, record)?;
        Ok(())
    }

    pub fn emit(&self, record: &Record) {
        // SQLite ingestion/reporting never runs on the inference critical path.
        // A private append-only spool remains durable without database lock waits.
        if let Err(error) = self.spool(record) {
            eprintln!("credit-router: AUDIT WRITE FAILED: {error}");
        }
    }

    pub(crate) fn spool(&self, record: &Record) -> Result<()> {
        use fs2::FileExt;
        use std::io::Write;
        static START: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
        record.validate()?;
        let directory = self.0.with_file_name("audit-spool");
        fs::create_dir_all(&directory)?;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        std::fs::File::open(directory.parent().context("spool has no parent")?)?.sync_all()?;
        let start = START.get_or_init(|| Utc::now().timestamp_nanos_opt().unwrap_or_default());
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(directory.join(format!("router-{}-{start}.jsonl", std::process::id())))?;
        ensure!(
            file.metadata()?.is_file() && file.metadata()?.permissions().mode() & 0o077 == 0,
            "audit spool is not private"
        );
        file.lock_exclusive()?;
        let before = file.metadata()?.len();
        let mut line = serde_json::to_vec(record)?;
        line.push(b'\n');
        if let Err(error) = file.write_all(&line) {
            file.set_len(before)?;
            return Err(error.into());
        }
        file.sync_all()?;
        std::fs::File::open(directory)?.sync_all()?;
        Ok(())
    }

    /// Import private metadata spools incrementally. Partial trailing lines are
    /// left for the next pass; transaction+event IDs make crash recovery idempotent.
    pub fn ingest(&self) -> Result<()> {
        let directory = self.0.with_file_name("audit-spool");
        if !directory.exists() {
            return Ok(());
        }
        let mut db = self.open()?;
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !(name.starts_with("opencode-") || name.starts_with("router-"))
                || !name.ends_with(".jsonl")
            {
                continue;
            }
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(entry.path())?;
            ensure!(
                file.metadata()?.is_file() && file.metadata()?.permissions().mode() & 0o077 == 0,
                "audit spool is not private"
            );
            let transaction = db.transaction()?;
            let offset: u64 = transaction.query_row(
                "SELECT COALESCE((SELECT offset FROM spools WHERE name=?1),0)",
                [&name],
                |row| row.get(0),
            )?;
            ensure!(
                file.metadata()?.len() >= offset,
                "audit spool was truncated; refusing to lose history"
            );
            let mut reader = BufReader::new(file);
            reader.seek(SeekFrom::Start(offset))?;
            let mut end = offset;
            loop {
                let mut line = Vec::new();
                let count =
                    std::io::Read::take(&mut reader, 65_537).read_until(b'\n', &mut line)?;
                if count == 0 {
                    break;
                }
                ensure!(count <= 65_536, "oversized audit record");
                if !line.ends_with(b"\n") {
                    break;
                }
                let record: Record =
                    serde_json::from_slice(&line).context("invalid metadata audit record")?;
                record.validate()?;
                insert(&transaction, &record)?;
                end += count as u64;
            }
            transaction.execute("INSERT INTO spools(name,offset) VALUES(?1,?2) ON CONFLICT(name) DO UPDATE SET offset=excluded.offset", params![name,end])?;
            transaction.commit()?;
        }
        Ok(())
    }

    pub fn records(&self, since: Option<DateTime<Utc>>) -> Result<Vec<Record>> {
        self.ingest()?;
        let db = self.open()?;
        let mut statement = db.prepare("SELECT data FROM records WHERE at>=?1 ORDER BY at,id")?;
        let rows = statement.query_map(
            [since.map_or(String::new(), |date| date.to_rfc3339())],
            |row| row.get::<_, String>(0),
        )?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }
}

fn insert(db: &Connection, record: &Record) -> Result<()> {
    db.execute(
        "INSERT OR IGNORE INTO records(id,at,event,data) VALUES(?1,?2,?3,?4)",
        params![
            record.id,
            record.at.to_rfc3339(),
            record.event,
            serde_json::to_string(record)?
        ],
    )?;
    Ok(())
}

/// T3 owns this database. Read only the explicit native binding IDs, never
/// messages, credentials, titles or guessed model/time matches.
pub fn thread_bindings(path: &Path) -> Result<HashMap<String, Vec<String>>> {
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.busy_timeout(std::time::Duration::from_secs(2))?;
    let mut query = db.prepare("SELECT thread_id,json_extract(payload_json,'$.nativeThreadRef.nativeId') FROM orchestration_v2_projection_provider_threads WHERE json_extract(payload_json,'$.nativeThreadRef.driver')='opencode'")?;
    let mut bindings = HashMap::<String, Vec<String>>::new();
    for row in query.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
    })? {
        let (thread, session) = row?;
        if let Some(session) = session.filter(|id| id.starts_with("ses_")) {
            bindings.entry(session).or_default().push(thread);
        }
    }
    Ok(bindings)
}

pub fn correlate(records: &mut [Record]) {
    let mut native_sessions = HashMap::<String, std::collections::HashSet<String>>::new();
    for record in records.iter() {
        if let (Some(native), Some(session)) = (
            &record.context.native_session_id,
            &record.context.session_id,
        ) {
            native_sessions
                .entry(native.clone())
                .or_default()
                .insert(session.clone());
        }
    }
    for record in records {
        if record.context.session_id.is_none() {
            record.context.session_id = record
                .context
                .native_session_id
                .as_ref()
                .and_then(|id| native_sessions.get(id))
                .filter(|sessions| sessions.len() == 1)
                .and_then(|sessions| sessions.iter().next().cloned());
        }
    }
}

pub fn report(records: &[Record], bindings: &HashMap<String, Vec<String>>) -> serde_json::Value {
    report_filtered(records, bindings, None, None)
}

pub fn report_filtered(
    records: &[Record],
    bindings: &HashMap<String, Vec<String>>,
    since: Option<DateTime<Utc>>,
    selected: Option<&str>,
) -> serde_json::Value {
    let mut parents = HashMap::new();
    let mut prices = HashMap::<String, Vec<&Rates>>::new();
    let mut native_sessions = HashMap::<String, std::collections::HashSet<String>>::new();
    for record in records {
        if let Some(session) = &record.context.session_id {
            if let Some(parent) = &record.context.parent_session_id {
                parents.insert(session.clone(), parent.clone());
            }
        }
        if let (Some(session), Some(native)) = (
            &record.context.session_id,
            &record.context.native_session_id,
        ) {
            native_sessions
                .entry(native.clone())
                .or_default()
                .insert(session.clone());
        }
        if let (Some(model), Some(rates)) = (&record.context.model, &record.rates) {
            prices.entry(model.clone()).or_default().push(rates);
        }
    }
    let mut groups = BTreeMap::<String, serde_json::Value>::new();
    for record in records {
        if record.event != "request_finished" && record.event != "opencode_usage" {
            continue;
        }
        if since.is_some_and(|since| record.at < since) {
            continue;
        }
        let session = record
            .context
            .session_id
            .clone()
            .or_else(|| {
                record
                    .context
                    .native_session_id
                    .as_ref()
                    .and_then(|id| native_sessions.get(id))
                    .filter(|sessions| sessions.len() == 1)
                    .and_then(|sessions| sessions.iter().next().cloned())
            })
            .unwrap_or_else(|| "uncorrelated".into());
        if selected.is_some_and(|selected| selected != session) {
            continue;
        }
        let mut ancestor = session.clone();
        let mut threads = Vec::new();
        for _ in 0..32 {
            if let Some(found) = bindings.get(&ancestor) {
                threads = found.clone();
                break;
            }
            let Some(parent) = parents.get(&ancestor) else {
                break;
            };
            ancestor = parent.clone();
        }
        // Separate transport truth from OpenCode accounting; summing both would
        // double-count. Subscription costs are never labelled API spending.
        let route = record.route.map_or("unobserved", |route| match route {
            crate::policy::Route::Promotional => "api",
            crate::policy::Route::Subscription => "subscription",
        });
        let key = format!(
            "{}|{}|{}|{}|{}",
            record.machine,
            session,
            record.context.model.as_deref().unwrap_or("unknown"),
            record.source,
            route
        );
        let group = groups.entry(key).or_insert_with(|| serde_json::json!({"machine":record.machine,"first_at":record.at,"last_at":record.at,"session_id":session,"t3_thread_ids":threads,"model":record.context.model,"source":record.source,"route":route,"requests":0,"tokens":Tokens::default(),"usage_missing":0,"api_cost_missing":0,"estimated_api_usd":null,"opencode_reported_usd":null}));
        group["last_at"] = serde_json::to_value(record.at).expect("UTC timestamp serialization");
        group["requests"] = (group["requests"].as_u64().unwrap() + 1).into();
        if let Some(tokens) = &record.tokens {
            for (key, value) in [
                ("input", tokens.input),
                ("output", tokens.output),
                ("reasoning", tokens.reasoning),
                ("cache_read", tokens.cache_read),
                ("cache_write", tokens.cache_write),
            ] {
                group["tokens"][key] =
                    (group["tokens"][key].as_u64().unwrap().saturating_add(value)).into();
            }
            if route == "api" {
                if let Some(cost) = record
                    .cost_unavailable_reason
                    .is_none()
                    .then_some(())
                    .and_then(|_| {
                        record
                            .context
                            .model
                            .as_ref()
                            .and_then(|id| prices.get(id))
                            .and_then(|rates| {
                                rates
                                    .iter()
                                    .filter(|rates| rates.fetched_at <= record.at)
                                    .max_by_key(|rates| rates.fetched_at)
                            })
                            .and_then(|rates| rates.estimate(tokens))
                    })
                {
                    group["estimated_api_usd"] =
                        (group["estimated_api_usd"].as_f64().unwrap_or_default() + cost).into();
                } else {
                    group["api_cost_missing"] =
                        (group["api_cost_missing"].as_u64().unwrap() + 1).into();
                }
            }
        } else {
            group["usage_missing"] = (group["usage_missing"].as_u64().unwrap() + 1).into();
            if route == "api" {
                group["api_cost_missing"] =
                    (group["api_cost_missing"].as_u64().unwrap() + 1).into();
            }
        }
        if let Some(cost) = record.opencode_cost_usd {
            group["opencode_reported_usd"] =
                (group["opencode_reported_usd"].as_f64().unwrap_or_default() + cost).into();
        }
    }
    serde_json::json!({"groups":groups.into_values().collect::<Vec<_>>(),"coverage":{"started_at":records.iter().find(|record|record.event=="audit_started").map(|record|record.at),"history_before_audit_is_incomplete":true},"policy_events":records.iter().filter(|record|record.event=="policy" && since.is_none_or(|since|record.at>=since)).collect::<Vec<_>>(),"billing":"Estimates only; not vendor invoices or per-request promotional-versus-paid attribution. Sources are separate: do not add router and OpenCode token totals. Estimated totals exclude api_cost_missing requests."})
}
