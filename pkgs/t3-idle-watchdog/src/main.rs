//! Run a command after every other T3 Code thread has gone idle.

use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::Parser;
use comfy_table::{Cell, CellAlignment, Color, ContentArrangement, Table, presets::UTF8_FULL};
use jiff::{Timestamp, Zoned};
use serde::Deserialize;
use serde_json::Value;
use signal_hook::{
    consts::{SIGINT, SIGTERM},
    iterator::{Handle as SignalHandle, Signals},
};

const NAME: &str = "t3-idle-watchdog";
const DEFAULT_URL: &str = "http://127.0.0.1:3773";
const DEFAULT_INTERVAL: u64 = 30;
const DEFAULT_IDLE_CHECKS: u32 = 3;
const DEFAULT_TOKEN_TTL: &str = "12h";
const REQUEST_TIMEOUT_SECS: u64 = 10;
const REQUEST_ATTEMPTS: u32 = 3;
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

struct State {
    root: PathBuf,
    log_file: PathBuf,
}

impl State {
    fn discover() -> Result<Self> {
        let home = env::var_os("HOME").context("HOME is not set")?;
        let cache = env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(home).join(".cache"));
        let root = cache.join(NAME);
        Ok(Self {
            log_file: root.join("watchdog.log"),
            root,
        })
    }

    fn log(&self, message: impl AsRef<str>) {
        let line = self.timestamped(message);
        println!("{line}");
        let _ = io::stdout().flush();
        self.write_log(&line);
    }

    fn record(&self, message: impl AsRef<str>) {
        self.write_log(&self.timestamped(message));
    }

    fn timestamped(&self, message: impl AsRef<str>) -> String {
        format!(
            "[{}] {}",
            Zoned::now().strftime("%Y-%m-%d %H:%M:%S"),
            message.as_ref()
        )
    }

    fn write_log(&self, line: &str) {
        if fs::create_dir_all(&self.root).is_ok()
            && let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.log_file)
        {
            let _ = writeln!(file, "{line}");
        }
    }
}

struct Cancellation {
    receiver: Receiver<i32>,
    handle: SignalHandle,
    worker: Option<JoinHandle<()>>,
}

impl Cancellation {
    fn new() -> Result<Self> {
        let mut signals = Signals::new([SIGINT, SIGTERM]).context("install signal handlers")?;
        let handle = signals.handle();
        let (sender, receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            for signal in signals.forever() {
                if sender.send(signal).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            receiver,
            handle,
            worker: Some(worker),
        })
    }

    fn wait(&self, duration: Duration) -> Option<i32> {
        self.receiver.recv_timeout(duration).ok()
    }
}

