//! Run a command after every other T3 Code thread has gone idle.

use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::Parser;
use comfy_table::Color;
use idle_watchdog::dashboard::{Dashboard, DashboardRow, DashboardView};
use idle_watchdog::runtime::{
    Cancellation, DEFAULT_IDLE_CHECKS, DEFAULT_INTERVAL, REQUEST_ATTEMPTS, State, clean_text,
    command_display, exit_with, http_client, resolve_command, run_command, signal_exit, summarize,
};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tempfile::NamedTempFile;

const NAME: &str = "t3-idle-watchdog";
const DEFAULT_URL: &str = "http://127.0.0.1:3773";
const DEFAULT_TOKEN_TTL: &str = "12h";
const TOKEN_COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
const TOKEN_REFRESH_MARGIN: Duration = Duration::from_secs(300);
const REVOKE_TIMEOUT: Duration = Duration::from_secs(5);
const REVOKE_ATTEMPTS: u32 = 2;
const ORCHESTRATION_PROTOCOL: &str = "2";
const HEADERS: [&str; 6] = ["STATUS", "AGE", "UPDATED", "PROJECT", "THREAD", "TITLE"];

#[derive(Parser)]
#[command(
    about = "Wait until all other T3 Code threads go idle, then run a command in the foreground.",
    after_help = "Shortcuts: sleep/suspend -> systemctl suspend, poweroff -> systemctl poweroff, hibernate -> systemctl hibernate"
)]
struct Options {
    #[arg(
        short = 'i',
        long,
        default_value_t = DEFAULT_INTERVAL,
        value_parser = clap::value_parser!(u64).range(1..)
    )]
    interval: u64,

    #[arg(
        short = 'r',
        long,
        default_value_t = DEFAULT_IDLE_CHECKS,
        value_parser = clap::value_parser!(u32).range(1..)
    )]
    required_idle_checks: u32,

    /// T3 server base URL. Defaults to T3CODE_URL or the running `t3 serve`.
    #[arg(long, value_name = "URL")]
    url: Option<String>,

    /// T3 data directory. Defaults to T3CODE_HOME, the running server's base directory, or ~/.t3.
    #[arg(long, value_name = "DIR")]
    base_dir: Option<PathBuf>,

    /// T3 CLI executable used to issue a read-only bearer token.
    #[arg(long, value_name = "PATH")]
    t3: Option<PathBuf>,

    /// Bearer token. Prefer T3CODE_TOKEN or --token-file; command-line tokens are visible in ps.
    #[arg(long, value_name = "TOKEN")]
    token: Option<String>,

    /// Read the bearer token from a file. The file is reread after a 401.
    #[arg(long, value_name = "PATH", conflicts_with = "token")]
    token_file: Option<PathBuf>,

    /// Lifetime of an automatically issued token, passed to `t3 auth session issue`.
    #[arg(long, default_value = DEFAULT_TOKEN_TTL, value_name = "TTL")]
    token_ttl: String,

    /// Exclude a thread from idle checks. Repeatable. Reads T3CODE_THREAD_ID or T3_THREAD_ID when set.
    #[arg(long = "ignore-thread", value_name = "THREAD", action = clap::ArgAction::Append)]
    ignored_threads: Vec<String>,

    /// Stop blocking after this long only for transport failures (0 blocks forever; all other errors block).
    #[arg(long, default_value_t = 0, value_name = "SECONDS")]
    stuck_after: u64,

    /// Scan once, print the result, and exit. Exit status 0 is idle, 2 is busy, 1 is an error.
    #[arg(long)]
    once: bool,

    #[arg(value_name = "COMMAND", num_args = 0..)]
    command: Vec<OsString>,
}

struct Config {
    url: String,
    base_dir: PathBuf,
    t3: PathBuf,
    token_source: TokenSource,
    token_ttl: String,
    ignored_threads: Vec<String>,
}

#[derive(Clone)]
enum TokenSource {
    Static(String),
    File(PathBuf),
    Issued,
}

struct IssuedToken {
    token: String,
    session_id: String,
    expires_at: Timestamp,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedToken {
    base_dir: PathBuf,
    ttl: String,
    token: String,
    session_id: String,
    expires_at: Timestamp,
}

struct Auth {
    source: TokenSource,
    token: Option<String>,
    expires_at: Option<Timestamp>,
    session_id: Option<String>,
    t3: PathBuf,
    base_dir: PathBuf,
    ttl: String,
    cache_file: PathBuf,
    cached: bool,
}

impl Auth {
    fn new(config: &Config, cache_dir: &Path) -> Result<Self> {
        let mut auth = Self {
            source: config.token_source.clone(),
            token: None,
            expires_at: None,
            session_id: None,
            t3: config.t3.clone(),
            base_dir: config.base_dir.clone(),
            ttl: config.token_ttl.clone(),
            cache_file: cache_dir.join("token.json"),
            cached: false,
        };
        match &auth.source {
            TokenSource::Static(_) => {}
            TokenSource::File(path) => auth.token = Some(read_token_file(path)?),
            TokenSource::Issued => {
                if let Some(cached) = read_cached_token(&auth.cache_file, &auth.base_dir, &auth.ttl)
                {
                    auth.token = Some(cached.token);
                    auth.session_id = Some(cached.session_id);
                    auth.expires_at = Some(cached.expires_at);
                    auth.cached = true;
                } else {
                    auth.issue()?;
                }
            }
        }
        Ok(auth)
    }

    fn write_cache(&self, issued: &IssuedToken) -> Result<()> {
        let parent = self
            .cache_file
            .parent()
            .context("token cache path has no parent")?;
        fs::create_dir_all(parent)?;
        let cached = CachedToken {
            base_dir: self.base_dir.clone(),
            ttl: self.ttl.clone(),
            token: issued.token.clone(),
            session_id: issued.session_id.clone(),
            expires_at: issued.expires_at,
        };
        let mut file = NamedTempFile::new_in(parent)?;
        serde_json::to_writer(file.as_file_mut(), &cached)?;
        file.as_file_mut().write_all(b"\n")?;
        file.persist(&self.cache_file)
            .map_err(|error| error.error)?;
        Ok(())
    }

    fn is_issued(&self) -> bool {
        matches!(&self.source, TokenSource::Issued)
    }

    fn source_label(&self) -> &'static str {
        match &self.source {
            TokenSource::Static(_) => "token",
            TokenSource::File(_) => "token-file",
            TokenSource::Issued => "issued",
        }
    }

    fn describe(&self) -> String {
        let session = self.session_id.as_deref().unwrap_or("unknown");
        let expires = self
            .expires_at
            .map(|expires_at| expires_at.to_string())
            .unwrap_or_else(|| "unknown".to_owned());
        let verb = if self.cached {
            "using cached"
        } else {
            "issued"
        };
        format!("{verb} T3 read token session={session} expires={expires}")
    }

    fn token(&self) -> Result<&str> {
        match &self.source {
            TokenSource::Static(token) => Ok(token),
            _ => self.token.as_deref().context("no T3 bearer token"),
        }
    }

    fn ensure_fresh(&mut self) -> Result<Option<String>> {
        if !self.is_issued() {
            return Ok(None);
        }
        let needs_refresh = self
            .expires_at
            .is_none_or(|expires_at| expires_soon(expires_at, TOKEN_REFRESH_MARGIN));
        if !needs_refresh {
            return Ok(None);
        }
        self.issue()?;
        Ok(Some(self.describe()))
    }

    fn refresh_after_401(&mut self) -> Result<String> {
        let file = match &self.source {
            TokenSource::File(path) => Some(path.clone()),
            _ => None,
        };
        if self.is_issued() {
            self.issue()?;
            return Ok(self.describe());
        }
        if let Some(path) = file {
            let token = read_token_file(&path)?;
            if self.token.as_deref() == Some(token.as_str()) {
                bail!(
                    "token file {} still contains the rejected bearer token",
                    path.display()
                );
            }
            self.token = Some(token);
            self.expires_at = None;
            return Ok(format!("reloaded T3 bearer token from {}", path.display()));
        }
        bail!(
            "T3 rejected the provided bearer token; issue a new token or omit --token/--token-file to let the watchdog issue one"
        )
    }

