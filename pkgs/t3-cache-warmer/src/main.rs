mod config;
mod oauth;
mod policy;
mod state;
mod t3;

use std::collections::HashSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use jiff::{Timestamp, tz::TimeZone};
use serde_json::Value;
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use crate::config::Config;
use crate::policy::{Decision, ThreadView, decide, next_wakeup, observe, seed, within_day_window};
use crate::state::{Credential, State};
use crate::t3::{T3Client, is_credential_rejected};

const NAME: &str = "t3-cache-warmer";

#[derive(Debug, Parser)]
#[command(name = NAME, version, about = "Keep T3 thread prompt caches warm during the day")]
struct Cli {
    /// Config file. Omit for built-in defaults plus `--state-dir`.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Override the state directory for this invocation.
    #[arg(long)]
    state_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command_,
}

#[derive(Debug, Subcommand)]
enum Command_ {
    /// Sign in to T3 with full access and store the MCP credential.
    Login,
    /// Warm once and exit. Useful for timers and smoke tests.
    Once {
        /// Print what would happen without sending keepalives.
        #[arg(long)]
        dry_run: bool,
    },
    /// Loop forever, waking when a refresh comes due.
    Run {
        /// Print what would happen without sending keepalives.
        #[arg(long)]
        dry_run: bool,
    },
    /// Show cached beliefs about threads.
    Status,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut config = Config::load(cli.config.as_deref())?;
    if let Some(dir) = cli.state_dir {
        config.state_dir = Some(dir);
    }
    match cli.command {
        Command_::Login => login(&config),
        Command_::Once { dry_run } => once(&config, dry_run),
        Command_::Run { dry_run } => run(&config, dry_run),
        Command_::Status => status(&config),
    }
}

/// Signs in and stores the credential with mode 600.
fn login(config: &Config) -> Result<()> {
    let dir = config.state_dir()?;
    let path = dir.join("credential.json");
    if let Some(existing) = Credential::load(&path)?
        && existing.expires_at.as_second() > Timestamp::now().as_second() + 86_400
    {
        log(&format!(
            "credential already valid until {}",
            existing.expires_at
        ));
        return Ok(());
    }
    log("creating a pairing code and signing in to T3…");
    let credential = oauth::login(config)?;
    credential.save(&path)?;
    log(&format!(
        "signed in ({}), credential valid until {}",
        credential.scope, credential.expires_at
    ));
    Ok(())
}

/// One warming pass over every configured project.
fn once(config: &Config, dry_run: bool) -> Result<()> {
    let dir = config.state_dir()?;
    let credential_path = dir.join("credential.json");
    let state_path = dir.join("state.json");
    let mut credential = Credential::load(&credential_path)?
        .context("no credential stored yet; run `t3-cache-warmer login` first")?;
    let mut state = State::load(&state_path)?;
    let now = Timestamp::now();
    state.prune(now, config.prune_days);
    let mut client = T3Client::new(&config.base_url, credential.access_token.clone());
    match pass(config, &mut client, &mut state, &state_path, dry_run) {
        Ok(summary) => {
            log(&summary);
            Ok(())
        }
        Err(error) if is_credential_rejected(&error) && config.auto_renew && !dry_run => {
            log("credential rejected; signing in again…");
            credential = oauth::login(config)?;
            credential.save(&credential_path)?;
            let mut client = T3Client::new(&config.base_url, credential.access_token.clone());
            let summary = pass(config, &mut client, &mut state, &state_path, dry_run)?;
            log(&summary);
            Ok(())
        }
        Err(error) => Err(error),
    }
}

/// One pass with its observations persisted whether it worked or not.
fn pass(
    config: &Config,
    client: &mut T3Client,
    state: &mut State,
    state_path: &Path,
    dry_run: bool,
) -> Result<String> {
    let outcome = tick(config, client, state, dry_run);
    let saved = state.save(state_path);
    let summary = outcome?;
    saved?;
    Ok(summary)
}

fn run(config: &Config, dry_run: bool) -> Result<()> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let mut signals = Signals::new([SIGINT, SIGTERM]).context("watch for SIGINT/SIGTERM")?;
    std::thread::spawn(move || {
        signals.forever().next();
        flag.store(true, Ordering::Relaxed);
    });
    log(&format!(
        "warming up to {}s before cache expiry{}",
        config.lead_seconds,
        if dry_run { " (dry run)" } else { "" }
    ));
    let dir = config.state_dir()?;
    let state_path = dir.join("state.json");
    while !stop.load(Ordering::Relaxed) {
        if let Err(error) = once(config, dry_run) {
            eprintln!("{NAME}: {error:#}");
        }
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let wait = sleep_for(config, &state_path);
        log(&format!("next pass in {}", humane(wait)));
        sleep_until(&stop, wait);
    }
    log("stopping");
    Ok(())
}

