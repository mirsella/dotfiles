mod config;
mod oauth;
mod policy;
mod state;
mod t3;

use std::collections::{BTreeMap, HashSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use clap::{Parser, Subcommand};
use jiff::{Timestamp, tz::TimeZone};
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use crate::config::{Config, runtime_origin};
use crate::policy::{Decision, decide, next_wait, observe, observe_run, warming_ttl};
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
    if let Some(override_url) = &config.base_url
        && let Some(origin) = runtime_origin()
        && !same_origin(override_url, &origin)
    {
        log(&format!(
            "warning: configured baseUrl {override_url} differs from the running T3 server at {origin}"
        ));
    }
    match cli.command {
        Command_::Login => login(&config),
        Command_::Once { dry_run } => {
            let mut state = State::load(&config.state_dir()?.join("state.json"))?;
            once(&config, &mut state, dry_run)
        }
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
fn once(config: &Config, state: &mut State, dry_run: bool) -> Result<()> {
    let dir = config.state_dir()?;
    let credential_path = dir.join("credential.json");
    let state_path = dir.join("state.json");
    let credential = Credential::load(&credential_path)?
        .context("no credential stored yet; run `t3-cache-warmer login` first")?;
    let now = Timestamp::now();
    state.prune(now, config.prune_days);
    let base_url = config.base_url()?;
    let mut client = T3Client::new(&base_url, credential.access_token);
    let summary = match pass(config, &mut client, state, &state_path, dry_run) {
        Ok(summary) => summary,
        Err(error) if is_credential_rejected(&error) && config.auto_renew && !dry_run => {
            log("credential rejected; signing in again…");
            let credential = oauth::login(config)?;
            credential.save(&credential_path)?;
            let mut client = T3Client::new(&base_url, credential.access_token);
            pass(config, &mut client, state, &state_path, dry_run)?
        }
        Err(error) => return Err(error),
    };
    log(&summary);
    Ok(())
}

/// One pass with its observations persisted whether it worked or not.
fn pass(
    config: &Config,
    client: &mut T3Client,
    state: &mut State,
    state_path: &Path,
    dry_run: bool,
) -> Result<String> {
    let outcome = tick(config, client, state, state_path, dry_run);
    match (outcome, state.save(state_path)) {
        (Err(error), Err(save_error)) => {
            Err(error.context(format!("also failed saving state: {save_error:#}")))
        }
        (outcome, Ok(())) => outcome,
        (_, Err(error)) => Err(error),
    }
}

fn run(config: &Config, dry_run: bool) -> Result<()> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let runner = std::thread::current();
    let mut signals = Signals::new([SIGINT, SIGTERM]).context("watch for SIGINT/SIGTERM")?;
    std::thread::spawn(move || {
        signals.forever().next();
        flag.store(true, Ordering::Relaxed);
        runner.unpark();
    });
    log(&format!(
        "warming up to {}s before cache expiry{}",
        config.lead_seconds,
        if dry_run { " (dry run)" } else { "" }
    ));
    let dir = config.state_dir()?;
    let mut state = State::load(&dir.join("state.json"))?;
    while !stop.load(Ordering::Relaxed) {
        if let Err(error) = once(config, &mut state, dry_run) {
            eprintln!("{NAME}: {error:#}");
        }
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let wait = next_wait(&state, config, Timestamp::now());
        log(&format!("next pass in {}", humane(wait)));
        sleep_until(&stop, wait);
    }
    log("stopping");
    Ok(())
}

/// Sleeps until `wait` elapses, waking early on SIGINT/SIGTERM.
fn sleep_until(stop: &AtomicBool, wait: Duration) {
    let deadline = std::time::Instant::now() + wait;
    while !stop.load(Ordering::Relaxed) {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        std::thread::park_timeout(remaining);
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
    let now = Timestamp::now();
    for (id, thread) in entries {
        let decision = match decide(config, thread, now) {
            Decision::Warm => "due",
            Decision::Skip { reason } => reason.label(),
        };
        writeln!(
            out,
            "- {id} [{decision}, {} refreshes] provider={} model={} status={} cache={} activity={} {} {}",
            thread.refreshes,
            thread.provider,
            thread.model,
            thread.status,
            show(thread.last_cache_at()),
            show(thread.last_activity_at),
            thread.title,
            thread.last_error.as_deref().unwrap_or_default(),
        )?;
    }
    // A closed pipe (e.g. `| head`) is not an error worth failing over.
    let _ = write!(std::io::stdout(), "{out}");
    Ok(())
}

#[derive(Default)]
struct TickSummary {
    warmed: u32,
    observed: u32,
    errors: u32,
    skipped: BTreeMap<&'static str, u32>,
}

/// Lists, observes, and warms one pass. Observations are folded into `state`
/// for every thread the server reports, so our own keepalives and anyone
/// else's activity are both accounted for before the next decision.
fn tick(
    config: &Config,
    client: &mut T3Client,
    state: &mut State,
    state_path: &Path,
    dry_run: bool,
) -> Result<String> {
    let projects = if config.projects.is_empty() {
        client.project_ids().context("list T3 projects")?
    } else {
        config.projects.clone()
    };
    let mut summary = TickSummary::default();
    let mut seen: HashSet<String> = HashSet::new();
    for project in &projects {
        let threads = client
            .thread_list(project)
            .with_context(|| format!("list threads of project {project}"))?;
        for view in threads {
            summary.observed += 1;
            seen.insert(view.id.clone());
            let entry = state.threads.entry(view.id.clone()).or_default();
            let now = Timestamp::now();
            observe(entry, &view, now, config.send_grace_seconds);
            if let Err(reason) = warming_ttl(config, entry, &now.to_zoned(TimeZone::system())) {
                *summary.skipped.entry(reason.label()).or_insert(0) += 1;
                continue;
            }
            if let Some(run_id) = &view.latest_run_id
                && !entry.has_run(run_id)
            {
                let result = client
                    .completed_run(&view.id, run_id)
                    .with_context(|| format!("read latest run of thread {}", view.id));
                match result {
                    Ok(run) => {
                        observe_run(entry, run, config.send_grace_seconds);
                        entry.last_error = None;
                    }
                    Err(error) => {
                        if is_credential_rejected(&error) {
                            return Err(error);
                        }
                        entry.last_error = Some(format!("{error:#}"));
                        summary.errors += 1;
                        eprintln!("{NAME}: {error:#}; skipping this thread");
                        continue;
                    }
                }
            }
            let now = Timestamp::now();
            if let Decision::Skip { reason } = decide(config, entry, now) {
                *summary.skipped.entry(reason.label()).or_insert(0) += 1;
                continue;
            }
            if dry_run {
                log(&format!("would warm {} ({})", view.id, view.title));
                summary.warmed += 1;
                continue;
            }
            let request_id = format!("cache-warm-{}", random_token::<16>());
            // Persist before sending: a crash or lost HTTP response cannot
            // cause an immediate duplicate. Uncertain sends consume budget.
            let previous = (
                entry.pending_warm_at,
                entry.pending_seen_running,
                entry.refreshes,
            );
            entry.pending_warm_at = Some(now);
            entry.pending_seen_running = false;
            entry.refreshes += 1;
            if let Err(error) = state.save(state_path) {
                let entry = state.threads.get_mut(&view.id).expect("observed thread");
                (
                    entry.pending_warm_at,
                    entry.pending_seen_running,
                    entry.refreshes,
                ) = previous;
                return Err(error);
            }
            let outcome = client.thread_send(&view.id, &config.keepalive_message, &request_id);
            let entry = state.threads.get_mut(&view.id).expect("observed thread");
            match outcome {
                Ok(_) => {
                    entry.last_error = None;
                    summary.warmed += 1;
                    log(&format!("warmed {} ({})", view.id, view.title));
                }
                Err(error) => {
                    entry.last_error = Some(format!("{error:#}"));
                    summary.errors += 1;
                    eprintln!(
                        "{NAME}: warm {}: {error:#}; send may have been accepted, waiting {}s before retry",
                        view.id, config.send_grace_seconds
                    );
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
    let mut line = format!(
        "observed {} threads in {} projects, warmed {}, {} errors",
        summary.observed,
        projects.len(),
        summary.warmed,
        summary.errors
    );
    if !summary.skipped.is_empty() {
        let skipped = summary
            .skipped
            .iter()
            .map(|(reason, count)| format!("{reason} {count}"))
            .collect::<Vec<_>>()
            .join(", ");
        line.push_str(&format!(", skipped {skipped}"));
    }
    Ok(line)
}

fn random_token<const N: usize>() -> String {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).expect("operating system randomness");
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Whether two http origins name the same server. Loopback spellings are
/// interchangeable: localhost, 127.0.0.1 and [::1] mean the same host.
fn same_origin(a: &str, b: &str) -> bool {
    fn key(url: &str) -> (&str, &str) {
        let rest = url.trim_end_matches('/');
        let rest = rest.strip_prefix("http://").unwrap_or(rest);
        let (host, port) = rest.rsplit_once(':').unwrap_or((rest, ""));
        let host = match host {
            "localhost" | "::1" | "[::1]" => "127.0.0.1",
            host => host,
        };
        (host, port)
    }
    key(a) == key(b)
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
    use crate::state::ThreadState;
    use crate::t3::tests::mock_server;
    use serde_json::json;

    fn active_config() -> Config {
        let local = Timestamp::now().to_zoned(TimeZone::system());
        Config {
            projects: vec!["project-1".to_owned()],
            max_refreshes_per_activity: 1,
            day_window: crate::config::DayWindow {
                start: jiff::civil::time(local.hour(), 0, 0, 0),
                end: jiff::civil::time((local.hour() + 23) % 24, 0, 0, 0),
            },
            rules: vec![crate::config::Rule {
                provider: Some("opencode".to_owned()),
                model_contains: "openai/gpt".to_owned(),
                ttl_seconds: 1680,
            }],
            ..Config::default()
        }
    }

    #[test]
    fn loopback_spellings_are_the_same_origin() {
        assert!(same_origin(
            "http://localhost:3773",
            "http://127.0.0.1:3773"
        ));
        assert!(same_origin("http://[::1]:3773/", "http://127.0.0.1:3773"));
        assert!(!same_origin(
            "http://127.0.0.1:3773",
            "http://127.0.0.1:3774"
        ));
        assert!(!same_origin(
            "http://127.0.0.1:3773",
            "http://192.168.1.19:3773"
        ));
    }

    #[test]
    fn shutdown_unparks_a_long_wait() {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let (started, start) = std::sync::mpsc::channel();
        let (finished, finish) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            started.send(()).unwrap();
            sleep_until(&flag, Duration::from_secs(120));
            finished.send(()).unwrap();
        });
        start.recv_timeout(Duration::from_secs(1)).unwrap();
        stop.store(true, Ordering::Relaxed);
        worker.thread().unpark();
        finish
            .recv_timeout(Duration::from_secs(1))
            .expect("shutdown wakes the parked thread");
        worker.join().unwrap();
    }

    #[test]
    fn malformed_listing_preserves_state_and_reports_save_failures() {
        let (base, server) = mock_server(
            &["initialize", "t3_thread_list", "t3_thread_list"],
            |_, _| {
                Some(
                    json!({ "threads": [{ "threadId": "malformed", "updatedAt": "not-a-timestamp" }] }),
                )
            },
        );
        let directory =
            std::env::temp_dir().join(format!("warmer-malformed-list-{}", std::process::id()));
        let state_path = directory.join("state.json");
        let config = Config {
            projects: vec!["project".to_owned()],
            ..Config::default()
        };
        let mut state = State::default();
        state
            .threads
            .insert("preserved".to_owned(), ThreadState::default());
        state.save(&state_path).unwrap();
        let mut client = T3Client::new(&base, "test".to_owned());
        let error = pass(&config, &mut client, &mut state, &state_path, true).unwrap_err();
        assert!(
            format!("{error:#}").contains("invalid threads"),
            "{error:#}"
        );
        let saved = State::load(&state_path).unwrap();
        assert_eq!(saved.threads.len(), 1);
        assert!(saved.threads.contains_key("preserved"));

        // The listing error must not hide a simultaneous state-write failure.
        let error = pass(
            &config,
            &mut client,
            &mut state,
            &state_path.join("child"),
            true,
        )
        .unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("invalid threads"), "{message}");
        assert!(message.contains("also failed saving state"), "{message}");
        server.join().unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn run_history_failure_never_warms_from_an_older_run() {
        let updated = Timestamp::now()
            .checked_sub(jiff::Span::new().hours(1))
            .unwrap();
        let (base, server) = mock_server(
            &["initialize", "t3_thread_list", "t3_thread_read"],
            move |name, _| {
                Some(if name == "t3_thread_list" {
                    json!({ "threads": [{
                        "threadId": "thread-1", "status": "completed",
                        "providerInstanceId": "opencode", "model": "openai/gpt-6-astra",
                        "latestRunId": "run-1", "updatedAt": updated.to_string()
                    }] })
                } else {
                    json!({ "recentRuns": [] })
                })
            },
        );
        let mut state = State::default();
        let entry = state.threads.entry("thread-1".to_owned()).or_default();
        entry.last_run = Some(crate::t3::CompletedRun {
            run_id: "older-run".to_owned(),
            completed_at: updated,
        });
        entry.last_activity_at = Some(updated);
        let mut client = T3Client::new(&base, "test".to_owned());
        let config = active_config();
        let result = tick(&config, &mut client, &mut state, Path::new("unused"), false).unwrap();
        assert!(result.contains("warmed 0, 1 errors"), "{result}");
        assert!(state.threads["thread-1"].last_error.is_some());
        assert!(state.threads["thread-1"].pending_warm_at.is_none());
        server.join().unwrap();
    }

    #[test]
    fn failed_save_allows_retry_and_lost_response_blocks_duplicates() {
        let updated = Timestamp::now()
            .checked_sub(jiff::Span::new().minutes(30))
            .unwrap();
        let threads = json!({ "threads": [{
            "threadId": "thread-1", "title": "Test", "status": "completed",
            "providerInstanceId": "opencode", "model": "openai/gpt-6-astra",
            "latestRunId": "run-1",
            "updatedAt": updated.to_string()
        }] });
        let directory =
            std::env::temp_dir().join(format!("warmer-lost-send-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let blocked_directory = directory.join("not-a-directory");
        std::fs::write(&blocked_directory, b"").unwrap();
        let state_path = directory.join("state.json");
        let saved_path = state_path.clone();
        let (base, server) = mock_server(
            &[
                "initialize",
                "t3_thread_list",
                "t3_thread_read",
                "t3_thread_list",
                "t3_thread_send",
                "t3_thread_list",
            ],
            move |name, arguments| {
                if name == "t3_thread_read" {
                    Some(json!({ "recentRuns": [{
                        "runId": "run-1", "completedAt": updated.to_string()
                    }] }))
                } else if name == "t3_thread_send" {
                    let saved = State::load(&saved_path).unwrap();
                    assert!(
                        saved.threads["thread-1"].pending_warm_at.is_some(),
                        "persist before the side effect"
                    );
                    assert_eq!(arguments["mode"], "auto");
                    assert!(
                        arguments["clientRequestId"]
                            .as_str()
                            .unwrap()
                            .starts_with("cache-warm-")
                    );
                    // T3 may have queued this request, but the response is lost.
                    None
                } else {
                    Some(threads.clone())
                }
            },
        );
        let config = active_config();
        let mut client = T3Client::new(&base, "test".to_owned());
        let mut state = State::default();
        pass(
            &config,
            &mut client,
            &mut state,
            &blocked_directory.join("state.json"),
            false,
        )
        .expect_err("failed persistence must prevent sending");
        let entry = &state.threads["thread-1"];
        assert!(entry.pending_warm_at.is_none());
        assert!(!entry.pending_seen_running);
        assert_eq!(entry.refreshes, 0);

        // Storage recovers; retry using the same state retained by the daemon.
        let summary = tick(&config, &mut client, &mut state, &state_path, false).unwrap();
        assert!(summary.contains("1 errors"), "{summary}");
        // Restart from disk, before another listing has acknowledged the send.
        let mut state = State::load(&state_path).unwrap();
        let summary = tick(&config, &mut client, &mut state, &state_path, false).unwrap();
        assert!(summary.contains("pending-warm 1"), "{summary}");
        assert_eq!(state.threads["thread-1"].refreshes, 1);
        server.join().unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn reads_history_only_when_the_latest_run_changes_even_after_restart() {
        let completed = Timestamp::now();
        let mut listings = 0;
        let (base, server) = mock_server(
            &[
                "initialize",
                "t3_thread_list",
                "t3_thread_read",
                "t3_thread_list",
                "t3_thread_list",
                "t3_thread_read",
            ],
            move |name, _| {
                Some(if name == "t3_thread_list" {
                    listings += 1;
                    json!({ "threads": [{
                        "threadId": "thread-1", "status": "completed",
                        "providerInstanceId": "opencode", "model": "openai/gpt-6-astra",
                        "latestRunId": if listings < 3 { "run-1" } else { "run-2" },
                        "updatedAt": completed.checked_add(jiff::Span::new().seconds(listings)).unwrap().to_string()
                    }] })
                } else {
                    json!({ "recentRuns": [{
                        "runId": if listings < 3 { "run-1" } else { "run-2" },
                        "completedAt": completed.to_string()
                    }] })
                })
            },
        );
        let config = active_config();
        let mut client = T3Client::new(&base, "test".to_owned());
        let mut state = State::default();
        tick(&config, &mut client, &mut state, Path::new("unused"), true).unwrap();
        // Exercise the persisted cache without touching the real state file.
        let mut state: State =
            serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
        state.threads.get_mut("thread-1").unwrap().refreshes = 1;
        tick(&config, &mut client, &mut state, Path::new("unused"), true).unwrap();
        assert_eq!(state.threads["thread-1"].refreshes, 1);
        tick(&config, &mut client, &mut state, Path::new("unused"), true).unwrap();
        assert!(state.threads["thread-1"].has_run("run-2"));
        assert_eq!(state.threads["thread-1"].refreshes, 0);
        server.join().unwrap();
    }
}