    fn issue(&mut self) -> Result<()> {
        let issued = issue_token(&self.t3, &self.base_dir, &self.ttl)?;
        self.cached = false;
        let cache_result = self.write_cache(&issued);
        let old_session = self.session_id.take();
        self.token = Some(issued.token);
        self.expires_at = Some(issued.expires_at);
        self.session_id = Some(issued.session_id);
        if let Some(old_session) = old_session
            && self.session_id.as_deref() != Some(old_session.as_str())
        {
            revoke_session(&self.t3, &self.base_dir, &old_session);
        }
        if let Err(error) = cache_result {
            eprintln!("{NAME}: could not cache T3 read token: {error:#}");
        }
        Ok(())
    }
}

fn read_cached_token(cache_file: &Path, base_dir: &Path, ttl: &str) -> Option<CachedToken> {
    let contents = fs::read_to_string(cache_file).ok()?;
    let cached: CachedToken = serde_json::from_str(&contents).ok()?;
    (cached.base_dir == base_dir
        && cached.ttl == ttl
        && !cached.token.is_empty()
        && !cached.session_id.is_empty()
        && !expires_soon(cached.expires_at, TOKEN_REFRESH_MARGIN))
    .then_some(cached)
}

fn read_token_file(path: &Path) -> Result<String> {
    let token = fs::read_to_string(path)
        .with_context(|| format!("read T3 bearer token from {}", path.display()))?;
    let token = token.trim();
    if token.is_empty() {
        bail!("T3 bearer token file {} is empty", path.display());
    }
    Ok(token.to_owned())
}

fn issue_token(t3: &Path, base_dir: &Path, ttl: &str) -> Result<IssuedToken> {
    let mut command = Command::new(t3);
    command
        .args([
            "auth",
            "session",
            "issue",
            "--json",
            "--scope",
            "orchestration:read",
            "--ttl",
            ttl,
            "--label",
            NAME,
            "--base-dir",
        ])
        .arg(base_dir);
    let output = run_output_timeout(command, TOKEN_COMMAND_TIMEOUT)
        .with_context(|| format!("run {} auth session issue", t3.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "{} auth session issue failed ({}): {}",
            t3.display(),
            output.status,
            summarize_text(stderr.trim(), 500),
        );
    }
    let value: Value = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("parse {} auth session issue output", t3.display()))?;
    parse_issued_token(&value)
}

fn parse_issued_token(value: &Value) -> Result<IssuedToken> {
    let token = value
        .get("token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .context("t3 auth session issue returned no token")?
        .to_owned();
    let session_id = value
        .get("sessionId")
        .and_then(Value::as_str)
        .filter(|session_id| !session_id.is_empty())
        .context("t3 auth session issue returned no sessionId")?
        .to_owned();
    let expires = value
        .get("expiresAt")
        .and_then(Value::as_str)
        .context("t3 auth session issue returned no expiresAt")?;
    let expires_at = expires
        .trim()
        .parse()
        .with_context(|| format!("parse token expiry {expires}"))?;
    Ok(IssuedToken {
        token,
        session_id,
        expires_at,
    })
}

fn revoke_session(t3: &Path, base_dir: &Path, session_id: &str) {
    for attempt in 0..REVOKE_ATTEMPTS {
        let mut command = Command::new(t3);
        command
            .args(["auth", "session", "revoke", session_id, "--base-dir"])
            .arg(base_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if run_status_timeout(command, REVOKE_TIMEOUT).is_some_and(|status| status.success()) {
            return;
        }
        if attempt + 1 < REVOKE_ATTEMPTS {
            thread::sleep(Duration::from_millis(200));
        }
    }
    eprintln!("{NAME}: failed to revoke T3 session {session_id}");
}

fn wait_child_timeout(child: &mut Child, timeout: Duration) -> io::Result<Option<ExitStatus>> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn run_output_timeout(mut command: Command, timeout: Duration) -> Result<Output> {
    // A grandchild can inherit the CLI's stdout/stderr and keep pipes open long
    // after the CLI exits. Temporary files avoid waiting for those inherited
    // descriptors before reading the token.
    let mut stdout = tempfile::tempfile().context("create temporary stdout")?;
    let mut stderr = tempfile::tempfile().context("create temporary stderr")?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout.try_clone()?))
        .stderr(Stdio::from(stderr.try_clone()?))
        .spawn()
        .context("spawn command")?;
    let Some(status) = wait_child_timeout(&mut child, timeout).context("wait for command")? else {
        bail!("command timed out after {}s", timeout.as_secs());
    };
    stdout.seek(SeekFrom::Start(0))?;
    stderr.seek(SeekFrom::Start(0))?;
    let mut stdout_bytes = Vec::new();
    let mut stderr_bytes = Vec::new();
    stdout.read_to_end(&mut stdout_bytes)?;
    stderr.read_to_end(&mut stderr_bytes)?;
    Ok(Output {
        status,
        stdout: stdout_bytes,
        stderr: stderr_bytes,
    })
}

fn run_status_timeout(mut command: Command, timeout: Duration) -> Option<ExitStatus> {
    let mut child = command.spawn().ok()?;
    wait_child_timeout(&mut child, timeout).ok().flatten()
}

struct ServerProcess {
    exe: Option<PathBuf>,
    url: Option<String>,
    base_dir: Option<PathBuf>,
}

fn resolve_config(options: &Options) -> Result<Config> {
    let home = env::var_os("HOME").context("HOME is not set")?;
    let explicit_base_dir = options
        .base_dir
        .clone()
        .or_else(|| env::var_os("T3CODE_HOME").map(PathBuf::from));
    let home_base_dir = PathBuf::from(&home).join(".t3");
    let default_base_dir = explicit_base_dir
        .clone()
        .unwrap_or_else(|| home_base_dir.clone());
    let server = discover_server(&default_base_dir);
    let base_dir = explicit_base_dir
        .clone()
        .or_else(|| server.as_ref().and_then(|server| server.base_dir.clone()))
        .unwrap_or(default_base_dir);
    let server = server.as_ref().filter(|server| {
        explicit_base_dir.is_none()
            || server.base_dir.as_deref() == Some(base_dir.as_path())
            || (server.base_dir.is_none() && base_dir == home_base_dir)
    });
    let url = options
        .url
        .clone()
        .or_else(|| env::var("T3CODE_URL").ok())
        .or_else(|| runtime_origin(&base_dir))
        .or_else(|| server.and_then(|server| server.url.clone()))
        .unwrap_or_else(|| DEFAULT_URL.to_owned());
    let url = normalize_url(&url)?;
    let t3 = options
        .t3
        .clone()
        .or_else(|| env::var_os("T3CODE_BIN").map(PathBuf::from))
        .or_else(|| server.and_then(|server| server.exe.clone()))
        .or_else(|| active_version_binary(&base_dir))
        .unwrap_or_else(|| PathBuf::from("t3"));
    let token_source = if let Some(token) = options.token.clone().filter(|token| !token.is_empty())
    {
        TokenSource::Static(token)
    } else if let Some(path) = options.token_file.clone() {
        TokenSource::File(path)
    } else if let Some(token) = env::var("T3CODE_TOKEN")
        .ok()
        .filter(|token| !token.is_empty())
    {
        TokenSource::Static(token)
    } else {
        TokenSource::Issued
    };
    let mut ignored_threads = options.ignored_threads.clone();
    for variable in [
        "T3CODE_THREAD_ID",
        "T3_THREAD_ID",
        "T3CODE_CURRENT_THREAD_ID",
    ] {
        if let Ok(thread) = env::var(variable) {
            ignored_threads.push(thread);
        }
    }
    ignored_threads.retain(|thread| !thread.is_empty());
    ignored_threads.sort();
    ignored_threads.dedup();
    Ok(Config {
        url,
        base_dir,
        t3,
        token_source,
        token_ttl: options.token_ttl.clone(),
        ignored_threads,
    })
}