/// How long `run` sleeps between passes: until the next refresh comes due.
/// Inside the day window this is capped at `tick_seconds` so new threads and
/// activity are noticed promptly; outside it the daemon sleeps until the
/// window reopens. A due instant already past means the thread was unwarmable
/// (e.g. still running), so re-check soon without hammering the server.
fn sleep_for(config: &Config, state_path: &Path) -> Duration {
    const OVERDUE_RECHECK: Duration = Duration::from_secs(15);
    let cap = Duration::from_secs(config.tick_seconds);
    let now = Timestamp::now();
    let Ok(state) = State::load(state_path) else {
        return cap;
    };
    let Some(due) = next_wakeup(&state, config, now) else {
        return cap;
    };
    let wait = due.as_second() - now.as_second();
    if wait <= 0 {
        return OVERDUE_RECHECK.min(cap);
    }
    let wait = Duration::from_secs(wait as u64);
    if within_day_window(config, now) {
        wait.min(cap)
    } else {
        wait
    }
}

/// Sleeps until `wait` elapses, waking early on SIGINT/SIGTERM.
fn sleep_until(stop: &AtomicBool, wait: Duration) {
    let deadline = std::time::Instant::now() + wait;
    while !stop.load(Ordering::Relaxed) {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        std::thread::sleep(remaining.min(Duration::from_secs(1)));
    }
}

fn humane(wait: Duration) -> String {
    let secs = wait.as_secs();
    if secs >= 3600 {
        format!("{:.1}h", secs as f64 / 3600.0)
    } else if secs >= 120 {
        format!("{}m", secs / 60)
    } else {
        format!("{secs}s")
    }
}

fn status(config: &Config) -> Result<()> {
    use std::fmt::Write as _;
    let dir = config.state_dir()?;
    let credential = Credential::load(&dir.join("credential.json"))?;
    let state = State::load(&dir.join("state.json"))?;
    let mut out = String::new();
    match &credential {
        Some(credential) => writeln!(
            out,
            "credential: {} (expires {})",
            credential.scope, credential.expires_at
        ),
        None => writeln!(out, "credential: missing (run `t3-cache-warmer login`)"),
    }?;
    writeln!(out, "threads tracked: {}", state.threads.len())?;
    let show = |at: Option<Timestamp>| {
        at.map(|at| at.to_string())
            .unwrap_or_else(|| "-".to_owned())
    };
    let mut entries: Vec<_> = state.threads.iter().collect();
    entries.sort_by_key(|(_, thread)| thread.seen_updated_at);
    for (id, thread) in entries {
        writeln!(
            out,
            "- {id} [{} refreshes] cache={} activity={} {} {}",
            thread.refreshes,
            show(thread.last_cache_at),
            show(thread.last_activity_at),
            thread.title,
            thread.last_error.as_deref().unwrap_or_default(),
        )?;
    }
    // A closed pipe (e.g. `| head`) is not an error worth failing over.
    let _ = write!(std::io::stdout(), "{out}");
    Ok(())
}

struct TickSummary {
    warmed: u32,
    observed: u32,
    errors: u32,
}

/// Lists, observes, and warms one pass. Observations are folded into `state`
/// for every thread the server reports, so our own keepalives and anyone
/// else's activity are both accounted for before the next decision.
fn tick(
    config: &Config,
    client: &mut T3Client,
    state: &mut State,
    dry_run: bool,
) -> Result<String> {
    let now = Timestamp::now();
    let projects = project_ids(config, client)?;
    let mut summary = TickSummary {
        warmed: 0,
        observed: 0,
        errors: 0,
    };
    let mut seen: HashSet<String> = HashSet::new();
    for project in &projects {
        let threads = client
            .thread_list(project)
            .with_context(|| format!("list threads of project {project}"))?;
        for raw in &threads {
            let Some(view) = parse_thread(raw) else {
                continue;
            };
            summary.observed += 1;
            seen.insert(view.id.clone());
            let entry = state
                .threads
                .entry(view.id.clone())
                .or_insert_with(|| seed(&view));
            entry.title = view.title.clone();
            entry.provider = view.provider.clone();
            entry.model = view.model.clone();
            observe(entry, &view, now, config.send_grace_seconds);
            if let Decision::Warm = decide(config, &view, entry, now) {
                // Unique per attempt: the key deduplicates sends server-side,
                // so a new keepalive must never reuse an earlier key (the
                // refresh counter resets on real activity and would otherwise
                // collide across epochs).
                static SEND_SEQ: AtomicU64 = AtomicU64::new(0);
                let seq = SEND_SEQ.fetch_add(1, Ordering::Relaxed);
                let request_id = format!(
                    "cache-warm-{}-r{}-{}-{seq}",
                    short_id(&view.id),
                    entry.refreshes + 1,
                    now.as_second()
                );
                // The reference travels in the message so a human reading
                // the thread can tell keepalives apart; the model is still
                // asked for a one-word reply.
                let message = format!("{} (ref {request_id})", config.keepalive_message);
                if dry_run {
                    log(&format!("would warm {} ({})", view.id, view.title));
                    summary.warmed += 1;
                    continue;
                }
                match client.thread_send(&view.id, &message, &request_id) {
                    Ok(_) => {
                        entry.pending_warm_at = Some(Timestamp::now());
                        entry.pending_seen_running = false;
                        entry.refreshes += 1;
                        entry.last_error = None;
                        summary.warmed += 1;
                        log(&format!("warmed {} ({})", view.id, view.title));
                    }
                    Err(error) => {
                        entry.last_error = Some(format!("{error:#}"));
                        summary.errors += 1;
                        eprintln!("{NAME}: warm {}: {error:#}", view.id);
                    }
                }
            }
        }
    }
    // Every listing in this pass succeeded (any failure returned early), so
    // threads the server no longer reports — settled or snoozed ones, which
    // the list filters out — are gone: drop them instead of letting the
    // scheduler chase their stale due times. Narrowing config.projects
    // therefore also stops tracking the removed projects.
    state.threads.retain(|id, _| seen.contains(id));
    Ok(format!(
        "observed {} threads in {} projects, warmed {}, {} errors",
        summary.observed,
        projects.len(),
        summary.warmed,
        summary.errors
    ))
}