impl Drop for Cancellation {
    fn drop(&mut self) {
        self.handle.close();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    Quit,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Ignore,
}

struct Dashboard {
    termios: Option<libc::termios>,
    action: Option<String>,
    terminal_size: std::cell::Cell<(u16, u16)>,
    scroll: std::cell::Cell<usize>,
    page: std::cell::Cell<usize>,
    max_scroll: std::cell::Cell<usize>,
}

impl Dashboard {
    fn terminal_size() -> (u16, u16) {
        let mut size = unsafe { std::mem::zeroed::<libc::winsize>() };
        if unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut size) } == 0 {
            (
                if size.ws_col > 0 { size.ws_col } else { 80 },
                if size.ws_row > 0 { size.ws_row } else { 24 },
            )
        } else {
            (80, 24)
        }
    }

    fn new(command: &[OsString]) -> Self {
        let interactive = io::stdin().is_terminal() && io::stdout().is_terminal();
        let mut original = unsafe { std::mem::zeroed::<libc::termios>() };
        let termios =
            if interactive && unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut original) } == 0 {
                let mut cbreak = original;
                cbreak.c_lflag &= !(libc::ICANON | libc::ECHO);
                cbreak.c_cc[libc::VMIN] = 0;
                cbreak.c_cc[libc::VTIME] = 0;
                (unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &cbreak) } == 0)
                    .then_some(original)
            } else {
                None
            };
        if termios.is_some() {
            print!("\x1b[?1049h\x1b[?25l\x1b[?7l");
            let _ = io::stdout().flush();
        }
        let action = (!command.is_empty()).then(|| command_display(command));
        Self {
            termios,
            action,
            terminal_size: std::cell::Cell::new(Self::terminal_size()),
            scroll: std::cell::Cell::new(0),
            page: std::cell::Cell::new(1),
            max_scroll: std::cell::Cell::new(0),
        }
    }

    fn render(&self, frame: &str) {
        if self.termios.is_none() {
            return;
        }
        let mut redraw = String::new();
        for (row, line) in frame
            .lines()
            .take(usize::from(self.terminal_size.get().1))
            .enumerate()
        {
            write!(redraw, "\x1b[{};1H\x1b[2K{line}", row + 1)
                .expect("writing to a string cannot fail");
        }
        redraw.push_str("\x1b[J\x1b[H");
        print!("{redraw}");
        let _ = io::stdout().flush();
    }

    fn loading(&self) {
        self.render("\x1b[1;36mT3 IDLE WATCHDOG\x1b[0m\n\n\x1b[2mScanning threads...\x1b[0m");
    }

    fn read_byte() -> Option<u8> {
        let mut byte = 0_u8;
        (unsafe { libc::read(libc::STDIN_FILENO, std::ptr::from_mut(&mut byte).cast(), 1) } == 1)
            .then_some(byte)
    }

    fn stdin_ready(timeout_ms: i32) -> bool {
        let mut descriptor = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        unsafe { libc::poll(&mut descriptor, 1, timeout_ms) > 0 }
    }

    fn read_key() -> Option<Key> {
        match Self::read_byte()? {
            b'q' => Some(Key::Quit),
            b'k' => Some(Key::Up),
            b'j' => Some(Key::Down),
            0x1b => {
                if !Self::stdin_ready(10) {
                    return Some(Key::Ignore);
                }
                let Some(second) = Self::read_byte() else {
                    return Some(Key::Ignore);
                };
                if !matches!(second, b'[' | b'O') {
                    return Some(Key::Ignore);
                }
                if !Self::stdin_ready(10) {
                    return Some(Key::Ignore);
                }
                let Some(third) = Self::read_byte() else {
                    return Some(Key::Ignore);
                };
                match third {
                    b'A' => Some(Key::Up),
                    b'B' => Some(Key::Down),
                    b'H' => Some(Key::Home),
                    b'F' => Some(Key::End),
                    b'1'..=b'9' => {
                        if !Self::stdin_ready(10) {
                            return Some(Key::Ignore);
                        }
                        let Some(fourth) = Self::read_byte() else {
                            return Some(Key::Ignore);
                        };
                        match (third, fourth) {
                            (b'1', b'~') | (b'7', b'~') => Some(Key::Home),
                            (b'4', b'~') | (b'8', b'~') => Some(Key::End),
                            (b'5', b'~') => Some(Key::PageUp),
                            (b'6', b'~') => Some(Key::PageDown),
                            _ => Some(Key::Ignore),
                        }
                    }
                    _ => Some(Key::Ignore),
                }
            }
            _ => Some(Key::Ignore),
        }
    }

    fn scroll_by(&self, delta: isize) -> bool {
        let current = self.scroll.get();
        let next = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as usize)
        }
        .min(self.max_scroll.get());
        if next == current {
            false
        } else {
            self.scroll.set(next);
            true
        }
    }

    fn wait(
        &self,
        cancellation: &Cancellation,
        duration: Duration,
        mut on_redraw: impl FnMut(),
    ) -> Option<i32> {
        if self.termios.is_none() {
            return cancellation.wait(duration);
        }
        let started = Instant::now();
        loop {
            if let Some(key) = Self::read_key() {
                match key {
                    Key::Quit => return Some(0),
                    Key::Up => {
                        if self.scroll_by(-1) {
                            on_redraw();
                        }
                    }
                    Key::Down => {
                        if self.scroll_by(1) {
                            on_redraw();
                        }
                    }
                    Key::PageUp => {
                        let page = self.page.get().max(1) as isize;
                        if self.scroll_by(-page) {
                            on_redraw();
                        }
                    }
                    Key::PageDown => {
                        let page = self.page.get().max(1) as isize;
                        if self.scroll_by(page) {
                            on_redraw();
                        }
                    }
                    Key::Home => {
                        if self.scroll.get() != 0 {
                            self.scroll.set(0);
                            on_redraw();
                        }
                    }
                    Key::End => {
                        let end = self.max_scroll.get();
                        if self.scroll.get() != end {
                            self.scroll.set(end);
                            on_redraw();
                        }
                    }
                    Key::Ignore => {}
                }
            }
            let terminal_size = Self::terminal_size();
            if terminal_size != self.terminal_size.replace(terminal_size) {
                on_redraw();
            }
            let remaining = duration.saturating_sub(started.elapsed());
            if let Some(signal) = cancellation.wait(remaining.min(Duration::from_millis(25))) {
                return Some(signal);
            }
            if started.elapsed() >= duration {
                return None;
            }
        }
    }

    fn run_work<T: Send + 'static>(
        &self,
        cancellation: &Cancellation,
        mut on_redraw: impl FnMut(),
        work: impl FnOnce() -> T + Send + 'static,
    ) -> std::result::Result<T, i32> {
        let worker = thread::spawn(work);
        while !worker.is_finished() {
            if let Some(signal) = self.wait(cancellation, Duration::from_millis(25), &mut on_redraw)
            {
                return Err(signal);
            }
        }
        Ok(worker.join().expect("scan worker panicked"))
    }

    fn status_line(
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

    fn tick(
        &self,
        evaluation: &ScanEvaluation,
        check: u32,
        idle: u32,
        required_idle: u32,
        next_scan: u64,
    ) {
        if self.termios.is_some() {
            print!(
                "\x1b[2;1H\x1b[2K{}\x1b[H",
                Self::status_line(evaluation, check, idle, required_idle, next_scan)
            );
            let _ = io::stdout().flush();
        }
    }

    fn draw(
        &self,
        evaluation: &ScanEvaluation,
        check: u32,
        idle: u32,
        required_idle: u32,
        next_scan: u64,
    ) {
        if self.termios.is_none() {
            return;
        }
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
        let width = self.terminal_size.get().0;
        let mut table = Table::new();
        table
            .load_style(UTF8_FULL.with_rounded_corners().with_solid_inner_borders())
            .set_content_arrangement(ContentArrangement::DynamicFullWidth)
            .set_width(width)
            .set_header(HEADERS);
        for column in [1, 2] {
            table
                .column_mut(column)
                .expect("age and updated columns exist")
                .set_cell_alignment(CellAlignment::Right);
        }
        if rows.is_empty() {
            table.add_row([
                Cell::new("IDLE").fg(Color::Green),
                Cell::new(""),
                Cell::new(""),
                Cell::new(""),
                Cell::new(""),
                Cell::new("No active threads"),
            ]);
        }
        let now = Timestamp::now();
        for row in rows {
            table.add_row([
                Cell::new(row.label).fg(row.color),
                Cell::new(elapsed(row.created, now)),
                Cell::new(elapsed(row.updated, now)),
                Cell::new(clean_text(&row.project)),
                Cell::new(short_id(&row.id)),
                Cell::new(clean_text(&row.title)),
            ]);
        }
        let mut body = table.to_string();
        if let Some(error) = &evaluation.api_error {
            body.push_str(if evaluation.api_stuck {
                "\n\n\x1b[33mT3 API unavailable (no longer blocking)\x1b[0m"
            } else {
                "\n\n\x1b[31mT3 API errors\x1b[0m"
            });
            body.push_str("\n  ");
            body.push_str(&clean_text(error));
        }
        let body_lines: Vec<&str> = body.lines().collect();
        let mut top = format!(
            "\x1b[1;36mT3 IDLE WATCHDOG\x1b[0m\n{}\n",
            Self::status_line(evaluation, check, idle, required_idle, next_scan),
        );
        if let Some(action) = &self.action {
            writeln!(top, "\x1b[2mon idle       |\x1b[0m  {action}")
                .expect("writing to a string cannot fail");
        }
        top.push('\n');

        // Keep the title, status and footer fixed; scroll the table and API
        // errors through the remaining terminal rows.
        let height = usize::from(self.terminal_size.get().1);
        let top_height = top.lines().count();
        let bottom_height = 3;
        let available = height.saturating_sub(top_height + bottom_height);
        let max_scroll = body_lines.len().saturating_sub(available);
        let scroll = self.scroll.get().min(max_scroll);
        let visible_end = (scroll + available).min(body_lines.len());
        self.scroll.set(scroll);
        self.page.set(available.max(1));
        self.max_scroll.set(max_scroll);

        let mut frame = top;
        frame.push_str(&body_lines[scroll..visible_end].join("\n"));
        let mut footer = String::from("\n\n\x1b[2mq / Ctrl-C to exit");
        if max_scroll > 0 {
            write!(
                footer,
                "  |  lines {}-{}/{}  |  ↑↓ PgUp/PgDn Home/End",
                scroll + 1,
                visible_end,
                body_lines.len()
            )
            .expect("writing to a string cannot fail");
        } else {
            footer.push_str("  |  ↑↓ PgUp/PgDn Home/End");
        }
        footer.push_str("\x1b[0m\n");
        frame.push_str(&footer);
        self.render(&frame);
    }
}