fn normalize_url(url: &str) -> Result<String> {
    let url = url.trim().trim_end_matches('/');
    if url.is_empty() {
        bail!("T3 server URL is empty");
    }
    let (scheme, rest) = match url.split_once("://") {
        Some((scheme, rest)) => (scheme, rest),
        None => ("http", url),
    };
    if scheme != "http" {
        bail!("this watchdog only supports local http:// T3 servers, not {scheme}://");
    }
    if rest.is_empty() {
        bail!("T3 server URL has no host");
    }
    Ok(format!("http://{rest}"))
}

fn runtime_origin(base_dir: &Path) -> Option<String> {
    let state = fs::read(base_dir.join("userdata/server-runtime.json")).ok()?;
    let value: Value = serde_json::from_slice(&state).ok()?;
    let origin = value.get("origin")?.as_str()?;
    normalize_url(origin).ok()
}

fn discover_server(base_dir: &Path) -> Option<ServerProcess> {
    let mut fallback = None;
    let entries = fs::read_dir("/proc").ok()?;
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let Some(args) = proc_cmdline(pid) else {
            continue;
        };
        if !args.iter().any(|arg| arg == "serve") {
            continue;
        }
        let exe = fs::read_link(format!("/proc/{pid}/exe")).ok();
        let is_t3 = exe
            .as_ref()
            .and_then(|path| path.file_name())
            .is_some_and(|name| name == "t3")
            || args
                .first()
                .and_then(|arg| Path::new(arg).file_name())
                .is_some_and(|name| name == "t3");
        if !is_t3 {
            continue;
        }
        let process_base_dir = parse_flag(&args, "--base-dir")
            .map(PathBuf::from)
            .or_else(|| proc_env(pid, "T3CODE_HOME").map(PathBuf::from));
        let candidate = ServerProcess {
            exe,
            url: server_url(&args),
            base_dir: process_base_dir,
        };
        if candidate.base_dir.as_deref() == Some(base_dir) {
            return Some(candidate);
        }
        if fallback.is_none() {
            fallback = Some(candidate);
        }
    }
    fallback
}

fn proc_cmdline(pid: u32) -> Option<Vec<String>> {
    let bytes = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let args = bytes
        .split(|byte| *byte == 0)
        .filter(|arg| !arg.is_empty())
        .map(|arg| String::from_utf8_lossy(arg).into_owned())
        .collect::<Vec<_>>();
    (!args.is_empty()).then_some(args)
}

fn proc_env(pid: u32, name: &str) -> Option<String> {
    let bytes = fs::read(format!("/proc/{pid}/environ")).ok()?;
    let prefix = format!("{name}=");
    bytes
        .split(|byte| *byte == 0)
        .filter_map(|entry| std::str::from_utf8(entry).ok())
        .find_map(|entry| entry.strip_prefix(&prefix).map(str::to_owned))
}

fn parse_flag(args: &[String], flag: &str) -> Option<String> {
    let prefix = format!("{flag}=");
    for (index, arg) in args.iter().enumerate() {
        if arg == flag {
            return args.get(index + 1).cloned();
        }
        if let Some(value) = arg.strip_prefix(&prefix) {
            return Some(value.to_owned());
        }
    }
    None
}

fn server_url(args: &[String]) -> Option<String> {
    let host = parse_flag(args, "--host").unwrap_or_else(|| "127.0.0.1".to_owned());
    let port = parse_flag(args, "--port")
        .unwrap_or_else(|| "3773".to_owned())
        .parse::<u16>()
        .ok()?;
    let host = match host.as_str() {
        "" | "0.0.0.0" => "127.0.0.1".to_owned(),
        "::" | "[::]" => "[::1]".to_owned(),
        host if host.contains(':') && !host.starts_with('[') => format!("[{host}]"),
        host => host.to_owned(),
    };
    Some(format!("http://{host}:{port}"))
}