/// Resolves which projects to scan: the config allowlist, or everything live.
fn project_ids(config: &Config, client: &mut T3Client) -> Result<Vec<String>> {
    if !config.projects.is_empty() {
        return Ok(config.projects.clone());
    }
    let projects = client.project_list().context("list T3 projects")?;
    let ids = projects
        .iter()
        .filter_map(|project| {
            project
                .get("id")
                .or_else(|| project.get("projectId"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect::<Vec<_>>();
    if ids.is_empty() {
        bail!("T3 reported no projects");
    }
    Ok(ids)
}

/// Shapes one `t3_thread_list` entry into a view. Returns `None` for entries
/// that do not carry the fields warming needs.
fn parse_thread(raw: &Value) -> Option<ThreadView> {
    let text = |key: &str| raw.get(key).and_then(Value::as_str).unwrap_or_default();
    let flag = |key: &str| raw.get(key).and_then(Value::as_bool).unwrap_or(false);
    let id = raw.get("threadId")?.as_str()?.to_owned();
    let status = raw
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let updated_at = raw
        .get("updatedAt")
        .and_then(Value::as_str)
        .and_then(|stamp| stamp.parse::<Timestamp>().ok())?;
    let subagent = raw
        .get("relationshipToParent")
        .and_then(Value::as_str)
        .is_some_and(|relationship| relationship == "subagent");
    Some(ThreadView {
        id,
        title: text("title").to_owned(),
        settled: flag("settled"),
        snoozed: flag("snoozed"),
        status,
        provider: text("providerInstanceId").to_owned(),
        model: text("model").to_owned(),
        updated_at,
        subagent,
    })
}

fn short_id(id: &str) -> String {
    id.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .take(12)
        .collect()
}

fn log(message: &str) {
    let now = Timestamp::now()
        .to_zoned(TimeZone::system())
        .strftime("%Y-%m-%d %H:%M:%S")
        .to_string();
    println!("{now} {NAME}: {message}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::is_quiescent;
    use serde_json::json;

    #[test]
    fn parses_a_list_entry() {
        let raw = json!({
            "threadId": "thread:abc",
            "title": "Fix login",
            "settled": false,
            "snoozed": false,
            "status": "idle",
            "providerInstanceId": "codex",
            "model": "gpt-6.1-sol",
            "updatedAt": "2026-10-14T09:00:00Z",
            "relationshipToParent": null,
        });
        let view = parse_thread(&raw).expect("view");
        assert_eq!(view.id, "thread:abc");
        assert!(!view.subagent);
        assert_eq!(view.provider, "codex");
    }

    #[test]
    fn marks_subagents() {
        let raw = json!({
            "threadId": "thread:sub",
            "title": "Sub",
            "settled": false,
            "snoozed": false,
            "status": "idle",
            "providerInstanceId": "opencode",
            "model": "claude-5",
            "updatedAt": "2026-10-14T09:00:00Z",
            "relationshipToParent": "subagent",
        });
        assert!(parse_thread(&raw).expect("view").subagent);
    }

    #[test]
    fn rejects_entries_without_timestamps() {
        let raw = json!({ "threadId": "thread:x" });
        assert!(parse_thread(&raw).is_none());
    }

    #[test]
    fn missing_status_is_not_warmable() {
        // Unknown states must fail closed: only explicitly quiescent ones
        // may receive keepalives.
        let raw = json!({
            "threadId": "thread:x",
            "updatedAt": "2026-10-14T09:00:00Z",
        });
        let view = parse_thread(&raw).expect("view");
        assert!(!is_quiescent(&view.status));
    }

    #[test]
    fn request_ids_are_stable_per_refresh() {
        assert_eq!(short_id("mcp:c8fe983b-6ed2"), "mcpc8fe983b6");
        assert_eq!(short_id("thread:abc/def+ghi").len(), 12);
    }
}