impl Drop for Dashboard {
    fn drop(&mut self) {
        if let Some(original) = &self.termios {
            unsafe {
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, original);
            }
        }
        if self.termios.is_some() {
            print!("\x1b[?7h\x1b[?25h\x1b[?1049l");
            let _ = io::stdout().flush();
        }
    }
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
    expires_at_display: String,
}

struct Auth {
    source: TokenSource,
    token: Option<String>,
    expires_at: Option<Timestamp>,
    expires_at_display: Option<String>,
    session_id: Option<String>,
    t3: PathBuf,
    base_dir: PathBuf,
    ttl: String,
}

impl Auth {
    fn new(config: &Config) -> Result<Self> {
        let mut auth = Self {
            source: config.token_source.clone(),
            token: None,
            expires_at: None,
            expires_at_display: None,
            session_id: None,
            t3: config.t3.clone(),
            base_dir: config.base_dir.clone(),
            ttl: config.token_ttl.clone(),
        };
        match &auth.source {
            TokenSource::Static(_) => {}
            TokenSource::File(path) => auth.token = Some(read_token_file(path)?),
            TokenSource::Issued => auth.issue()?,
        }
        Ok(auth)
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
        let expires = self.expires_at_display.as_deref().unwrap_or("unknown");
        format!("issued T3 read token session={session} expires={expires}")
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
        let needs_refresh = self.expires_at.map_or(true, |expires_at| {
            expires_soon(expires_at, TOKEN_REFRESH_MARGIN)
        });
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
            self.expires_at_display = None;
            return Ok(format!("reloaded T3 bearer token from {}", path.display()));
        }
        bail!(
            "T3 rejected the provided bearer token; issue a new token or omit --token/--token-file to let the watchdog issue one"
        )
    }

    fn issue(&mut self) -> Result<()> {
        let issued = issue_token(&self.t3, &self.base_dir, &self.ttl)?;
        let old_session = self.session_id.take();
        self.token = Some(issued.token);
        self.expires_at = Some(issued.expires_at);
        self.expires_at_display = Some(issued.expires_at_display);
        self.session_id = Some(issued.session_id);
        if let Some(old_session) = old_session
            && self.session_id.as_deref() != Some(old_session.as_str())
        {
            revoke_session(&self.t3, &self.base_dir, &old_session);
        }
        Ok(())
    }

    fn revoke(&mut self) {
        if self.is_issued()
            && let Some(session_id) = self.session_id.take()
        {
            revoke_session(&self.t3, &self.base_dir, &session_id);
        }
    }
}