fn active_version_binary(base_dir: &Path) -> Option<PathBuf> {
    let state = fs::read(base_dir.join("runtime/service-state.json")).ok()?;
    let value: Value = serde_json::from_slice(&state).ok()?;
    let version = value.get("activeVersion")?.as_str()?;
    let path = base_dir.join("runtime/versions").join(version).join("t3");
    path.is_file().then_some(path)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShellSnapshot {
    #[serde(default)]
    schema_version: Option<u64>,
    threads: Vec<ThreadShell>,
    archived_threads: Vec<ThreadShell>,
    #[serde(default)]
    projects: Vec<ProjectShell>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectShell {
    id: String,
    title: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadShell {
    id: String,
    project_id: String,
    title: String,
    status: String,
    #[serde(default)]
    activity_run_status: Option<String>,
    #[serde(default)]
    blocked_on_thread_wait: Option<bool>,
    #[serde(default)]
    pending_runtime_request: Option<PendingRuntimeRequest>,
    #[serde(default)]
    pending_background_tasks: Option<Vec<PendingBackgroundTask>>,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    deleted_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingRuntimeRequest {
    #[serde(default)]
    blocking: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingBackgroundTask {
    task_id: String,
    kind: String,
}

struct ThreadVerdict {
    id: String,
    title: String,
    project: String,
    label: &'static str,
    color: Color,
    blocker: Option<String>,
    created: Option<Timestamp>,
    updated: Option<Timestamp>,
    waiting: bool,
}

struct ScanEvaluation {
    excluded_threads: usize,
    rows: Vec<ThreadVerdict>,
    blockers: Vec<String>,
    api_error: Option<String>,
    api_stuck: bool,
}

impl ScanEvaluation {
    fn state(&self) -> &'static str {
        if self.blockers.is_empty() {
            "IDLE"
        } else {
            "BLOCKED"
        }
    }

    fn blocking_threads(&self) -> usize {
        self.rows.iter().filter(|row| row.blocker.is_some()).count()
    }

    fn waiting_threads(&self) -> usize {
        self.rows.iter().filter(|row| row.waiting).count()
    }
}

#[derive(Debug)]
enum ScanError {
    Unauthorized(String),
    Unreachable(String),
    Fatal(anyhow::Error),
}

enum AcquireError {
    Signal(i32),
    Unauthorized(String),
    Unreachable(String),
    Fatal(String),
}

fn is_transient(error: &ureq::Error) -> bool {
    match error {
        ureq::Error::Timeout(_)
        | ureq::Error::ConnectionFailed
        | ureq::Error::HostNotFound
        | ureq::Error::Io(_)
        | ureq::Error::Protocol(_) => true,
        ureq::Error::StatusCode(status) => *status == 429 || *status >= 500,
        _ => false,
    }
}

fn is_unreachable(error: &ureq::Error) -> bool {
    match error {
        ureq::Error::Timeout(_)
        | ureq::Error::ConnectionFailed
        | ureq::Error::HostNotFound
        | ureq::Error::Io(_) => true,
        _ => false,
    }
}

fn fetch_shell(
    client: &ureq::Agent,
    url: &str,
    token: &str,
) -> std::result::Result<ShellSnapshot, ScanError> {
    let endpoint = format!("{}/api/orchestration/shell", url.trim_end_matches('/'));
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        let request = client
            .get(&endpoint)
            .header("Accept", "application/json")
            .header("Authorization", format!("Bearer {token}"))
            .header("x-t3-orchestration-protocol", ORCHESTRATION_PROTOCOL);
        match request.call() {
            Ok(mut response) => {
                let body = response
                    .body_mut()
                    .read_to_string()
                    .with_context(|| format!("read GET {endpoint}"))
                    .map_err(ScanError::Fatal)?;
                let snapshot: ShellSnapshot = serde_json::from_str(&body)
                    .with_context(|| format!("parse GET {endpoint}"))
                    .map_err(ScanError::Fatal)?;
                if snapshot.schema_version != Some(2) {
                    return Err(ScanError::Fatal(anyhow::anyhow!(
                        "GET {endpoint} returned unsupported shell schema {:?}",
                        snapshot.schema_version
                    )));
                }
                return Ok(snapshot);
            }
            Err(ureq::Error::StatusCode(401)) => {
                return Err(ScanError::Unauthorized(format!("GET {endpoint}: 401")));
            }
            Err(error) if attempt < REQUEST_ATTEMPTS && is_transient(&error) => {
                thread::sleep(Duration::from_millis(500 * u64::from(attempt)));
            }
            Err(error) => {
                let unreachable = is_unreachable(&error);
                let error = anyhow::Error::new(error).context(format!("GET {endpoint}"));
                return Err(if unreachable {
                    ScanError::Unreachable(format!("{error:#}"))
                } else {
                    ScanError::Fatal(error)
                });
            }
        }
    }
}

fn run_fetch(
    dashboard: &Dashboard,
    cancellation: &Cancellation,
    client: &ureq::Agent,
    url: &str,
    token: &str,
    on_redraw: impl FnMut(),
) -> std::result::Result<ShellSnapshot, AcquireError> {
    let client = client.clone();
    let url = url.to_owned();
    let token = token.to_owned();
    match dashboard.run_work(cancellation, on_redraw, move || {
        fetch_shell(&client, &url, &token)
    }) {
        Ok(Ok(snapshot)) => Ok(snapshot),
        Ok(Err(ScanError::Unauthorized(message))) => Err(AcquireError::Unauthorized(message)),
        Ok(Err(ScanError::Unreachable(message))) => Err(AcquireError::Unreachable(message)),
        Ok(Err(ScanError::Fatal(error))) => Err(AcquireError::Fatal(format!("{error:#}"))),
        Err(signal) => Err(AcquireError::Signal(signal)),
    }
}

fn fetch_with_auth(
    auth: &mut Auth,
    report: &dyn Fn(String),
    mut fetch: impl FnMut(&str) -> std::result::Result<ShellSnapshot, AcquireError>,
) -> std::result::Result<ShellSnapshot, AcquireError> {
    if let Some(message) = auth
        .ensure_fresh()
        .map_err(|error| AcquireError::Fatal(format!("{error:#}")))?
    {
        report(message);
    }
    let token = auth
        .token()
        .map_err(|error| AcquireError::Fatal(format!("{error:#}")))?
        .to_owned();
    match fetch(&token) {
        Err(AcquireError::Unauthorized(message)) => {
            report(format!(
                "T3 rejected the bearer token ({message}); refreshing it"
            ));
            let message = auth
                .refresh_after_401()
                .map_err(|error| AcquireError::Fatal(format!("{error:#}")))?;
            report(message);
            let token = auth
                .token()
                .map_err(|error| AcquireError::Fatal(format!("{error:#}")))?
                .to_owned();
            fetch(&token)
        }
        result => result,
    }
}

struct ApiHealth {
    failure_since: Option<Instant>,
    stuck: bool,
    fatal: bool,
    fatal_reported: bool,
    stuck_after: Duration,
}

impl ApiHealth {
    fn new(stuck_after: Duration) -> Self {
        Self {
            failure_since: None,
            stuck: false,
            fatal: false,
            fatal_reported: false,
            stuck_after,
        }
    }

    fn apply(
        &mut self,
        result: std::result::Result<ShellSnapshot, AcquireError>,
        ignored_threads: &[String],
        report: &dyn Fn(String),
    ) -> std::result::Result<ScanEvaluation, i32> {
        match result {
            Ok(snapshot) => {
                self.failure_since = None;
                self.stuck = false;
                self.fatal = false;
                self.fatal_reported = false;
                Ok(evaluate(Some(&snapshot), None, ignored_threads, false))
            }
            Err(AcquireError::Signal(signal)) => Err(signal),
            Err(AcquireError::Unreachable(error)) => {
                let now = Instant::now();
                let started = match self.failure_since {
                    Some(started) => started,
                    None => {
                        self.failure_since = Some(now);
                        report(format!("T3 API scan failed: {error}"));
                        now
                    }
                };
                let elapsed = now.duration_since(started);
                let stuck =
                    !self.fatal && !self.stuck_after.is_zero() && elapsed >= self.stuck_after;
                if stuck && !self.stuck {
                    self.stuck = true;
                    report(format!(
                        "T3 API unavailable for {}s; no longer blocking idle",
                        elapsed.as_secs()
                    ));
                }
                Ok(evaluate(None, Some(error), ignored_threads, stuck))
            }
            Err(AcquireError::Unauthorized(error)) | Err(AcquireError::Fatal(error)) => {
                self.failure_since = None;
                self.stuck = false;
                self.fatal = true;
                if !self.fatal_reported {
                    self.fatal_reported = true;
                    report(format!("T3 API scan failed: {error}"));
                }
                Ok(evaluate(None, Some(error), ignored_threads, false))
            }
        }
    }
}

fn evaluate(
    snapshot: Option<&ShellSnapshot>,
    api_error: Option<String>,
    ignored_threads: &[String],
    api_stuck: bool,
) -> ScanEvaluation {
    let excluded = ignored_threads
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut rows = Vec::new();
    let mut blockers = Vec::new();
    if let Some(snapshot) = snapshot {
        let projects = snapshot
            .projects
            .iter()
            .map(|project| (project.id.as_str(), project.title.as_str()))
            .collect::<HashMap<_, _>>();
        let mut seen = HashSet::new();
        for thread in snapshot
            .threads
            .iter()
            .chain(snapshot.archived_threads.iter())
        {
            if !seen.insert(thread.id.as_str()) {
                continue;
            }
            if let Some(verdict) = classify_thread(thread, &excluded, &projects) {
                if let Some(blocker) = &verdict.blocker {
                    blockers.push(blocker.clone());
                }
                rows.push(verdict);
            }
        }
    }
    if let Some(error) = &api_error
        && !api_stuck
    {
        blockers.push(format!("T3 API: {error}"));
    }
    ScanEvaluation {
        excluded_threads: excluded.len(),
        rows,
        blockers,
        api_error,
        api_stuck,
    }
}

fn classify_thread(
    thread: &ThreadShell,
    excluded: &HashSet<&str>,
    projects: &HashMap<&str, &str>,
) -> Option<ThreadVerdict> {
    if thread.deleted_at.is_some() {
        return None;
    }
    let project = projects
        .get(thread.project_id.as_str())
        .map(|title| (*title).to_owned())
        .unwrap_or_else(|| short_id(&thread.project_id));
    let created = parse_timestamp(&thread.created_at);
    let updated = parse_timestamp(&thread.updated_at);
    let verdict = |label, color, blocker, waiting| ThreadVerdict {
        id: thread.id.clone(),
        title: thread.title.clone(),
        project: project.clone(),
        label,
        color,
        blocker,
        created,
        updated,
        waiting,
    };
    let id = short_id(&thread.id);
    let blocked =
        |label, color, reason: String| verdict(label, color, Some(format!("{id}:{reason}")), false);
    let queued = || blocked("QUEUED", Color::Yellow, "queued".to_owned());
    let waiting = || verdict("WAITING", Color::Blue, None, true);
    if excluded.contains(thread.id.as_str()) {
        return Some(verdict("IGNORED", Color::Cyan, None, false));
    }
    if let Some(task) = thread
        .pending_background_tasks
        .as_deref()
        .unwrap_or_default()
        .iter()
        .find(|task| !is_pull_request_watch(task))
    {
        return Some(blocked(
            "BACKGROUND",
            Color::Yellow,
            format!("background({})", task.kind),
        ));
    }
    let activity_status = thread.activity_run_status.as_deref();
    let status = activity_status.unwrap_or(&thread.status);
    let running = matches!(status, "preparing" | "starting" | "running");
    let waiting_on_thread = thread.blocked_on_thread_wait == Some(true);
    let waiting_on_request = thread
        .pending_runtime_request
        .as_ref()
        .is_some_and(|request| request.blocking == Some(true) || status == "waiting");
    // T3 marks provider-blocking requests explicitly; asynchronous message-based
    // questions do not stop a running agent. Missing flags do not prove a pause.
    // A genuine request wait also holds up queued follow-ups.
    if waiting_on_request {
        return Some(waiting());
    }
    if thread.status == "queued" && (!running || waiting_on_thread) {
        return Some(queued());
    }
    if waiting_on_thread {
        return Some(waiting());
    }
    if running {
        return Some(blocked("RUNNING", Color::Yellow, status.to_owned()));
    }
    match status {
        "waiting" if only_pull_request_watches(thread) => Some(waiting()),
        "waiting" => Some(blocked("WAITING?", Color::Red, "waiting".to_owned())),
        "idle" | "completed" | "interrupted" | "failed" | "cancelled" | "rolled_back"
            if activity_status.is_none() =>
        {
            None
        }
        other => {
            let source = if activity_status.is_some() {
                "activity"
            } else {
                "status"
            };
            Some(blocked("UNKNOWN", Color::Red, format!("{source}:{other}")))
        }
    }
}

fn is_pull_request_watch(task: &PendingBackgroundTask) -> bool {
    task.kind == "monitor" && task.task_id.starts_with("pull-request-watch:")
}

fn only_pull_request_watches(thread: &ThreadShell) -> bool {
    thread
        .pending_background_tasks
        .as_deref()
        .is_some_and(|tasks| !tasks.is_empty() && tasks.iter().all(is_pull_request_watch))
}

fn dashboard_status(
    evaluation: &ScanEvaluation,
    check: u32,
    idle: u32,
    required_idle: u32,
    next_scan: u64,
) -> String {
    format!(
        "\x1b[2mcheck #{check}  |\x1b[0m  {}  |  {} blocking  |  {} waiting  |  idle {idle}/{required_idle}  |  next scan in {next_scan}s",
        evaluation.state(),
        evaluation.blockers.len(),
        evaluation.waiting_threads(),
    )
}

fn draw_dashboard(
    dashboard: &Dashboard,
    evaluation: &ScanEvaluation,
    check: u32,
    idle: u32,
    required_idle: u32,
    next_scan: u64,
) {
    let mut rows = evaluation.rows.iter().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        let rank = |row: &ThreadVerdict| {
            if row.blocker.is_some() {
                0
            } else if row.waiting {
                1
            } else {
                2
            }
        };
        rank(left)
            .cmp(&rank(right))
            .then_with(|| right.updated.cmp(&left.updated))
    });
    let now = Timestamp::now();
    let rows = rows
        .into_iter()
        .map(|row| DashboardRow {
            status: row.label.to_owned(),
            color: row.color,
            age: elapsed(row.created, now),
            updated: elapsed(row.updated, now),
            scope: clean_text(&row.project),
            id: short_id(&row.id),
            title: clean_text(&row.title),
        })
        .collect();
    let extra = evaluation.api_error.as_ref().map(|error| {
        let mut section = if evaluation.api_stuck {
            "\x1b[33mT3 API unavailable (no longer blocking)\x1b[0m".to_owned()
        } else {
            "\x1b[31mT3 API errors\x1b[0m".to_owned()
        };
        section.push_str("\n  ");
        section.push_str(&clean_text(error));
        section
    });
    dashboard.draw(DashboardView {
        status: dashboard_status(evaluation, check, idle, required_idle, next_scan),
        rows,
        empty: (
            "IDLE".to_owned(),
            Color::Green,
            "No active threads".to_owned(),
        ),
        extra,
    });
}

fn wait_until_idle(
    state: &State,
    options: &Options,
    config: &Config,
    auth: &mut Auth,
    command: &[OsString],
    cancellation: &Cancellation,
) -> Result<i32> {
    let client = http_client();
    let dashboard = Dashboard::new(command, "T3 IDLE WATCHDOG", "Scanning threads...", HEADERS);
    let report = |message: String| {
        if dashboard.termios_active() {
            state.record(message);
        } else {
            state.log(message);
        }
    };
    if auth.is_issued() {
        report(auth.describe());
    }
    report(format!(
        "watchdog started; url={}; base_dir={}; t3={}; excluded_threads={}; interval={}s required_idle_checks={} stuck_after={}s token_source={}; command={}; cache={}",
        config.url,
        config.base_dir.display(),
        config.t3.display(),
        if config.ignored_threads.is_empty() {
            "none".to_owned()
        } else {
            config.ignored_threads.join(",")
        },
        options.interval,
        options.required_idle_checks,
        options.stuck_after,
        auth.source_label(),
        if command.is_empty() {
            "none".to_owned()
        } else {
            command_display(command)
        },
        state.root().display(),
    ));
    dashboard.loading();
    let mut api = ApiHealth::new(Duration::from_secs(options.stuck_after));
    let mut scan = |on_redraw: &mut dyn FnMut()| {
        let result = fetch_with_auth(auth, &report, |token| {
            run_fetch(
                &dashboard,
                cancellation,
                &client,
                &config.url,
                token,
                || on_redraw(),
            )
        });
        api.apply(result, &config.ignored_threads, &report)
    };
    let mut evaluation = {
        let mut on_redraw = || dashboard.loading();
        match scan(&mut on_redraw) {
            Ok(evaluation) => evaluation,
            Err(signal) => {
                drop(scan);
                drop(dashboard);
                signal_exit(state, signal);
            }
        }
    };

    let mut consecutive_idle = 0;
    let mut check = 0;
    let signal = 'checks: loop {
        check += 1;
        consecutive_idle = if evaluation.blockers.is_empty() {
            consecutive_idle + 1
        } else {
            0
        };
        report(format!(
            "check {check}: idle={consecutive_idle}/{}; blockers={}",
            options.required_idle_checks,
            if evaluation.blockers.is_empty() {
                "none"
            } else {
                "t3 API or thread status"
            }
        ));
        if !evaluation.blockers.is_empty() {
            report(format!(
                "t3_api_blockers: {}",
                summarize(&evaluation.blockers, 5)
            ));
        }
        let draw = |next_scan| {
            draw_dashboard(
                &dashboard,
                &evaluation,
                check,
                consecutive_idle,
                options.required_idle_checks,
                next_scan,
            );
        };
        draw(options.interval);
        if let Some(signal) =
            dashboard.wait(cancellation, Duration::ZERO, || draw(options.interval))
        {
            break Some(signal);
        }
        if consecutive_idle >= options.required_idle_checks {
            let mut on_redraw = || draw(options.interval);
            match scan(&mut on_redraw) {
                Ok(final_evaluation) => {
                    if final_evaluation.blockers.is_empty() {
                        break None;
                    }
                    evaluation = final_evaluation;
                    consecutive_idle = 0;
                    continue;
                }
                Err(signal) => break Some(signal),
            }
        }
        let mut remaining = options.interval;
        while remaining > 0 {
            if let Some(signal) =
                dashboard.wait(cancellation, Duration::from_secs(1), || draw(remaining))
            {
                break 'checks Some(signal);
            }
            remaining -= 1;
            dashboard.tick(&dashboard_status(
                &evaluation,
                check,
                consecutive_idle,
                options.required_idle_checks,
                remaining,
            ));
        }
        let mut on_redraw = || draw(0);
        match scan(&mut on_redraw) {
            Ok(next_evaluation) => evaluation = next_evaluation,
            Err(signal) => break Some(signal),
        }
    };

    drop(scan);
    drop(dashboard);
    if let Some(signal) = signal {
        signal_exit(state, signal);
    }
    state.log("idle confirmed");
    if let Some(signal) = cancellation.wait(Duration::ZERO) {
        signal_exit(state, signal);
    }
    if command.is_empty() {
        Ok(0)
    } else {
        Ok(run_command(state, cancellation, command))
    }
}

fn check_once(
    state: &State,
    config: &Config,
    auth: &mut Auth,
    cancellation: &Cancellation,
) -> Result<i32> {
    if auth.is_issued() {
        state.log(auth.describe());
    }
    let client = http_client();
    let report = |message: String| state.log(message);
    let snapshot = match acquire_snapshot_sync(auth, &client, &config.url, &report) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            state.log(format!("T3 API scan failed: {error}"));
            eprintln!("{NAME}: {error}");
            return Ok(1);
        }
    };
    if let Some(signal) = cancellation.wait(Duration::ZERO) {
        signal_exit(state, signal);
    }
    let evaluation = evaluate(Some(&snapshot), None, &config.ignored_threads, false);
    print_once(&evaluation);
    Ok(if evaluation.blockers.is_empty() { 0 } else { 2 })
}

fn acquire_snapshot_sync(
    auth: &mut Auth,
    client: &ureq::Agent,
    url: &str,
    report: &dyn Fn(String),
) -> std::result::Result<ShellSnapshot, String> {
    fetch_with_auth(auth, report, |token| {
        fetch_shell(client, url, token).map_err(|error| match error {
            ScanError::Unauthorized(message) => AcquireError::Unauthorized(message),
            ScanError::Unreachable(message) => AcquireError::Unreachable(message),
            ScanError::Fatal(error) => AcquireError::Fatal(format!("{error:#}")),
        })
    })
    .map_err(|error| match error {
        AcquireError::Signal(signal) => format!("interrupted by signal {signal}"),
        AcquireError::Unauthorized(message) => {
            format!("T3 rejected the refreshed bearer token: {message}")
        }
        AcquireError::Unreachable(message) | AcquireError::Fatal(message) => message,
    })
}