impl Drop for Auth {
    fn drop(&mut self) {
        self.revoke();
    }
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
    let expires_at_display = value
        .get("expiresAt")
        .and_then(Value::as_str)
        .context("t3 auth session issue returned no expiresAt")?
        .to_owned();
    let expires_at = parse_timestamp(&expires_at_display)
        .with_context(|| format!("parse token expiry {expires_at_display}"))?;
    Ok(IssuedToken {
        token,
        session_id,
        expires_at,
        expires_at_display,
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
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn command")?;
    match wait_child_timeout(&mut child, timeout).context("wait for command")? {
        Some(_) => child.wait_with_output().context("collect command output"),
        None => bail!("command timed out after {}s", timeout.as_secs()),
    }
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
    let active_activity = matches!(
        thread.activity_run_status.as_deref(),
        Some("preparing" | "starting" | "running")
    );
    let waiting_on_thread = thread.blocked_on_thread_wait == Some(true);
    let waiting_on_request = thread
        .pending_runtime_request
        .as_ref()
        .is_some_and(|request| request.blocking != Some(false));
    if thread.status == "queued" && (!active_activity || waiting_on_thread || waiting_on_request) {
        return Some(queued());
    }
    if waiting_on_thread || waiting_on_request {
        return Some(waiting());
    }
    if let Some(status) = thread.activity_run_status.as_deref() {
        return Some(match status {
            "preparing" | "starting" | "running" => {
                blocked("RUNNING", Color::Yellow, status.to_owned())
            }
            "waiting" if only_pull_request_watches(thread) => waiting(),
            "waiting" => blocked("WAITING?", Color::Red, "waiting".to_owned()),
            other => blocked("UNKNOWN", Color::Red, format!("activity:{other}")),
        });
    }
    match thread.status.as_str() {
        "preparing" | "starting" | "running" => {
            Some(blocked("RUNNING", Color::Yellow, thread.status.clone()))
        }
        "queued" => Some(queued()),
        "waiting" if only_pull_request_watches(thread) => Some(waiting()),
        "waiting" => Some(blocked("WAITING?", Color::Red, "waiting".to_owned())),
        "idle" | "completed" | "interrupted" | "failed" | "cancelled" | "rolled_back" => None,
        other => Some(blocked("UNKNOWN", Color::Red, format!("status:{other}"))),
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

fn wait_until_idle(
    state: &State,
    options: &Options,
    config: &Config,
    auth: &mut Auth,
    command: &[OsString],
    cancellation: &Cancellation,
) -> Result<i32> {
    let client = http_client();
    let dashboard = Dashboard::new(command);
    let report = |message: String| {
        if dashboard.termios.is_some() {
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
        state.root.display(),
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
                return Ok(signal_exit(state, signal));
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
            dashboard.draw(
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
            dashboard.tick(
                &evaluation,
                check,
                consecutive_idle,
                options.required_idle_checks,
                remaining,
            );
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
        return Ok(signal_exit(state, signal));
    }
    state.log("idle confirmed");
    auth.revoke();
    if let Some(signal) = cancellation.wait(Duration::ZERO) {
        return Ok(signal_exit(state, signal));
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
        return Ok(signal_exit(state, signal));
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

fn http_client() -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(REQUEST_TIMEOUT_SECS)))
            .build(),
    )
}

fn signal_exit(state: &State, signal: i32) -> i32 {
    if signal == 0 {
        state.log("watchdog exited");
        return 0;
    }
    state.log(if signal == SIGINT {
        "watchdog interrupted"
    } else {
        "watchdog terminated"
    });
    128 + signal
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn command_display(command: &[OsString]) -> String {
    command
        .iter()
        .map(|arg| format!("{arg:?}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn systemctl_command() -> OsString {
    [
        "/run/current-system/sw/bin/systemctl",
        "/usr/bin/systemctl",
        "/bin/systemctl",
    ]
    .into_iter()
    .find(|path| Path::new(path).is_file())
    .map(OsString::from)
    .unwrap_or_else(|| OsString::from("systemctl"))
}

fn resolve_command(command: &[OsString]) -> Vec<OsString> {
    if let [shortcut] = command {
        let action = match shortcut.to_str() {
            Some("sleep" | "suspend") => Some("suspend"),
            Some("poweroff") => Some("poweroff"),
            Some("hibernate") => Some("hibernate"),
            _ => None,
        };
        if let Some(action) = action {
            return [systemctl_command(), OsString::from(action)].into();
        }
    }
    command.to_vec()
}

fn run_command(state: &State, cancellation: &Cancellation, command: &[OsString]) -> i32 {
    if let Some(signal) = cancellation.wait(Duration::ZERO) {
        return signal_exit(state, signal);
    }
    state.log(format!("running command: {}", command_display(command)));
    let Some((program, arguments)) = command.split_first() else {
        state.log("could not run command: empty command");
        return 127;
    };
    let mut child = match Command::new(program).args(arguments).spawn() {
        Ok(child) => child,
        Err(error) => {
            state.log(format!("could not run command: {error}"));
            return 127;
        }
    };

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let code = status
                    .code()
                    .or_else(|| status.signal().map(|signal| 128 + signal))
                    .unwrap_or(1);
                state.log(format!("command exited with status {code}"));
                return code;
            }
            Ok(None) => {}
            Err(error) => {
                terminate_child(&mut child);
                state.log(format!("could not wait for command: {error}"));
                return 1;
            }
        }
        if let Some(signal) = cancellation.wait(Duration::from_millis(50)) {
            terminate_child(&mut child);
            return signal_exit(state, signal);
        }
    }
}

fn execute() -> Result<i32> {
    let options = Options::parse();
    let state = State::discover()?;
    let config = resolve_config(&options)?;
    let command = resolve_command(&options.command);
    let cancellation = Cancellation::new()?;
    let mut auth = Auth::new(&config)?;
    if let Some(signal) = cancellation.wait(Duration::ZERO) {
        return Ok(signal_exit(&state, signal));
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
    let code = match execute() {
        Ok(code) => code,
        Err(error) => {
            if let Ok(state) = State::discover() {
                state.log(format!("watchdog crashed: {error:#}"));
            }
            eprintln!("{NAME}: {error:#}");
            1
        }
    };
    std::process::exit(code);
}

fn summarize(items: &[String], limit: usize) -> String {
    let mut visible: Vec<_> = items.iter().take(limit).cloned().collect();
    if items.len() > limit {
        visible.push(format!("... +{} more", items.len() - limit));
    }
    visible.join(" | ")
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

fn clean_text(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .collect()
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
        assert_eq!(issued.expires_at_display, "2026-10-21T10:13:42.675Z");
    }

    #[test]
    fn issued_token_requires_a_session_id() {
        let value = serde_json::json!({
            "token": "secret-token",
            "expiresAt": "2026-10-21T10:13:42.675Z"
        });
        assert!(parse_issued_token(&value).is_err());
    }

    #[test]
    fn resolves_command_shortcuts() {
        assert_eq!(
            resolve_command(&[OsString::from("sleep")]),
            [systemctl_command(), OsString::from("suspend")]
        );
        assert_eq!(
            resolve_command(&[OsString::from("poweroff")]),
            [systemctl_command(), OsString::from("poweroff")]
        );
        assert_eq!(
            resolve_command(&[OsString::from("hibernate")]),
            [systemctl_command(), OsString::from("hibernate")]
        );
        assert_eq!(
            resolve_command(&[OsString::from("echo"), OsString::from("hi")]),
            [OsString::from("echo"), OsString::from("hi")]
        );
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