fn print_once(evaluation: &ScanEvaluation) {
    println!("{NAME}: {}", evaluation.state());
    println!(
        "blocking={} threads={} waiting={} excluded={}",
        evaluation.blockers.len(),
        evaluation.blocking_threads(),
        evaluation.waiting_threads(),
        evaluation.excluded_threads
    );
    println!(
        "{:<10} {:<8} {:<8} {:<20} {:<8} TITLE",
        "STATUS", "AGE", "UPDATED", "PROJECT", "THREAD"
    );
    let now = Timestamp::now();
    for row in &evaluation.rows {
        println!(
            "{:<10} {:<8} {:<8} {:<20} {:<8} {}",
            row.label,
            elapsed(row.created, now),
            elapsed(row.updated, now),
            clean_text(&row.project),
            short_id(&row.id),
            clean_text(&row.title),
        );
    }
    if evaluation.rows.is_empty() {
        println!("(no active threads)");
    }
}

fn execute() -> Result<i32> {
    let options = Options::parse();
    let state = State::discover(NAME)?;
    let config = resolve_config(&options)?;
    let command = resolve_command(&options.command);
    let cancellation = Cancellation::new()?;
    let mut auth = Auth::new(&config, state.root())?;
    if let Some(signal) = cancellation.wait(Duration::ZERO) {
        signal_exit(&state, signal);
    }
    if options.once {
        return check_once(&state, &config, &mut auth, &cancellation);
    }
    wait_until_idle(
        &state,
        &options,
        &config,
        &mut auth,
        &command,
        &cancellation,
    )
}

fn main() {
    exit_with(NAME, execute());
}

fn summarize_text(value: &str, limit: usize) -> String {
    let value = clean_text(value);
    match value.char_indices().nth(limit) {
        Some((end, _)) => format!("{}...", &value[..end]),
        None => value,
    }
}

fn elapsed(time: Option<Timestamp>, now: Timestamp) -> String {
    let Some(time) = time else {
        return "-".to_owned();
    };
    let seconds = now.as_second().saturating_sub(time.as_second()).max(0) as u64;
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m {:02}s", seconds / 60, seconds % 60),
        3600..86400 => format!("{}h {:02}m", seconds / 3600, seconds / 60 % 60),
        _ => format!("{}d {:02}h", seconds / 86400, seconds / 3600 % 24),
    }
}

fn short_id(value: &str) -> String {
    value.chars().take(8).collect()
}

fn expires_soon(expires_at: Timestamp, margin: Duration) -> bool {
    expires_at
        .as_second()
        .saturating_sub(Timestamp::now().as_second())
        <= margin.as_secs() as i64
}

fn parse_timestamp(value: &str) -> Option<Timestamp> {
    value.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn thread(id: &str, status: &str) -> ThreadShell {
        ThreadShell {
            id: id.to_owned(),
            project_id: "project".to_owned(),
            title: "Thread".to_owned(),
            status: status.to_owned(),
            activity_run_status: None,
            blocked_on_thread_wait: None,
            pending_runtime_request: None,
            pending_background_tasks: None,
            created_at: "2026-10-21T10:00:00Z".to_owned(),
            updated_at: "2026-10-21T10:00:01Z".to_owned(),
            deleted_at: None,
        }
    }

    fn classify(thread: &ThreadShell) -> Option<ThreadVerdict> {
        classify_thread(thread, &HashSet::new(), &HashMap::new())
    }

    fn serve_response(status: &str, body: &str) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        let status = status.to_owned();
        let body = body.to_owned();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept test request");
            let mut buffer = [0_u8; 8192];
            stream.read(&mut buffer).expect("read test request");
            let response = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write test response");
        });
        (format!("http://{address}"), server)
    }

    #[test]
    fn running_activity_blocks_even_when_latest_status_is_queued() {
        let mut thread = thread("running-thread", "queued");
        thread.activity_run_status = Some("running".to_owned());
        let verdict = classify(&thread).expect("active thread has a verdict");
        assert_eq!(verdict.label, "RUNNING");
        assert!(verdict.blocker.is_some());
    }

    #[test]
    fn queued_run_without_activity_blocks() {
        let thread = thread("queued-thread", "queued");
        let verdict = classify(&thread).expect("queued thread has a verdict");
        assert_eq!(verdict.label, "QUEUED");
        assert!(verdict.blocker.is_some());
    }

    #[test]
    fn queued_run_blocks_even_while_waiting_on_another_thread() {
        let mut thread = thread("queued-wait-thread", "queued");
        thread.activity_run_status = Some("running".to_owned());
        thread.blocked_on_thread_wait = Some(true);
        let verdict = classify(&thread).expect("queued thread has a verdict");
        assert_eq!(verdict.label, "QUEUED");
        assert!(verdict.blocker.is_some());
        assert!(!verdict.waiting);
    }

    #[test]
    fn queued_run_blocks_even_with_only_a_pr_watch_waiting() {
        let mut thread = thread("queued-pr-thread", "queued");
        thread.activity_run_status = Some("waiting".to_owned());
        thread.pending_background_tasks = Some(vec![PendingBackgroundTask {
            task_id: "pull-request-watch:org/repo#1".to_owned(),
            kind: "monitor".to_owned(),
        }]);
        let verdict = classify(&thread).expect("queued thread has a verdict");
        assert_eq!(verdict.label, "QUEUED");
        assert!(verdict.blocker.is_some());
        assert!(!verdict.waiting);
    }

    #[test]
    fn pending_runtime_request_is_waiting_without_blocking() {
        let mut thread = thread("waiting-thread", "waiting");
        thread.activity_run_status = Some("waiting".to_owned());
        thread.pending_runtime_request = Some(PendingRuntimeRequest {
            blocking: Some(true),
        });
        let verdict = classify(&thread).expect("waiting thread has a verdict");
        assert_eq!(verdict.label, "WAITING");
        assert!(verdict.waiting);
        assert!(verdict.blocker.is_none());
    }

    #[test]
    fn queued_follow_up_does_not_override_a_blocking_runtime_request() {
        for activity in [None, Some("running"), Some("waiting")] {
            let mut thread = thread("queued-question-thread", "queued");
            thread.activity_run_status = activity.map(str::to_owned);
            thread.pending_runtime_request = Some(PendingRuntimeRequest {
                blocking: Some(true),
            });
            let mut snapshot = ShellSnapshot {
                schema_version: Some(2),
                threads: vec![thread],
                archived_threads: Vec::new(),
                projects: Vec::new(),
            };
            let evaluation = evaluate(Some(&snapshot), None, &[], false);
            assert_eq!(evaluation.state(), "IDLE");
            assert_eq!(evaluation.waiting_threads(), 1);
            assert_eq!(evaluation.rows[0].label, "WAITING");

            snapshot.threads[0].pending_runtime_request = None;
            assert!(
                !evaluate(Some(&snapshot), None, &[], false)
                    .blockers
                    .is_empty()
            );
        }
    }

    #[test]
    fn non_blocking_runtime_request_does_not_hide_queued_work() {
        for activity in [None, Some("preparing"), Some("starting"), Some("running")] {
            for blocking in [None, Some(false)] {
                let mut thread = thread("queued-async-question-thread", "queued");
                thread.activity_run_status = activity.map(str::to_owned);
                thread.pending_runtime_request = Some(PendingRuntimeRequest { blocking });
                let verdict = classify(&thread).expect("queued thread has a verdict");
                assert!(verdict.blocker.is_some());
                assert!(!verdict.waiting);
            }
        }
    }

    #[test]
    fn runtime_request_without_a_blocking_flag_requires_a_stopped_agent() {
        for activity in [None, Some("waiting")] {
            let mut thread = thread("waiting-thread", "waiting");
            thread.activity_run_status = activity.map(str::to_owned);
            thread.pending_runtime_request = Some(PendingRuntimeRequest { blocking: None });
            let verdict = classify(&thread).expect("waiting thread has a verdict");
            assert_eq!(verdict.label, "WAITING");
            assert!(verdict.blocker.is_none());
        }
        let mut thread = thread("running-thread", "running");
        thread.pending_runtime_request = Some(PendingRuntimeRequest { blocking: None });
        assert!(classify(&thread).unwrap().blocker.is_some());
    }

    #[test]
    fn background_work_still_blocks_during_a_runtime_request() {
        let mut thread = thread("background-question-thread", "queued");
        thread.activity_run_status = Some("running".to_owned());
        thread.pending_runtime_request = Some(PendingRuntimeRequest {
            blocking: Some(true),
        });
        thread.pending_background_tasks = Some(vec![PendingBackgroundTask {
            task_id: "task-1".to_owned(),
            kind: "command".to_owned(),
        }]);
        let verdict = classify(&thread).expect("background thread has a verdict");
        assert_eq!(verdict.label, "BACKGROUND");
        assert!(verdict.blocker.is_some());
    }

    #[test]
    fn blocked_thread_wait_is_waiting_without_blocking() {
        let mut thread = thread("child-wait-thread", "running");
        thread.activity_run_status = Some("running".to_owned());
        thread.blocked_on_thread_wait = Some(true);
        let verdict = classify(&thread).expect("waiting thread has a verdict");
        assert_eq!(verdict.label, "WAITING");
        assert!(verdict.waiting);
        assert!(verdict.blocker.is_none());
    }

    #[test]
    fn non_blocking_runtime_request_does_not_hide_running_work() {
        let mut thread = thread("async-question-thread", "running");
        thread.activity_run_status = Some("running".to_owned());
        thread.pending_runtime_request = Some(PendingRuntimeRequest {
            blocking: Some(false),
        });
        let verdict = classify(&thread).expect("running thread has a verdict");
        assert_eq!(verdict.label, "RUNNING");
        assert!(verdict.blocker.is_some());
    }

    #[test]
    fn background_work_blocks_after_the_latest_run_settles() {
        let mut thread = thread("background-thread", "completed");
        thread.pending_background_tasks = Some(vec![PendingBackgroundTask {
            task_id: "task-1".to_owned(),
            kind: "command".to_owned(),
        }]);
        let verdict = classify(&thread).expect("background thread has a verdict");
        assert_eq!(verdict.label, "BACKGROUND");
        assert!(verdict.blocker.is_some());
    }

    #[test]
    fn pull_request_watch_is_not_blocking_background_work() {
        let mut thread = thread("pr-watch-thread", "completed");
        thread.pending_background_tasks = Some(vec![PendingBackgroundTask {
            task_id: "pull-request-watch:org/repo#1".to_owned(),
            kind: "monitor".to_owned(),
        }]);
        assert!(classify(&thread).is_none());
    }

    #[test]
    fn pull_request_watch_waiting_is_non_blocking() {
        let mut thread = thread("pr-watch-wait-thread", "waiting");
        thread.activity_run_status = Some("waiting".to_owned());
        thread.pending_background_tasks = Some(vec![PendingBackgroundTask {
            task_id: "pull-request-watch:org/repo#1".to_owned(),
            kind: "monitor".to_owned(),
        }]);
        let verdict = classify(&thread).expect("PR watch wait has a verdict");
        assert_eq!(verdict.label, "WAITING");
        assert!(verdict.waiting);
        assert!(verdict.blocker.is_none());
    }

    #[test]
    fn self_thread_is_excluded() {
        let mut thread = thread("self-thread", "running");
        thread.activity_run_status = Some("running".to_owned());
        let excluded = HashSet::from(["self-thread"]);
        let verdict = classify_thread(&thread, &excluded, &HashMap::new())
            .expect("self thread has a verdict");
        assert_eq!(verdict.label, "IGNORED");
        assert!(verdict.blocker.is_none());
    }

    #[test]
    fn deleted_threads_are_ignored() {
        let mut thread = thread("deleted-thread", "running");
        thread.deleted_at = Some("2026-10-21T10:00:02Z".to_owned());
        assert!(classify(&thread).is_none());
    }

    #[test]
    fn unknown_waiting_activity_blocks() {
        let mut thread = thread("unknown-wait-thread", "waiting");
        thread.activity_run_status = Some("waiting".to_owned());
        let verdict = classify(&thread).expect("waiting thread has a verdict");
        assert_eq!(verdict.label, "WAITING?");
        assert!(verdict.blocker.is_some());
    }

    #[test]
    fn evaluate_counts_blockers_and_waiting_threads() {
        let mut running = thread("running-thread", "running");
        running.activity_run_status = Some("running".to_owned());
        let mut waiting = thread("waiting-thread", "waiting");
        waiting.activity_run_status = Some("waiting".to_owned());
        waiting.pending_runtime_request = Some(PendingRuntimeRequest {
            blocking: Some(true),
        });
        let snapshot = ShellSnapshot {
            schema_version: Some(2),
            threads: vec![running, waiting],
            archived_threads: Vec::new(),
            projects: Vec::new(),
        };
        let evaluation = evaluate(Some(&snapshot), None, &[], false);
        assert_eq!(evaluation.blocking_threads(), 1);
        assert_eq!(evaluation.waiting_threads(), 1);
        assert_eq!(evaluation.blockers.len(), 1);
    }

    #[test]
    fn api_error_blocks_unless_stuck() {
        let evaluation = evaluate(None, Some("connection refused".to_owned()), &[], false);
        assert_eq!(evaluation.blockers.len(), 1);
        let evaluation = evaluate(None, Some("connection refused".to_owned()), &[], true);
        assert!(evaluation.blockers.is_empty());
        assert!(evaluation.api_stuck);
    }

    #[test]
    fn api_health_reports_failure_once_and_sticks_after_threshold() {
        let messages = RefCell::new(Vec::new());
        let report = |message: String| messages.borrow_mut().push(message);
        let mut health = ApiHealth::new(Duration::from_secs(1));

        let evaluation = health
            .apply(
                Err(AcquireError::Unreachable("down".to_owned())),
                &[],
                &report,
            )
            .expect("API failure does not signal");
        assert!(!evaluation.api_stuck);
        assert_eq!(messages.borrow().len(), 1);

        health.failure_since = Some(Instant::now() - Duration::from_secs(2));
        let evaluation = health
            .apply(
                Err(AcquireError::Unreachable("down".to_owned())),
                &[],
                &report,
            )
            .expect("API failure does not signal");
        assert!(evaluation.api_stuck);
        assert_eq!(messages.borrow().len(), 2);

        let snapshot = ShellSnapshot {
            schema_version: Some(2),
            threads: Vec::new(),
            archived_threads: Vec::new(),
            projects: Vec::new(),
        };
        health
            .apply(Ok(snapshot), &[], &report)
            .expect("successful scan does not signal");
        assert!(health.failure_since.is_none());
        assert!(!health.stuck);
        assert_eq!(messages.borrow().len(), 2);
    }

    #[test]
    fn api_health_never_uses_stuck_escape_for_fatal_errors() {
        let messages = RefCell::new(Vec::new());
        let report = |message: String| messages.borrow_mut().push(message);
        let mut health = ApiHealth::new(Duration::from_secs(1));
        health.failure_since = Some(Instant::now() - Duration::from_secs(2));

        let evaluation = health
            .apply(
                Err(AcquireError::Fatal("invalid token".to_owned())),
                &[],
                &report,
            )
            .expect("fatal API error does not signal");
        assert!(!evaluation.api_stuck);
        assert_eq!(evaluation.blockers.len(), 1);
        assert!(health.fatal);
        assert!(health.failure_since.is_none());

        health.failure_since = Some(Instant::now() - Duration::from_secs(2));
        let evaluation = health
            .apply(
                Err(AcquireError::Unreachable("down".to_owned())),
                &[],
                &report,
            )
            .expect("API failure does not signal");
        assert!(!evaluation.api_stuck);

        let snapshot = ShellSnapshot {
            schema_version: Some(2),
            threads: Vec::new(),
            archived_threads: Vec::new(),
            projects: Vec::new(),
        };
        health
            .apply(Ok(snapshot), &[], &report)
            .expect("successful scan does not signal");
        assert!(!health.fatal);

        health.failure_since = Some(Instant::now() - Duration::from_secs(2));
        let evaluation = health
            .apply(
                Err(AcquireError::Unreachable("down".to_owned())),
                &[],
                &report,
            )
            .expect("API failure does not signal");
        assert!(evaluation.api_stuck);
    }

    #[test]
    fn shell_snapshot_requires_thread_arrays() {
        for value in [
            serde_json::json!({"schemaVersion": 2}),
            serde_json::json!({"schemaVersion": 2, "threads": []}),
            serde_json::json!({"schemaVersion": 2, "archivedThreads": []}),
        ] {
            assert!(serde_json::from_value::<ShellSnapshot>(value).is_err());
        }
    }

    #[test]
    fn parses_rfc3339_utc_and_offsets() {
        assert_eq!(
            parse_timestamp("1970-01-01T00:00:00Z").map(Timestamp::as_second),
            Some(0)
        );
        let timestamp = parse_timestamp("2026-10-21T10:13:42.675Z").expect("parse timestamp");
        assert_eq!(timestamp.as_second(), 1_792_577_622);
        assert_eq!(timestamp.subsec_nanosecond(), 675_000_000);
        assert_eq!(
            parse_timestamp("2026-10-21T10:13:42+02:00").map(Timestamp::as_second),
            Some(1_792_570_422)
        );
        assert_eq!(parse_timestamp("not a timestamp"), None);
    }

    #[test]
    fn parses_server_url_from_arguments() {
        let args = vec![
            "t3".to_owned(),
            "serve".to_owned(),
            "--host".to_owned(),
            "0.0.0.0".to_owned(),
            "--port".to_owned(),
            "4000".to_owned(),
        ];
        assert_eq!(server_url(&args).as_deref(), Some("http://127.0.0.1:4000"));
        let args = vec![
            "t3".to_owned(),
            "serve".to_owned(),
            "--host=::".to_owned(),
            "--port=4001".to_owned(),
        ];
        assert_eq!(server_url(&args).as_deref(), Some("http://[::1]:4001"));
        let args = vec![
            "t3".to_owned(),
            "serve".to_owned(),
            "--host=[::]".to_owned(),
            "--port=4002".to_owned(),
        ];
        assert_eq!(server_url(&args).as_deref(), Some("http://[::1]:4002"));
    }

    #[test]
    fn parses_issued_token_without_leaking_it() {
        let value = serde_json::json!({
            "token": "secret-token",
            "sessionId": "session-id",
            "expiresAt": "2026-10-21T10:13:42.675Z"
        });
        let issued = parse_issued_token(&value).expect("token parses");
        assert_eq!(issued.token, "secret-token");
        assert_eq!(issued.session_id, "session-id");
        assert_eq!(
            issued.expires_at,
            "2026-10-21T10:13:42.675Z".parse::<Timestamp>().unwrap()
        );
    }

    #[test]
    fn issued_token_requires_a_session_id() {
        let value = serde_json::json!({
            "token": "secret-token",
            "expiresAt": "2026-10-21T10:13:42.675Z"
        });
        assert!(parse_issued_token(&value).is_err());
    }

    fn cached_token(expires_at: &str) -> CachedToken {
        CachedToken {
            base_dir: PathBuf::from("/tmp/t3"),
            ttl: "12h".to_owned(),
            token: "cached-token".to_owned(),
            session_id: "cached-session".to_owned(),
            expires_at: expires_at.parse().expect("parse expiry"),
        }
    }

    #[test]
    fn reuses_fresh_cached_token_without_running_the_cli() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let base_dir = directory.path().join("base");
        let config = Config {
            url: "http://127.0.0.1:3773".to_owned(),
            base_dir: base_dir.clone(),
            t3: PathBuf::from("t3-must-not-run"),
            token_source: TokenSource::Issued,
            token_ttl: "12h".to_owned(),
            ignored_threads: Vec::new(),
        };
        let cached = CachedToken {
            base_dir: base_dir.clone(),
            ..cached_token("2099-01-01T00:00:00Z")
        };
        fs::write(
            directory.path().join("token.json"),
            serde_json::to_vec(&cached).expect("serialize cached token"),
        )
        .expect("write cached token");

        let auth = Auth::new(&config, directory.path()).expect("load cached token");
        assert_eq!(auth.token().expect("cached token"), "cached-token");
        assert_eq!(auth.session_id.as_deref(), Some("cached-session"));
        assert!(auth.cached);
    }

    #[test]
    fn ignores_expired_or_foreign_cached_tokens() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let cache_file = directory.path().join("token.json");
        for token in [
            cached_token("2000-01-01T00:00:00Z"),
            CachedToken {
                base_dir: PathBuf::from("/tmp/other"),
                ..cached_token("2099-01-01T00:00:00Z")
            },
            CachedToken {
                ttl: "1h".to_owned(),
                ..cached_token("2099-01-01T00:00:00Z")
            },
        ] {
            fs::write(
                &cache_file,
                serde_json::to_vec(&token).expect("serialize cached token"),
            )
            .expect("write cached token");
            assert!(read_cached_token(&cache_file, Path::new("/tmp/t3"), "12h").is_none());
        }
    }

    #[test]
    fn command_output_timeout_kills_child() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 5"]);
        let error = run_output_timeout(command, Duration::from_millis(100))
            .expect_err("command should time out");
        assert!(error.to_string().contains("timed out"));
    }

    #[test]
    fn status_timeout_returns_exit_status() {
        let mut command = Command::new("sh");
        command.args(["-c", "exit 7"]);
        let status = run_status_timeout(command, Duration::from_secs(1)).expect("child exits");
        assert_eq!(status.code(), Some(7));
    }

    #[test]
    fn only_transport_failures_are_unreachable() {
        assert!(is_unreachable(&ureq::Error::HostNotFound));
        assert!(is_unreachable(&ureq::Error::ConnectionFailed));
        assert!(!is_unreachable(&ureq::Error::StatusCode(500)));
        assert!(!is_unreachable(&ureq::Error::StatusCode(429)));
    }

    #[test]
    fn fetch_shell_maps_unauthorized() {
        let (url, server) = serve_response("401 Unauthorized", "");
        let client = http_client();
        let result = fetch_shell(&client, &url, "test-token");
        assert!(matches!(result, Err(ScanError::Unauthorized(_))));
        server.join().expect("test server finishes");
    }

    #[test]
    fn fetch_shell_maps_truncated_json_as_fatal() {
        let (url, server) = serve_response("200 OK", r#"{"schemaVersion":2,"threads":[]"#);
        let client = http_client();
        let result = fetch_shell(&client, &url, "test-token");
        assert!(matches!(result, Err(ScanError::Fatal(_))));
        server.join().expect("test server finishes");
    }

    #[test]
    fn fetch_shell_sends_auth_and_protocol_headers() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept test request");
            let mut buffer = [0_u8; 8192];
            let read = stream.read(&mut buffer).expect("read test request");
            let request = String::from_utf8_lossy(&buffer[..read]).to_ascii_lowercase();
            assert!(request.contains("get /api/orchestration/shell"));
            assert!(request.contains("authorization: bearer test-token"));
            assert!(request.contains("x-t3-orchestration-protocol: 2"));
            let body = r#"{"schemaVersion":2,"snapshotSequence":1,"threads":[],"archivedThreads":[],"projects":[]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write test response");
        });
        let client = http_client();
        let snapshot = fetch_shell(&client, &format!("http://{address}"), "test-token")
            .expect("fetch shell snapshot");
        assert!(snapshot.threads.is_empty());
        server.join().expect("test server finishes");
    }
}
