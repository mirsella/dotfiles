//! Run a command after every other opencode session has gone idle.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use base64::Engine;
use clap::Parser;
use comfy_table::{Cell, CellAlignment, Color, ContentArrangement, Table, presets::UTF8_FULL};
use jiff::Zoned;
use serde_json::Value;
use signal_hook::{
    consts::{SIGINT, SIGTERM},
    iterator::{Handle as SignalHandle, Signals},
};

const NAME: &str = "opencode-idle-watchdog";
const DEFAULT_INTERVAL: u64 = 30;
const DEFAULT_IDLE_CHECKS: u32 = 3;
const DEFAULT_STUCK_AFTER: u64 = 900;
const REQUEST_TIMEOUT_SECS: u64 = 10;
const REQUEST_ATTEMPTS: u32 = 3;
const HEADERS: [&str; 6] = ["STATUS", "AGE", "UPDATED", "WORKSPACE", "SESSION", "TITLE"];

#[derive(Parser)]
#[command(
    about = "Wait until all other opencode sessions go idle, then run a command in the foreground.",
    after_help = "Shortcuts: sleep/suspend -> systemctl suspend, poweroff -> systemctl poweroff, hibernate -> systemctl hibernate",
    trailing_var_arg = true
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

    /// Stop blocking on a busy session whose detail fetch keeps failing once it
    /// shows no progress for this long (0 disables and blocks on them forever).
    #[arg(
        long = "stuck-after",
        default_value_t = DEFAULT_STUCK_AFTER,
        value_name = "SECONDS",
        value_parser = clap::value_parser!(u64)
    )]
    stuck_after: u64,

    #[arg(long = "ignore-session", value_name = "SESSION", action = clap::ArgAction::Append)]
    ignored_sessions: Vec<String>,

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

struct OpenCodeProcess {
    pid: i32,
    authorization: Option<String>,
}

fn proc_entry_gone(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(libc::ENOENT | libc::ESRCH))
}

fn proc_file(path: &Path, name: &str) -> Result<Option<Vec<u8>>> {
    match fs::read(path.join(name)) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if proc_entry_gone(&error) => Ok(None),
        Err(error) => Err(error).with_context(|| format!("read {}", path.join(name).display())),
    }
}

fn nul_strings(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|value| !value.is_empty())
        .map(|value| String::from_utf8_lossy(value).into_owned())
        .collect()
}

fn env_entry(environment: &[u8], name: &[u8]) -> Result<Option<String>> {
    let mut found = None;
    for entry in environment.split(|byte| *byte == 0) {
        let Some(value) = entry
            .strip_prefix(name)
            .and_then(|entry| entry.strip_prefix(b"="))
        else {
            continue;
        };
        // Environment values are arbitrary bytes. Decode them the same way a
        // UTF-8 runtime does instead of aborting the whole process scan.
        // Duplicate keys keep the last value, as process.env does.
        found = Some(String::from_utf8_lossy(value).into_owned());
    }
    Ok(found)
}

fn env_value(environment: &[u8], name: &[u8]) -> Result<Option<String>> {
    Ok(env_entry(environment, name)?.filter(|value| !value.trim().is_empty()))
}

fn password_from_env(environment: &[u8]) -> Result<Option<String>> {
    // v2 uses OPENCODE_PASSWORD first. The legacy name is only consulted when
    // the primary key is absent; an empty primary key still suppresses it.
    let value = match env_entry(environment, b"OPENCODE_PASSWORD")? {
        Some(value) => Some(value),
        None => env_entry(environment, b"OPENCODE_SERVER_PASSWORD")?,
    };
    Ok(value.filter(|value| !value.is_empty()))
}

fn basic_authorization(username: &str, password: &str) -> String {
    let encoded =
        base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
    format!("Basic {encoded}")
}

fn service_password(path: &Path) -> Result<Option<String>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    let mut config: serde_json::Map<String, Value> =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    match config.remove("password") {
        None => Ok(None),
        Some(Value::String(value)) => Ok((!value.is_empty()).then_some(value)),
        _ => bail!("native OpenCode service password must be a string"),
    }
}

fn xdg_dir(environment: &[u8], variable: &[u8], fallback: &str) -> Result<Option<PathBuf>> {
    if let Some(value) = env_value(environment, variable)? {
        return Ok(Some(PathBuf::from(value)));
    }
    Ok(env_value(environment, b"HOME")?.map(|home| PathBuf::from(home).join(fallback)))
}

fn service_files(directory: &Path) -> Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).with_context(|| format!("read {}", directory.display())),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("read {}", directory.display()))?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with("service") || !name.ends_with(".json") {
            continue;
        }
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) if proc_entry_gone(&error) => continue,
            Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
        };
        if file_type.is_file() {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn service_registration_password(path: &Path, pid: i32) -> Result<Option<Option<String>>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    let config: serde_json::Map<String, Value> = match serde_json::from_slice(&bytes) {
        Ok(config) => config,
        Err(_) => return Ok(None),
    };
    if config.get("pid").and_then(Value::as_i64) != Some(i64::from(pid)) {
        return Ok(None);
    }
    // A matching registration is authoritative even when its password is
    // missing, so never fall through to another channel's configuration.
    Ok(Some(match config.get("password") {
        Some(Value::String(value)) => (!value.is_empty()).then(|| value.clone()),
        _ => None,
    }))
}

fn config_service_password(directory: &Path) -> Result<Option<String>> {
    if let Some(password) = service_password(&directory.join("service.json"))? {
        return Ok(Some(password));
    }
    let mut candidates = Vec::new();
    for path in service_files(directory)? {
        if path.file_name().and_then(|name| name.to_str()) == Some("service.json") {
            continue;
        }
        if let Some(password) = service_password(&path)? {
            candidates.push(password);
        }
    }
    // Without a registration PID, a single channel file is unambiguous.
    match candidates.len() {
        0 => Ok(None),
        1 => Ok(candidates.pop()),
        _ => Ok(None),
    }
}

fn native_service_password(environment: &[u8], pid: i32) -> Result<Option<String>> {
    // The v2 service writes its runtime credential to a channel-specific
    // registration in the state directory. Match its PID so stale channels
    // cannot supply another server's password.
    if let Some(state) = xdg_dir(environment, b"XDG_STATE_HOME", ".local/state")? {
        for path in service_files(&state.join("opencode"))? {
            if let Some(password) = service_registration_password(&path, pid)? {
                return Ok(password);
            }
        }
    }

    // Older installs only wrote the credential to the config directory.
    let directory = match env_value(environment, b"OPENCODE_CONFIG_DIR")? {
        Some(directory) => PathBuf::from(directory),
        None => match xdg_dir(environment, b"XDG_CONFIG_HOME", ".config")? {
            Some(root) => root.join("opencode"),
            None => return Ok(None),
        },
    };
    config_service_password(&directory)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ServerMode {
    service: bool,
}

fn bool_flag_value(value: &str) -> Option<bool> {
    match value {
        "true" | "yes" | "on" | "1" | "y" => Some(true),
        "false" | "no" | "off" | "0" | "n" => Some(false),
        _ => None,
    }
}

fn server_mode(args: &[String]) -> Option<ServerMode> {
    let mut index = 1;
    let mut serve = false;
    let mut service = false;
    let mut stdio = false;
    while index < args.len() {
        let arg = &args[index];
        if !serve {
            match arg.as_str() {
                "serve" => {
                    serve = true;
                    index += 1;
                }
                "--print-logs" => index += 1,
                "--log-level" => {
                    if index + 1 >= args.len() {
                        return None;
                    }
                    index += 2;
                }
                _ => {
                    if let Some(value) = arg.strip_prefix("--print-logs=") {
                        bool_flag_value(value)?;
                        index += 1;
                    } else if arg.starts_with("--log-level=") {
                        index += 1;
                    } else {
                        return None;
                    }
                }
            }
            continue;
        }
        match arg.as_str() {
            "--service" => {
                service = true;
                index += 1;
            }
            "--stdio" => {
                stdio = true;
                index += 1;
            }
            "--print-logs" => index += 1,
            "--log-level" | "--hostname" | "--port" | "--cors" => {
                if index + 1 >= args.len() {
                    return None;
                }
                index += 2;
            }
            _ => {
                if let Some(value) = arg.strip_prefix("--service=") {
                    service = bool_flag_value(value)?;
                } else if let Some(value) = arg.strip_prefix("--stdio=") {
                    stdio = bool_flag_value(value)?;
                } else if let Some(value) = arg.strip_prefix("--print-logs=") {
                    bool_flag_value(value)?;
                } else if !arg.starts_with("--log-level=")
                    && !arg.starts_with("--hostname=")
                    && !arg.starts_with("--port=")
                    && !arg.starts_with("--cors=")
                {
                    return None;
                }
                index += 1;
            }
        }
    }
    (serve && !stdio).then_some(ServerMode { service })
}

#[cfg(test)]
fn is_server_command(args: &[String]) -> bool {
    server_mode(args).is_some()
}

fn opencode_processes() -> Result<Vec<OpenCodeProcess>> {
    let uid = unsafe { libc::getuid() };
    let mut processes = Vec::new();
    for entry in fs::read_dir("/proc").context("read /proc")? {
        let entry = entry.context("read /proc entry")?;
        let path = entry.path();
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<i32>().ok())
        else {
            continue;
        };
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(error) if proc_entry_gone(&error) => continue,
            Err(error) => return Err(error).context("read /proc entry metadata"),
        };
        if metadata.uid() != uid {
            continue;
        }
        let Some(comm) = proc_file(&path, "comm")? else {
            continue;
        };
        if comm.strip_suffix(b"\n") != Some(b"opencode") {
            continue;
        }
        let Some(cmdline) = proc_file(&path, "cmdline")? else {
            continue;
        };
        let args = nul_strings(&cmdline);
        if args.is_empty() {
            // Zombies and exiting processes keep a comm but lose their argv.
            continue;
        }
        let Some(mode) = server_mode(&args) else {
            continue;
        };
        let Some(environment) = proc_file(&path, "environ")? else {
            continue;
        };
        let password = if mode.service {
            native_service_password(&environment, pid)?
        } else {
            password_from_env(&environment)?
        };
        let authorization = password
            .as_deref()
            .map(|password| basic_authorization("opencode", password));
        processes.push(OpenCodeProcess { pid, authorization });
    }
    processes.sort_by_key(|process| process.pid);
    Ok(processes)
}

fn listener_host(address: &str) -> Result<String> {
    match address.len() {
        8 => {
            let address = u32::from_str_radix(address, 16).context("invalid IPv4 listener")?;
            let address = std::net::Ipv4Addr::from(address.to_le_bytes());
            Ok((if address.is_unspecified() {
                std::net::Ipv4Addr::LOCALHOST
            } else {
                address
            })
            .to_string())
        }
        32 => {
            let mut octets = [0; 16];
            for (word, output) in address
                .as_bytes()
                .as_chunks::<8>()
                .0
                .iter()
                .zip(octets.chunks_mut(4))
            {
                let word = std::str::from_utf8(word).context("invalid IPv6 listener")?;
                output.copy_from_slice(
                    &u32::from_str_radix(word, 16)
                        .context("invalid IPv6 listener")?
                        .to_le_bytes(),
                );
            }
            let address = std::net::Ipv6Addr::from(octets);
            Ok(format!(
                "[{}]",
                if address.is_unspecified() {
                    std::net::Ipv6Addr::LOCALHOST
                } else {
                    address
                }
            ))
        }
        _ => bail!("invalid listener address length"),
    }
}

fn tcp_listener_urls() -> Result<HashMap<String, String>> {
    let mut listeners = HashMap::new();
    for path in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let contents = fs::read_to_string(path).with_context(|| format!("read {path}"))?;
        for (line_number, line) in contents.lines().enumerate().skip(1) {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() < 10 {
                bail!("malformed {path} line {}", line_number + 1);
            }
            if fields[3] != "0A" {
                continue;
            }
            let (address, port) = fields[1].split_once(':').context("listener has no port")?;
            let host = listener_host(address)?;
            let port = u16::from_str_radix(port, 16).context("invalid listener port")?;
            listeners.insert(fields[9].to_owned(), format!("http://{host}:{port}"));
        }
    }
    Ok(listeners)
}

fn process_sockets(pid: i32) -> Result<Vec<String>> {
    let entries = match fs::read_dir(format!("/proc/{pid}/fd")) {
        Ok(entries) => entries,
        Err(error) if proc_entry_gone(&error) => return Ok(Vec::new()),
        Err(error) => return Err(error).context(format!("read /proc/{pid}/fd")),
    };
    let mut sockets = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) if proc_entry_gone(&error) => continue,
            Err(error) => return Err(error).context("read process file descriptor"),
        };
        let target = match fs::read_link(entry.path()) {
            Ok(target) => target,
            Err(error) if proc_entry_gone(&error) => continue,
            Err(error) => return Err(error).context("read process file descriptor link"),
        };
        let Some(inode) = target
            .to_str()
            .and_then(|target| target.strip_prefix("socket:["))
            .and_then(|target| target.strip_suffix(']'))
        else {
            continue;
        };
        sockets.push(inode.to_owned());
    }
    Ok(sockets)
}

struct OpenCodeApi<'a> {
    client: &'a ureq::Agent,
    url: &'a str,
    authorization: Option<&'a str>,
}

fn is_transient(error: &ureq::Error) -> bool {
    match error {
        ureq::Error::Timeout(_)
        | ureq::Error::ConnectionFailed
        | ureq::Error::Io(_)
        | ureq::Error::Protocol(_) => true,
        ureq::Error::StatusCode(status) => *status == 429 || *status >= 500,
        _ => false,
    }
}

fn error_status(error: &anyhow::Error) -> Option<u16> {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<ureq::Error>())
        .and_then(|error| match error {
            ureq::Error::StatusCode(status) => Some(*status),
            _ => None,
        })
}

fn is_v2_version(version: &str) -> bool {
    version == "local"
        || version == "2"
        || version.starts_with("2.")
        || version.starts_with("v2.")
        || version.starts_with("0.0.0-dev-")
        || version.starts_with("0.0.0-beta-")
        || version.starts_with("0.0.0-next-")
        || version.starts_with("0.0.0-alpha-")
}

fn is_v2_api(api: &OpenCodeApi<'_>) -> bool {
    if api.get("/api/info").is_ok_and(|value| {
        value
            .get("version")
            .and_then(Value::as_str)
            .is_some_and(is_v2_version)
    }) {
        return true;
    }
    // A local or embedded v2 build may report "local"/"unknown". The active
    // session endpoint exists only in v2, so use its shape as a fallback.
    api.active().is_ok()
}

fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            write!(encoded, "%{byte:02X}").expect("writing to a string cannot fail");
        }
    }
    encoded
}

impl OpenCodeApi<'_> {
    fn get(&self, path: &str) -> Result<Value> {
        self.get_query(path, &[])
    }

    fn get_query(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        let url = format!("{}{path}", self.url.trim_end_matches('/'));
        let mut attempt = 0u32;
        let mut response = loop {
            attempt += 1;
            let mut request = self.client.get(&url).header("Accept", "application/json");
            for (name, value) in query {
                request = request.query(*name, *value);
            }
            if let Some(authorization) = self.authorization {
                request = request.header("Authorization", authorization);
            }
            match request.call() {
                Ok(response) => break response,
                Err(error) if attempt < REQUEST_ATTEMPTS && is_transient(&error) => {
                    std::thread::sleep(Duration::from_millis(500 * u64::from(attempt)));
                }
                Err(error) => return Err(error).with_context(|| format!("GET {url}")),
            }
        };
        let body = response
            .body_mut()
            .read_to_string()
            .with_context(|| format!("read GET {url}"))?;
        serde_json::from_str(&body).with_context(|| format!("parse GET {url}"))
    }

    fn sessions(&self) -> Result<Vec<Value>> {
        let mut sessions = Vec::new();
        let mut cursor = None::<String>;
        let mut seen = HashSet::new();
        loop {
            let mut query = vec![("limit", "1000")];
            if let Some(ref value) = cursor {
                query.push(("cursor", value));
            }
            let mut page = self.get_query("/api/session", &query)?;
            cursor = match page.pointer_mut("/cursor/next").map(Value::take) {
                None | Some(Value::Null) => None,
                Some(Value::String(value)) => Some(value),
                _ => bail!("sessions page has an invalid next cursor"),
            };
            sessions.extend(page_array(page).context("sessions page")?);
            let Some(ref next) = cursor else {
                break;
            };
            if !seen.insert(next.clone()) {
                bail!("sessions pagination repeated a cursor");
            }
        }
        Ok(sessions)
    }

    fn active(&self) -> Result<serde_json::Map<String, Value>> {
        let mut page = self.get("/api/session/active")?;
        match page.get_mut("data").map(Value::take) {
            Some(Value::Object(active)) => Ok(active),
            _ => bail!("active sessions have no data map"),
        }
    }

    fn get_location(&self, path: &str, directory: &str) -> Result<Value> {
        self.get_query(path, &[("location[directory]", directory)])
    }

    fn get_location_optional(&self, path: &str, directory: &str) -> Result<Option<Value>> {
        match self.get_location(path, directory) {
            Ok(page) => Ok(Some(page)),
            // The location was removed after its sessions were listed. No
            // scoped form or permission can survive that, so treat it as empty.
            Err(error) if error_status(&error) == Some(404) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn forms(&self, directory: &str) -> Result<Vec<Value>> {
        match self
            .get_location_optional("/api/form", directory)
            .context("pending forms")?
        {
            Some(page) => page_array(page),
            None => Ok(Vec::new()),
        }
    }

    fn permissions(&self, directory: &str) -> Result<Vec<Value>> {
        match self
            .get_location_optional("/api/permission/request", directory)
            .context("pending permissions")?
        {
            Some(page) => page_array(page),
            None => Ok(Vec::new()),
        }
    }

    fn sessions_by_directory(&self) -> Result<BTreeMap<String, serde_json::Map<String, Value>>> {
        let mut directories: BTreeMap<String, serde_json::Map<String, Value>> = BTreeMap::new();
        for session in self.sessions()? {
            let directory = session
                .pointer("/location/directory")
                .and_then(Value::as_str)
                .context("global session has no directory")?
                .to_owned();
            let id = session
                .get("id")
                .and_then(Value::as_str)
                .context("global session has no string id")?
                .to_owned();
            directories
                .entry(directory)
                .or_default()
                .insert(id, session);
        }
        Ok(directories)
    }

    fn latest_assistant(&self, session: &str) -> Result<Value> {
        let session = encode_path_segment(session);
        let page = self.get_query(
            &format!("/api/session/{session}/message"),
            &[("limit", "1"), ("order", "desc"), ("type", "assistant")],
        )?;
        let message = page_array(page)?
            .into_iter()
            .next()
            .context("session has no assistant message")?;
        if message.get("type").and_then(Value::as_str) != Some("assistant") {
            bail!("latest assistant has invalid type");
        }
        Ok(message)
    }
}

fn page_array(mut page: Value) -> Result<Vec<Value>> {
    match page.get_mut("data").map(Value::take) {
        Some(Value::Array(data)) => Ok(data),
        _ => bail!("page has no data array"),
    }
}

fn status_kind(status: &Value) -> &str {
    status
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
}

fn active_part(part: &Value) -> bool {
    matches!(
        part.pointer("/state/status").and_then(Value::as_str),
        Some("streaming" | "running")
    )
}

fn subagent_background(part: &Value) -> bool {
    match part
        .pointer("/state/input/background")
        .or_else(|| part.pointer("/state/metadata/background"))
    {
        None => false,
        Some(Value::Bool(value)) => *value,
        // A malformed value must not make a background task look like a
        // foreground wait the watchdog can safely suspend through.
        Some(_) => true,
    }
}

fn waiting_sessions<'a>(
    status: &'a serde_json::Map<String, Value>,
    questions: &'a [Value],
    permissions: &'a [Value],
    assistants: &'a serde_json::Map<String, Value>,
    excluded_sessions: &'a HashSet<String>,
) -> HashSet<&'a str> {
    let mut waiting = excluded_sessions
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    waiting.extend(
        [questions, permissions]
            .into_iter()
            .flatten()
            .filter_map(|request| request.get("sessionID").and_then(Value::as_str)),
    );
    waiting.extend(
        assistants
            .iter()
            .filter(|(_, message)| {
                message
                    .get("content")
                    .and_then(Value::as_array)
                    .is_some_and(|parts| {
                        parts.iter().any(|part| {
                            active_part(part)
                                && part.get("type").and_then(Value::as_str) == Some("tool")
                                && part.get("name").and_then(Value::as_str) == Some("question")
                        })
                    })
            })
            .map(|(session, _)| session.as_str()),
    );
    loop {
        let parents: Vec<_> = status
            .iter()
            .filter_map(|(parent, state)| {
                if waiting.contains(parent.as_str()) || status_kind(state) != "busy" {
                    return None;
                }
                let parts = assistants.get(parent)?.get("content")?.as_array()?;
                let mut running = parts.iter().filter(|part| active_part(part)).peekable();
                running.peek()?;
                running
                    .all(|part| {
                        part.get("type").and_then(Value::as_str) == Some("tool")
                            && part.get("name").and_then(Value::as_str) == Some("subagent")
                            && !subagent_background(part)
                            && part.pointer("/state/metadata").is_some_and(|metadata| {
                                metadata
                                    .get("sessionID")
                                    .and_then(Value::as_str)
                                    .is_some_and(|child| waiting.contains(child))
                            })
                    })
                    .then_some(parent.as_str())
            })
            .collect();
        if parents.is_empty() {
            break;
        }
        waiting.extend(parents);
    }
    waiting
}

struct SessionVerdict {
    session: String,
    label: &'static str,
    color: Color,
    blocker: Option<String>,
    title: String,
    created: Option<u64>,
    updated: Option<u64>,
    active: bool,
}

fn classify_snapshot<'a>(
    state: &'a WorkspaceState,
    excluded_sessions: &'a HashSet<String>,
    stuck: &'a HashSet<String>,
    waiting: &'a HashSet<&str>,
) -> impl Iterator<Item = SessionVerdict> + 'a {
    let unresolved: HashSet<&str> = state
        .unresolved_busy
        .iter()
        .map(|unresolved| unresolved.session.as_str())
        .collect();
    state.status.iter().map(move |(session, value)| {
        let kind = status_kind(value);
        let active = kind != "idle"
            || (!excluded_sessions.contains(session) && waiting.contains(session.as_str()));
        let (label, color, blocker) = if excluded_sessions.contains(session) {
            ("SELF", Color::Cyan, None)
        } else if stuck.contains(session) {
            ("STUCK", Color::DarkGrey, None)
        } else if waiting.contains(session.as_str()) {
            // A pending native form or permission proves the wait even when
            // the message detail fetch failed, so these never block.
            ("WAITING", Color::Blue, None)
        } else if unresolved.contains(session.as_str()) {
            // The detail fetch failed and nothing proves idleness, so fail
            // closed. Only busy sessions are fetched, so these are busy.
            (
                "RUNNING",
                Color::Yellow,
                Some(format!("{session}:busy-unresolved")),
            )
        } else {
            match kind {
                "idle" => ("IDLE", Color::Green, None),
                "busy" => ("RUNNING", Color::Yellow, Some(format!("{session}:busy"))),
                _ => ("UNKNOWN", Color::Red, Some(format!("{session}:unknown"))),
            }
        };
        let details = state.sessions.get(session.as_str());
        SessionVerdict {
            session: session.clone(),
            label,
            color,
            blocker,
            title: clean_text(
                details
                    .and_then(|value| value.get("title"))
                    .and_then(Value::as_str)
                    .unwrap_or("Untitled session"),
            ),
            created: details
                .and_then(|value| value.pointer("/time/created"))
                .and_then(Value::as_u64),
            updated: details
                .and_then(|value| value.pointer("/time/updated"))
                .and_then(Value::as_u64),
            active,
        }
    })
}

fn has_running_watchdog(message: &Value) -> bool {
    let mut active = message
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|part| active_part(part))
        .peekable();
    active.peek().is_some()
        && active.all(|part| {
            part.get("type").and_then(Value::as_str) == Some("tool")
                && part.get("name").and_then(Value::as_str) == Some("shell")
                && part
                    .pointer("/state/input/command")
                    .and_then(Value::as_str)
                    .and_then(|command| command.split_ascii_whitespace().next())
                    .and_then(|command| Path::new(command.trim_matches(['\'', '"'])).file_name())
                    .is_some_and(|command| command == NAME)
        })
}

struct ApiSnapshot {
    pid: i32,
    url: String,
    directory: String,
    state: Result<WorkspaceState>,
}

struct UnresolvedBusy {
    session: String,
    updated: Option<u64>,
    reason: String,
}

struct WorkspaceState {
    status: serde_json::Map<String, Value>,
    sessions: serde_json::Map<String, Value>,
    assistants: serde_json::Map<String, Value>,
    unresolved_busy: Vec<UnresolvedBusy>,
    questions: Vec<Value>,
    permissions: Vec<Value>,
}

struct ApiScan {
    snapshots: Vec<ApiSnapshot>,
    failures: Vec<String>,
}

struct SnapshotJob {
    pid: i32,
    url: String,
    authorization: Option<String>,
    directory: String,
    sessions: serde_json::Map<String, Value>,
    active: Arc<serde_json::Map<String, Value>>,
}

fn process_jobs(
    client: &ureq::Agent,
    listeners: &HashMap<String, String>,
    process: &OpenCodeProcess,
) -> std::result::Result<Vec<SnapshotJob>, String> {
    let sockets = process_sockets(process.pid)
        .map_err(|error| format!("pid={} socket discovery: {error:#}", process.pid))?;
    let url = sockets
        .into_iter()
        .filter_map(|inode| listeners.get(&inode))
        .find(|url| {
            is_v2_api(&OpenCodeApi {
                client,
                url,
                authorization: process.authorization.as_deref(),
            })
        })
        .cloned()
        .ok_or_else(|| format!("pid={} opencode-api:unreachable", process.pid))?;
    let api = OpenCodeApi {
        client,
        url: &url,
        authorization: process.authorization.as_deref(),
    };
    let sessions = api
        .sessions_by_directory()
        .map_err(|error| format!("pid={} global-sessions:{error:#}", process.pid))?;
    let active = Arc::new(
        api.active()
            .map_err(|error| format!("pid={} active-sessions:{error:#}", process.pid))?,
    );
    Ok(sessions
        .into_iter()
        .map(|(directory, sessions)| SnapshotJob {
            pid: process.pid,
            url: url.clone(),
            authorization: process.authorization.clone(),
            directory,
            sessions,
            active: Arc::clone(&active),
        })
        .collect())
}

fn workspace_snapshot(
    client: &ureq::Agent,
    pid: i32,
    url: String,
    authorization: Option<String>,
    directory: String,
    sessions: serde_json::Map<String, Value>,
    active: &serde_json::Map<String, Value>,
) -> ApiSnapshot {
    let api = OpenCodeApi {
        client,
        url: &url,
        authorization: authorization.as_deref(),
    };
    let state = (|| -> Result<WorkspaceState> {
        let mut status = serde_json::Map::new();
        for session in sessions.keys() {
            let kind = match active.get(session) {
                Some(state) if state.get("type").and_then(Value::as_str) == Some("running") => {
                    "busy"
                }
                Some(_) => bail!("unknown active session state for {session}"),
                None => "idle",
            };
            status.insert(session.clone(), serde_json::json!({ "type": kind }));
        }
        let mut assistants = serde_json::Map::new();
        let mut unresolved_busy = Vec::new();
        let mut has_busy = false;
        for (session, state) in &status {
            if status_kind(state) != "busy" {
                continue;
            }
            has_busy = true;
            match api.latest_assistant(session) {
                Ok(message) => {
                    assistants.insert(session.clone(), message);
                }
                Err(error) => unresolved_busy.push(UnresolvedBusy {
                    session: session.clone(),
                    updated: sessions
                        .get(session)
                        .and_then(|details| details.pointer("/time/updated"))
                        .and_then(Value::as_u64),
                    reason: format!("{error:#}"),
                }),
            }
        }
        // Pending forms and permissions only exist for busy sessions. Skipping
        // the scoped requests also avoids a 404 for directories the server no
        // longer knows, such as removed worktrees with historical sessions.
        let (questions, permissions) = if has_busy {
            (api.forms(&directory)?, api.permissions(&directory)?)
        } else {
            (Vec::new(), Vec::new())
        };
        Ok(WorkspaceState {
            status,
            sessions,
            assistants,
            unresolved_busy,
            questions,
            permissions,
        })
    })();
    ApiSnapshot {
        pid,
        url,
        directory,
        state,
    }
}

fn api_snapshots(client: &ureq::Agent, processes: &[OpenCodeProcess]) -> ApiScan {
    let listeners = match tcp_listener_urls() {
        Ok(listeners) => listeners,
        Err(error) => {
            return ApiScan {
                snapshots: Vec::new(),
                failures: vec![format!("listener discovery: {error:#}")],
            };
        }
    };
    let (mut jobs, failures) = thread::scope(|scope| {
        let handles = processes
            .iter()
            .map(|process| scope.spawn(|| process_jobs(client, &listeners, process)))
            .collect::<Vec<_>>();
        let mut jobs = Vec::new();
        let mut failures = Vec::new();
        for handle in handles {
            match handle.join().expect("process scanner panicked") {
                Ok(mut process_jobs) => jobs.append(&mut process_jobs),
                Err(failure) => failures.push(failure),
            }
        }
        (jobs, failures)
    });
    jobs.sort_by(|left, right| {
        left.pid
            .cmp(&right.pid)
            .then_with(|| left.directory.cmp(&right.directory))
    });
    let snapshots = thread::scope(|scope| {
        jobs.into_iter()
            .map(|job| {
                scope.spawn(move || {
                    workspace_snapshot(
                        client,
                        job.pid,
                        job.url,
                        job.authorization,
                        job.directory,
                        job.sessions,
                        &job.active,
                    )
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|job| job.join().expect("workspace scanner panicked"))
            .collect::<Vec<_>>()
    });
    ApiScan {
        snapshots,
        failures,
    }
}

struct WorkspaceVerdicts {
    directory: String,
    sessions: Vec<SessionVerdict>,
}

fn session_rank(verdict: &SessionVerdict) -> u8 {
    if verdict.blocker.is_some() {
        4
    } else if verdict.label == "WAITING" {
        3
    } else if verdict.active {
        2
    } else {
        1
    }
}

fn deduplicate_sessions(workspaces: &mut [WorkspaceVerdicts]) {
    let mut unique: HashMap<String, (usize, SessionVerdict)> = HashMap::new();
    for (workspace_index, workspace) in workspaces.iter_mut().enumerate() {
        for verdict in workspace.sessions.drain(..) {
            let replace = unique
                .get(&verdict.session)
                .is_none_or(|(_, current)| session_rank(&verdict) > session_rank(current));
            if replace {
                unique.insert(verdict.session.clone(), (workspace_index, verdict));
            }
        }
    }
    for (workspace_index, verdict) in unique.into_values() {
        workspaces[workspace_index].sessions.push(verdict);
    }
}

struct ScanEvaluation {
    excluded_sessions: HashSet<String>,
    workspaces: Vec<WorkspaceVerdicts>,
    blockers: Vec<String>,
    active: usize,
}

struct StuckEntry {
    first_seen: Instant,
    updated: Option<u64>,
    reported: bool,
}

fn evaluate(
    scan: &ApiScan,
    ignored_sessions: &[String],
    stuck_after: Duration,
    tracker: &mut HashMap<String, StuckEntry>,
    seen_active: &mut HashSet<String>,
    report: &dyn Fn(String),
) -> ScanEvaluation {
    let mut excluded_sessions = ignored_sessions.iter().cloned().collect::<HashSet<_>>();
    excluded_sessions.extend(
        scan.snapshots
            .iter()
            .filter_map(|snapshot| snapshot.state.as_ref().ok())
            .flat_map(|state| {
                state
                    .status
                    .iter()
                    .filter(|(_, state)| status_kind(state) == "busy")
                    .filter(|(session, _)| {
                        state
                            .assistants
                            .get(*session)
                            .is_some_and(has_running_watchdog)
                    })
                    .map(|(session, _)| session.clone())
            }),
    );
    // Reuse the same wait closure for stuck tracking, classification and redraws.
    let waiting: Vec<_> = scan
        .snapshots
        .iter()
        .map(|snapshot| match &snapshot.state {
            Ok(state) => waiting_sessions(
                &state.status,
                &state.questions,
                &state.permissions,
                &state.assistants,
                &excluded_sessions,
            ),
            Err(_) => HashSet::new(),
        })
        .collect();
    let mut stuck = HashSet::new();
    if stuck_after.is_zero() {
        tracker.clear();
    } else {
        let mut current: HashMap<&str, (Option<u64>, &str)> = HashMap::new();
        let mut observed: HashSet<&str> = HashSet::new();
        for (snapshot, waiting) in scan.snapshots.iter().zip(&waiting) {
            let Ok(state) = &snapshot.state else {
                continue;
            };
            observed.extend(state.status.keys().map(String::as_str));
            // Sessions that cannot block (excluded or waiting on the user) need no
            // stuck tracking; claiming they block would contradict the verdicts.
            for unresolved in &state.unresolved_busy {
                if excluded_sessions.contains(&unresolved.session)
                    || waiting.contains(unresolved.session.as_str())
                {
                    continue;
                }
                current.insert(
                    unresolved.session.as_str(),
                    (unresolved.updated, unresolved.reason.as_str()),
                );
            }
        }
        // Only drop entries the scan affirmatively resolved. Sessions a failed scan
        // did not observe keep their timer so a flaky server cannot reset it.
        tracker.retain(|session, entry| {
            let resolved =
                observed.contains(session.as_str()) && !current.contains_key(session.as_str());
            if resolved && entry.reported {
                report(format!("session {session} left the stuck-unresolved state"));
            }
            !resolved
        });
        for (session, (updated, reason)) in &current {
            let entry = tracker.entry((*session).to_owned()).or_insert_with(|| {
                report(format!(
                    "blocking on {session} (session detail unavailable: {reason}); stops blocking after {}s without progress",
                    stuck_after.as_secs()
                ));
                StuckEntry {
                    first_seen: Instant::now(),
                    updated: *updated,
                    reported: false,
                }
            });
            if entry.updated != *updated {
                entry.first_seen = Instant::now();
                entry.updated = *updated;
                entry.reported = false;
            }
            if entry.first_seen.elapsed() >= stuck_after {
                if !entry.reported {
                    entry.reported = true;
                    report(format!(
                        "no progress from {session} for {}s; no longer blocking idle",
                        stuck_after.as_secs()
                    ));
                }
                stuck.insert((*session).to_owned());
            }
        }
    }
    let mut blockers = scan.failures.clone();
    let mut workspaces = Vec::with_capacity(scan.snapshots.len());
    for (snapshot, waiting) in scan.snapshots.iter().zip(&waiting) {
        let state = match &snapshot.state {
            Ok(state) => state,
            Err(error) => {
                blockers.push(format!(
                    "pid={} {} directory={} workspace-scan:{error:#}",
                    snapshot.pid, snapshot.url, snapshot.directory
                ));
                continue;
            }
        };
        let mut classified = Vec::new();
        for verdict in classify_snapshot(state, &excluded_sessions, &stuck, waiting) {
            if verdict.active {
                seen_active.insert(verdict.session.clone());
            }
            if verdict.active || seen_active.contains(&verdict.session) {
                classified.push(verdict);
            }
        }
        let current: Vec<_> = classified
            .iter()
            .filter_map(|verdict| verdict.blocker.as_deref())
            .collect();
        if !current.is_empty() {
            blockers.push(format!(
                "pid={} {} directory={} {}",
                snapshot.pid,
                snapshot.url,
                snapshot.directory,
                current[..current.len().min(4)].join(" ")
            ));
        }
        workspaces.push(WorkspaceVerdicts {
            directory: clean_text(&snapshot.directory),
            sessions: classified,
        });
    }
    deduplicate_sessions(&mut workspaces);
    let active = workspaces
        .iter()
        .flat_map(|workspace| &workspace.sessions)
        .filter(|verdict| verdict.blocker.is_some())
        .count();
    ScanEvaluation {
        excluded_sessions,
        workspaces,
        blockers,
        active,
    }
}

fn collect_scan(client: &ureq::Agent) -> Result<ApiScan> {
    Ok(api_snapshots(client, &opencode_processes()?))
}

fn summarize(items: &[String], limit: usize) -> String {
    let mut visible: Vec<_> = items.iter().take(limit).cloned().collect();
    if items.len() > limit {
        visible.push(format!("... +{} more", items.len() - limit));
    }
    visible.join(" | ")
}

fn elapsed(millis: u64) -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .saturating_sub(millis as u128)
        / 1000;
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
        self.render(
            "\x1b[1;36mOPENCODE IDLE WATCHDOG\x1b[0m\n\n\x1b[2mScanning sessions...\x1b[0m",
        );
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
            if let Some(signal) = self.wait(cancellation, Duration::from_millis(10), &mut on_redraw)
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
        let state = if evaluation.blockers.is_empty() {
            "IDLE"
        } else {
            "BLOCKED"
        };
        format!(
            "\x1b[2mcheck #{check}  |\x1b[0m  {state}  |  {} active  |  idle {idle}/{required_idle}  |  next scan in {next_scan}s",
            evaluation.active,
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
        let mut rows = Vec::new();
        for workspace in &evaluation.workspaces {
            for verdict in &workspace.sessions {
                let age = verdict
                    .created
                    .map(elapsed)
                    .unwrap_or_else(|| "-".to_owned());
                let updated = verdict
                    .updated
                    .map(elapsed)
                    .unwrap_or_else(|| "-".to_owned());
                rows.push((
                    verdict.color,
                    [
                        verdict.label.to_owned(),
                        age,
                        updated,
                        workspace.directory.clone(),
                        verdict.session.clone(),
                        verdict.title.clone(),
                    ],
                ));
            }
        }
        rows.sort_unstable_by(|left, right| left.1.cmp(&right.1));
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
            let (label, color, message) = if evaluation.blockers.is_empty() {
                ("IDLE", Color::Green, "No active sessions".to_owned())
            } else {
                ("BLOCKED", Color::Red, summarize(&evaluation.blockers, 2))
            };
            table.add_row([
                Cell::new(label).fg(color),
                Cell::new(""),
                Cell::new(""),
                Cell::new(""),
                Cell::new(""),
                Cell::new(message),
            ]);
        }
        for (color, row) in rows {
            let [status, age, updated, workspace, session, title] = row;
            table.add_row([
                Cell::new(status).fg(color),
                Cell::new(age),
                Cell::new(updated),
                Cell::new(workspace),
                Cell::new(session),
                Cell::new(title),
            ]);
        }
        let mut body = table.to_string();
        if !evaluation.blockers.is_empty() {
            body.push_str("\n\n\x1b[31mAPI errors\x1b[0m");
            for failure in &evaluation.blockers {
                body.push_str("\n  ");
                body.push_str(&clean_text(failure));
            }
        }
        let body_lines: Vec<&str> = body.lines().collect();
        let mut top = format!(
            "\x1b[1;36mOPENCODE IDLE WATCHDOG\x1b[0m\n{}\n",
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

fn session_exclusions(ignored_sessions: &[String], self_session: Option<&str>) -> Vec<String> {
    let mut exclusions = ignored_sessions.to_vec();
    if let Some(session) = self_session.filter(|session| !session.is_empty())
        && !exclusions.iter().any(|existing| existing == session)
    {
        exclusions.push(session.to_owned());
    }
    exclusions
}

fn wait_until_idle(state: &State, options: &Options, command: &[OsString]) -> Result<i32> {
    // The v2 shell tool exposes the launching session to the command. Prefer
    // that over inferring the session from its latest shell tool call.
    let self_session = env::var("OPENCODE_SESSION_ID").ok();
    let ignored_sessions = session_exclusions(&options.ignored_sessions, self_session.as_deref());
    let client = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(REQUEST_TIMEOUT_SECS)))
            .build(),
    );
    let cancellation = Cancellation::new()?;
    let dashboard = Dashboard::new(command);
    let report = |message| {
        if dashboard.termios.is_some() {
            state.record(message);
        } else {
            state.log(message);
        }
    };
    let scan_work = || {
        let client = client.clone();
        move || collect_scan(&client)
    };
    dashboard.loading();
    let stuck_after = Duration::from_secs(options.stuck_after);
    let mut stuck_tracker: HashMap<String, StuckEntry> = HashMap::new();
    let mut seen_active = HashSet::new();
    let mut evaluation = {
        let scan = match dashboard.run_work(&cancellation, || dashboard.loading(), scan_work()) {
            Ok(result) => result?,
            Err(signal) => {
                drop(dashboard);
                return Ok(signal_exit(state, signal));
            }
        };
        evaluate(
            &scan,
            &ignored_sessions,
            stuck_after,
            &mut stuck_tracker,
            &mut seen_active,
            &report,
        )
    };
    let mut excluded_sessions: Vec<_> = evaluation.excluded_sessions.iter().cloned().collect();
    excluded_sessions.sort_unstable();
    let started = format!(
        "watchdog started; excluded_sessions={}; interval={}s required_idle_checks={} stuck_after={}s request_timeout={}s request_attempts={}; command={}; cache={}",
        if excluded_sessions.is_empty() {
            "none".to_owned()
        } else {
            excluded_sessions.join(",")
        },
        options.interval,
        options.required_idle_checks,
        options.stuck_after,
        REQUEST_TIMEOUT_SECS,
        REQUEST_ATTEMPTS,
        if command.is_empty() {
            "none".to_owned()
        } else {
            command_display(command)
        },
        state.root.display()
    );
    report(started);

    let mut consecutive_idle = 0;
    let mut check = 0;
    let signal = 'checks: loop {
        check += 1;
        consecutive_idle = if evaluation.blockers.is_empty() {
            consecutive_idle + 1
        } else {
            0
        };
        let check_message = format!(
            "check {check}: idle={consecutive_idle}/{}; blockers={}",
            options.required_idle_checks,
            if evaluation.blockers.is_empty() {
                "none"
            } else {
                "opencode API session status"
            }
        );
        report(check_message);
        if !evaluation.blockers.is_empty() {
            report(format!(
                "opencode_api_blockers: {}",
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
            dashboard.wait(&cancellation, Duration::ZERO, || draw(options.interval))
        {
            break Some(signal);
        }
        if consecutive_idle >= options.required_idle_checks {
            if command.is_empty() {
                break None;
            }
            let final_scan =
                match dashboard.run_work(&cancellation, || draw(options.interval), scan_work()) {
                    Ok(result) => result?,
                    Err(signal) => break Some(signal),
                };
            let final_evaluation = evaluate(
                &final_scan,
                &ignored_sessions,
                stuck_after,
                &mut stuck_tracker,
                &mut seen_active,
                &report,
            );
            if final_evaluation.blockers.is_empty() {
                break None;
            }
            evaluation = final_evaluation;
            consecutive_idle = 0;
            continue;
        }
        let mut remaining = options.interval;
        while remaining > 0 {
            if let Some(signal) =
                dashboard.wait(&cancellation, Duration::from_secs(1), || draw(remaining))
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
        match dashboard.run_work(&cancellation, || draw(0), scan_work()) {
            Ok(result) => {
                evaluation = evaluate(
                    &result?,
                    &ignored_sessions,
                    stuck_after,
                    &mut stuck_tracker,
                    &mut seen_active,
                    &report,
                );
            }
            Err(signal) => break Some(signal),
        }
    };

    drop(dashboard);
    if let Some(signal) = signal {
        return Ok(signal_exit(state, signal));
    }
    state.log("idle confirmed");
    if command.is_empty() {
        Ok(0)
    } else {
        Ok(run_command(state, &cancellation, command))
    }
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

fn resolve_command(command: &[OsString]) -> Vec<OsString> {
    if let [shortcut] = command {
        let action = match shortcut.to_str() {
            Some("sleep" | "suspend") => Some("suspend"),
            Some("poweroff") => Some("poweroff"),
            Some("hibernate") => Some("hibernate"),
            _ => None,
        };
        if let Some(action) = action {
            return ["/usr/bin/systemctl", action].map(OsString::from).into();
        }
    }
    command.to_vec()
}

fn run_command(state: &State, cancellation: &Cancellation, command: &[OsString]) -> i32 {
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
    let command = resolve_command(&options.command);
    wait_until_idle(&state, &options, &command)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    fn serve_json(responses: Vec<Value>) -> (String, JoinHandle<Vec<String>>) {
        serve_statuses(responses.into_iter().map(|body| (200, body)).collect())
    }

    fn serve_statuses(responses: Vec<(u16, Value)>) -> (String, JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(&stream);
                let mut request = String::new();
                loop {
                    let mut line = String::new();
                    assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                    if line == "\r\n" {
                        break;
                    }
                    request.push_str(&line);
                }
                requests.push(request);
                let body = serde_json::to_vec(&body).unwrap();
                let reason = match status {
                    200 => "OK",
                    404 => "Not Found",
                    _ => "Error",
                };
                write!(stream,
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len(),
                ).unwrap();
                stream.write_all(&body).unwrap();
            }
            requests
        });
        (url, server)
    }

    fn session_map<const N: usize>(sessions: [(&str, Value); N]) -> serde_json::Map<String, Value> {
        sessions
            .into_iter()
            .map(|(id, details)| (id.to_owned(), details))
            .collect()
    }

    fn test_client() -> ureq::Agent {
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(5)))
            .build()
            .into()
    }

    #[test]
    fn requests_preserve_query_parameters_and_authentication() {
        let (url, server) = serve_json(vec![json!({ "ok": true })]);
        let client = test_client();
        let authorization = basic_authorization("opencode", "secret");
        let api = OpenCodeApi {
            client: &client,
            url: &url,
            authorization: Some(&authorization),
        };
        assert_eq!(
            api.get("/api/session?limit=1").unwrap(),
            json!({ "ok": true })
        );
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].lines().next().unwrap(),
            "GET /api/session?limit=1 HTTP/1.1"
        );
        assert!(requests[0].lines().any(|line| {
            let Some((name, value)) = line.split_once(':') else {
                return false;
            };
            name.eq_ignore_ascii_case("authorization") && value.trim() == authorization
        }));
    }

    #[test]
    fn forms_and_permissions_scope_by_location() {
        let (url, server) = serve_json(vec![json!({"data": []}), json!({"data": []})]);
        let client = test_client();
        let api = OpenCodeApi {
            client: &client,
            url: &url,
            authorization: None,
        };
        assert!(api.forms("/tmp/a b%+é").unwrap().is_empty());
        assert!(api.permissions("/tmp/a b%+é").unwrap().is_empty());
        let requests = server.join().unwrap();
        assert_eq!(
            requests[0].lines().next().unwrap(),
            "GET /api/form?location%5Bdirectory%5D=%2Ftmp%2Fa%20b%25%2B%C3%A9 HTTP/1.1"
        );
        assert_eq!(
            requests[1].lines().next().unwrap(),
            "GET /api/permission/request?location%5Bdirectory%5D=%2Ftmp%2Fa%20b%25%2B%C3%A9 HTTP/1.1"
        );
    }

    #[test]
    fn missing_locations_have_no_scoped_requests() {
        let (url, server) = serve_statuses(vec![
            (
                404,
                json!({"_tag": "LocationNotFoundError", "message": "location not found"}),
            ),
            (200, json!({"data": []})),
        ]);
        let client = test_client();
        let api = OpenCodeApi {
            client: &client,
            url: &url,
            authorization: None,
        };
        assert!(api.forms("/tmp/removed").unwrap().is_empty());
        assert!(api.permissions("/tmp/removed").unwrap().is_empty());
        assert_eq!(server.join().unwrap().len(), 2);
    }

    #[test]
    fn recognizes_http_status_from_request_context() {
        let error = anyhow::Error::new(ureq::Error::StatusCode(404)).context("GET /api/form");
        assert_eq!(error_status(&error), Some(404));
    }

    #[test]
    fn accepts_v2_versions_from_every_channel() {
        for version in [
            "2",
            "2.1.2",
            "v2.1.2",
            "local",
            "0.0.0-dev-20261021",
            "0.0.0-beta-20261021",
            "0.0.0-next-20261021",
            "0.0.0-alpha-20261021",
        ] {
            assert!(is_v2_version(version), "{version}");
        }
        for version in ["1.15.0", "0.15.0", "0.0.0-dev", "unknown"] {
            assert!(!is_v2_version(version), "{version}");
        }
    }

    #[test]
    fn latest_assistant_encodes_session_id_path_segments() {
        let (url, server) = serve_json(vec![
            json!({"data": [{"type": "assistant", "content": []}]}),
        ]);
        let client = test_client();
        let api = OpenCodeApi {
            client: &client,
            url: &url,
            authorization: None,
        };
        assert!(api.latest_assistant("ses_a/b?c").is_ok());
        let requests = server.join().unwrap();
        assert!(requests[0].lines().next().unwrap().starts_with(
            "GET /api/session/ses_a%2Fb%3Fc/message?limit=1&order=desc&type=assistant "
        ));
        assert_eq!(encode_path_segment("ses_a/b?c d%"), "ses_a%2Fb%3Fc%20d%25");
    }

    #[test]
    fn ignores_empty_and_non_utf8_cmdlines() {
        assert!(nul_strings(b"").is_empty());
        assert_eq!(
            nul_strings(b"opencode\0serve\0\xff\0"),
            ["opencode", "serve", "\u{fffd}"]
        );
    }

    #[test]
    fn reads_lossy_environment_values_and_v2_password_precedence() {
        assert_eq!(
            password_from_env(b"OPENCODE_PASSWORD=\xff\0").unwrap(),
            Some("\u{fffd}".to_owned())
        );
        assert_eq!(
            password_from_env(b"OPENCODE_SERVER_PASSWORD=legacy\0").unwrap(),
            Some("legacy".to_owned())
        );
        assert_eq!(
            password_from_env(b"OPENCODE_PASSWORD=primary\0OPENCODE_SERVER_PASSWORD=legacy\0")
                .unwrap(),
            Some("primary".to_owned())
        );
        // An empty primary key is still present and suppresses the legacy key.
        assert_eq!(
            password_from_env(b"OPENCODE_PASSWORD=\0OPENCODE_SERVER_PASSWORD=legacy\0").unwrap(),
            None
        );
        assert_eq!(password_from_env(b"OPENCODE_PASSWORD=\0").unwrap(), None);
        // Duplicate keys keep the last value, matching process.env.
        assert_eq!(
            password_from_env(b"OPENCODE_PASSWORD=first\0OPENCODE_PASSWORD=last\0").unwrap(),
            Some("last".to_owned())
        );
    }

    #[test]
    fn discovers_only_serve_processes() {
        assert!(is_server_command(&["opencode".into(), "serve".into()]));
        assert!(is_server_command(&[
            "opencode".into(),
            "serve".into(),
            "--service".into()
        ]));
        assert!(is_server_command(&[
            "opencode".into(),
            "--log-level".into(),
            "debug".into(),
            "serve".into()
        ]));
        assert!(is_server_command(&[
            "opencode".into(),
            "--log-level=debug".into(),
            "serve".into()
        ]));
        assert!(is_server_command(&[
            "opencode".into(),
            "--print-logs".into(),
            "serve".into()
        ]));
        assert!(is_server_command(&[
            "opencode".into(),
            "--print-logs=false".into(),
            "serve".into()
        ]));
        assert!(is_server_command(&[
            "opencode".into(),
            "--print-logs=yes".into(),
            "serve".into()
        ]));
        assert!(is_server_command(&[
            "opencode".into(),
            "serve".into(),
            "--stdio=false".into()
        ]));
        assert!(is_server_command(&[
            "opencode".into(),
            "serve".into(),
            "--stdio=no".into()
        ]));
        assert!(is_server_command(&[
            "opencode".into(),
            "serve".into(),
            "--service=true".into()
        ]));
        assert!(is_server_command(&[
            "opencode".into(),
            "serve".into(),
            "--hostname".into(),
            "--stdio".into()
        ]));
        for command in [
            vec!["opencode".into()],
            vec!["opencode".into(), "attach".into()],
            vec!["opencode".into(), "--standalone".into()],
            vec!["opencode".into(), "--version".into()],
            vec!["opencode".into(), "serve".into(), "--stdio".into()],
            vec!["opencode".into(), "serve".into(), "--stdio=true".into()],
            vec!["opencode".into(), "serve".into(), "--stdio=yes".into()],
            vec!["opencode".into(), "serve".into(), "--stdio=maybe".into()],
            vec![
                "opencode".into(),
                "--print-logs=maybe".into(),
                "serve".into(),
            ],
            vec!["opencode".into(), "serve".into(), "--help".into()],
            vec!["opencode".into(), "--standalone".into(), "serve".into()],
            vec![
                "opencode".into(),
                "--log-level".into(),
                "debug".into(),
                "serve".into(),
                "--stdio".into(),
            ],
        ] {
            assert!(!is_server_command(&command));
        }

        assert!(
            !server_mode(&["opencode".into(), "serve".into()])
                .unwrap()
                .service
        );
        assert!(
            server_mode(&["opencode".into(), "serve".into(), "--service=true".into()])
                .unwrap()
                .service
        );
        assert!(
            !server_mode(&["opencode".into(), "serve".into(), "--service=false".into()])
                .unwrap()
                .service
        );
    }

    #[test]
    fn native_service_password_matches_registration_pid_and_channel() {
        let root =
            std::env::temp_dir().join(format!("{NAME}-service-password-{}", std::process::id()));
        let state = root.join("state/opencode");
        let config = root.join("config/opencode");
        fs::create_dir_all(&state).unwrap();
        fs::create_dir_all(&config).unwrap();
        fs::write(config.join("service.json"), r#"{"password":"config"}"#).unwrap();
        let environment = format!(
            "HOME={}\0XDG_STATE_HOME={}\0XDG_CONFIG_HOME={}\0",
            root.display(),
            root.join("state").display(),
            root.join("config").display(),
        );
        let environment = environment.as_bytes();
        assert_eq!(
            native_service_password(environment, 7).unwrap(),
            Some("config".to_owned())
        );

        // A stale registration for another PID must not supply a password.
        fs::write(
            state.join("service.json"),
            r#"{"pid":8,"password":"stale"}"#,
        )
        .unwrap();
        assert_eq!(
            native_service_password(environment, 7).unwrap(),
            Some("config".to_owned())
        );

        // The matching registration wins over every config fallback.
        fs::write(
            state.join("service.json"),
            r#"{"pid":7,"password":"state"}"#,
        )
        .unwrap();
        assert_eq!(
            native_service_password(environment, 7).unwrap(),
            Some("state".to_owned())
        );

        // Channel-specific registrations are found by PID.
        fs::remove_file(state.join("service.json")).unwrap();
        fs::write(
            state.join("service-dev.json"),
            r#"{"pid":7,"password":"dev"}"#,
        )
        .unwrap();
        assert_eq!(
            native_service_password(environment, 7).unwrap(),
            Some("dev".to_owned())
        );

        // A matching registration without a password is authoritative and
        // must not fall through to a different channel's config.
        fs::remove_file(state.join("service-dev.json")).unwrap();
        fs::write(state.join("service.json"), r#"{"pid":7}"#).unwrap();
        assert_eq!(native_service_password(environment, 7).unwrap(), None);

        // Without a registration, one channel config is unambiguous.
        fs::remove_file(state.join("service.json")).unwrap();
        fs::remove_file(config.join("service.json")).unwrap();
        fs::write(config.join("service-local.json"), r#"{"password":"local"}"#).unwrap();
        assert_eq!(
            native_service_password(environment, 7).unwrap(),
            Some("local".to_owned())
        );
        fs::write(
            config.join("service-dev.json"),
            r#"{"password":"dev-config"}"#,
        )
        .unwrap();
        assert_eq!(native_service_password(environment, 7).unwrap(), None);

        let custom = root.join("custom");
        fs::create_dir_all(&custom).unwrap();
        fs::write(custom.join("service.json"), r#"{"password":"custom"}"#).unwrap();
        let custom_environment = format!(
            "HOME={}\0OPENCODE_CONFIG_DIR={}\0",
            root.display(),
            custom.display(),
        );
        assert_eq!(
            native_service_password(custom_environment.as_bytes(), 7).unwrap(),
            Some("custom".to_owned())
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_service_password_ignores_malformed_state_but_reports_bad_config() {
        let root =
            std::env::temp_dir().join(format!("{NAME}-service-malformed-{}", std::process::id()));
        let state = root.join("state/opencode");
        let config = root.join("config/opencode");
        fs::create_dir_all(&state).unwrap();
        fs::create_dir_all(&config).unwrap();
        fs::write(config.join("service.json"), r#"{"password":"config"}"#).unwrap();
        let environment = format!(
            "HOME={}\0XDG_STATE_HOME={}\0XDG_CONFIG_HOME={}\0",
            root.display(),
            root.join("state").display(),
            root.join("config").display(),
        );
        let environment = environment.as_bytes();

        // A corrupt state registration is ignored, not fatal.
        fs::write(state.join("service.json"), "{").unwrap();
        assert_eq!(
            native_service_password(environment, 7).unwrap(),
            Some("config".to_owned())
        );

        // A corrupt config is a real configuration error.
        fs::remove_file(state.join("service.json")).unwrap();
        fs::write(config.join("service.json"), r#"{"password":false}"#).unwrap();
        assert!(native_service_password(environment, 7).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn workspace_snapshot_skips_location_requests_without_busy_sessions() {
        let sessions = session_map([(
            "ses_idle",
            json!({"title": "Idle", "time": {"created": 1, "updated": 2}}),
        )]);
        let (url, server) = serve_json(Vec::new());
        let client = test_client();
        let snapshot = workspace_snapshot(
            &client,
            7,
            url,
            None,
            "/tmp/project".to_owned(),
            sessions,
            &serde_json::Map::new(),
        );
        let state = snapshot.state.unwrap();
        assert_eq!(state.status["ses_idle"]["type"], "idle");
        assert!(state.questions.is_empty());
        assert!(state.permissions.is_empty());
        let requests = server.join().unwrap();
        assert!(requests.is_empty());
    }

    #[test]
    fn workspace_snapshot_scopes_requests_for_busy_sessions() {
        let sessions = session_map([(
            "ses_busy",
            json!({"title": "Busy", "time": {"created": 1, "updated": 2}}),
        )]);
        let (url, server) = serve_json(vec![
            json!({"data": [{"type": "assistant", "content": []}]}),
            json!({"data": []}),
            json!({"data": []}),
        ]);
        let client = test_client();
        let active = serde_json::from_value(json!({"ses_busy": {"type": "running"}})).unwrap();
        let snapshot = workspace_snapshot(
            &client,
            7,
            url,
            None,
            "/tmp/project".to_owned(),
            sessions,
            &active,
        );
        let state = snapshot.state.unwrap();
        assert_eq!(state.status["ses_busy"]["type"], "busy");
        assert!(state.unresolved_busy.is_empty());
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(
            requests[0].lines().next().unwrap().starts_with(
                "GET /api/session/ses_busy/message?limit=1&order=desc&type=assistant "
            )
        );
        assert_eq!(
            requests[1].lines().next().unwrap(),
            "GET /api/form?location%5Bdirectory%5D=%2Ftmp%2Fproject HTTP/1.1"
        );
        assert_eq!(
            requests[2].lines().next().unwrap(),
            "GET /api/permission/request?location%5Bdirectory%5D=%2Ftmp%2Fproject HTTP/1.1"
        );
    }

    #[test]
    fn sessions_by_directory_exhausts_pages_and_groups_sessions() {
        let mut first = (0..1_000)
            .map(|index| {
                json!({
                    "id": format!("z{index}"),
                    "location": {"directory": "/z"},
                    "time": {"updated": 20}
                })
            })
            .collect::<Vec<_>>();
        first[0] = json!({
            "id": "a0",
            "location": {"directory": "/a"},
            "time": {"updated": 21}
        });
        let (url, server) = serve_json(vec![
            json!({"data": first, "cursor":{"next":"page-2"}}),
            json!({"data":[
                {"id":"a1", "location":{"directory":"/a"}},
                {"id":"m0", "location":{"directory":"/m"}},
                {"id":"z-extra", "location":{"directory":"/z"}}
            ], "cursor":{"next":null}}),
        ]);
        let client = test_client();
        let api = OpenCodeApi {
            client: &client,
            url: &url,
            authorization: None,
        };
        let sessions = api.sessions_by_directory().unwrap();
        assert_eq!(
            sessions.keys().map(String::as_str).collect::<Vec<_>>(),
            ["/a", "/m", "/z"]
        );
        assert_eq!(sessions["/a"].len(), 2);
        assert_eq!(sessions["/m"].len(), 1);
        assert_eq!(sessions["/z"].len(), 1_000);
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0].lines().next().unwrap(),
            "GET /api/session?limit=1000 HTTP/1.1"
        );
        assert_eq!(
            requests[1].lines().next().unwrap(),
            "GET /api/session?limit=1000&cursor=page-2 HTTP/1.1"
        );
    }

    #[test]
    fn malformed_global_rows_do_not_return_partial_directories() {
        let (url, server) = serve_json(vec![json!({"data":[
                {"id":"valid", "location":{"directory":"/valid"}},
                {"id":"bad", "location":{"directory":42}}
            ],"cursor":{"next":null}})]);
        let client = test_client();
        let api = OpenCodeApi {
            client: &client,
            url: &url,
            authorization: None,
        };
        assert_eq!(
            api.sessions_by_directory().unwrap_err().to_string(),
            "global session has no directory"
        );
        assert_eq!(server.join().unwrap().len(), 1);
    }

    #[test]
    fn cached_verdicts_keep_workspace_metadata_and_failed_scans_blocking() {
        let mut running = workspace_state(&json!({
            "status": {"busy": {"type": "busy"}}, "assistants": {"busy": {"content": []}}
        }));
        running.sessions.insert(
            "busy".to_owned(),
            json!({
                "title": "busy\nwork", "time": {"created": 10, "updated": 20}
            }),
        );
        let scan = ApiScan {
            failures: Vec::new(),
            snapshots: vec![
                ApiSnapshot {
                    pid: 1,
                    url: "http://localhost".to_owned(),
                    directory: "/running\n".to_owned(),
                    state: Ok(running),
                },
                ApiSnapshot {
                    pid: 1,
                    url: "http://localhost".to_owned(),
                    directory: "/failed".to_owned(),
                    state: Err(anyhow::anyhow!("unavailable")),
                },
                ApiSnapshot {
                    pid: 1,
                    url: "http://localhost".to_owned(),
                    directory: "/idle".to_owned(),
                    state: Ok(workspace_state(
                        &json!({"status": {"idle": {"type": "idle"}}}),
                    )),
                },
            ],
        };
        let result = evaluate(
            &scan,
            &[],
            Duration::ZERO,
            &mut HashMap::new(),
            &mut HashSet::new(),
            &|_| {},
        );
        assert_eq!(result.active, 1);
        assert_eq!(result.blockers.len(), 2);
        assert!(result.blockers[1].contains("directory=/failed workspace-scan:unavailable"));
        assert_eq!(result.workspaces.len(), 2);
        assert_eq!(result.workspaces[0].directory, "/running");
        let busy = &result.workspaces[0].sessions[0];
        assert_eq!(
            (busy.session.as_str(), busy.label, busy.title.as_str()),
            ("busy", "RUNNING", "busywork")
        );
        assert_eq!((busy.created, busy.updated), (Some(10), Some(20)));
        assert_eq!(result.workspaces[1].directory, "/idle");
        assert!(result.workspaces[1].sessions.is_empty());
    }

    #[test]
    fn idle_sessions_are_only_shown_after_being_active() {
        let mut scan = ApiScan {
            snapshots: vec![ApiSnapshot {
                pid: 7,
                url: "http://127.0.0.1:1234".to_owned(),
                directory: "/tmp/project".to_owned(),
                state: Ok(workspace_state(&json!({
                    "status": {
                        "busy": {"type": "busy"},
                        "old": {"type": "idle"}
                    }
                }))),
            }],
            failures: Vec::new(),
        };
        let mut tracker = HashMap::new();
        let mut seen_active = HashSet::new();
        let report = |_: String| {};
        let evaluation = evaluate(
            &scan,
            &[],
            Duration::ZERO,
            &mut tracker,
            &mut seen_active,
            &report,
        );
        assert_eq!(evaluation.workspaces[0].sessions.len(), 1);
        assert_eq!(evaluation.workspaces[0].sessions[0].session, "busy");

        if let Ok(state) = &mut scan.snapshots[0].state {
            state.status =
                serde_json::from_str(r#"{"busy":{"type":"idle"},"old":{"type":"idle"}}"#).unwrap();
        }
        let evaluation = evaluate(
            &scan,
            &[],
            Duration::ZERO,
            &mut tracker,
            &mut seen_active,
            &report,
        );
        assert_eq!(evaluation.workspaces[0].sessions.len(), 1);
        assert_eq!(evaluation.workspaces[0].sessions[0].session, "busy");
        assert_eq!(evaluation.workspaces[0].sessions[0].label, "IDLE");
    }

    #[test]
    fn duplicate_sessions_keep_one_row() {
        let scan = ApiScan {
            snapshots: vec![
                ApiSnapshot {
                    pid: 7,
                    url: "http://127.0.0.1:1234".to_owned(),
                    directory: "/tmp/project".to_owned(),
                    state: Ok(workspace_state(&json!({
                        "status": {"shared": {"type": "busy"}}
                    }))),
                },
                ApiSnapshot {
                    pid: 8,
                    url: "http://127.0.0.1:1235".to_owned(),
                    directory: "/tmp/project".to_owned(),
                    state: Ok(workspace_state(&json!({
                        "status": {"shared": {"type": "idle"}}
                    }))),
                },
            ],
            failures: Vec::new(),
        };
        let mut tracker = HashMap::new();
        let mut seen_active = HashSet::new();
        let report = |_: String| {};
        let evaluation = evaluate(
            &scan,
            &[],
            Duration::ZERO,
            &mut tracker,
            &mut seen_active,
            &report,
        );
        assert_eq!(evaluation.active, 1);
        assert_eq!(evaluation.workspaces[0].sessions.len(), 1);
        assert_eq!(evaluation.workspaces[0].sessions[0].label, "RUNNING");
        assert!(evaluation.workspaces[1].sessions.is_empty());
    }

    fn workspace_state(fixture: &Value) -> WorkspaceState {
        WorkspaceState {
            status: fixture["status"].as_object().cloned().unwrap_or_default(),
            sessions: serde_json::Map::new(),
            assistants: fixture["assistants"]
                .as_object()
                .cloned()
                .unwrap_or_default(),
            unresolved_busy: Vec::new(),
            questions: fixture["questions"].as_array().cloned().unwrap_or_default(),
            permissions: fixture["permissions"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
        }
    }

    fn blockers_of(
        state: &WorkspaceState,
        excluded: &HashSet<String>,
        stuck: &HashSet<String>,
    ) -> Vec<String> {
        let waiting = waiting_sessions(
            &state.status,
            &state.questions,
            &state.permissions,
            &state.assistants,
            excluded,
        );
        classify_snapshot(state, excluded, stuck, &waiting)
            .filter_map(|verdict| verdict.blocker)
            .collect()
    }

    #[test]
    fn classifies_disappearing_proc_entries() {
        assert!(proc_entry_gone(&io::Error::from_raw_os_error(libc::ENOENT)));
        assert!(proc_entry_gone(&io::Error::from_raw_os_error(libc::ESRCH)));
        assert!(!proc_entry_gone(&io::Error::from_raw_os_error(
            libc::EACCES
        )));
    }

    #[test]
    fn parses_proc_listener_addresses() {
        assert_eq!(listener_host("0100007F").unwrap(), "127.0.0.1");
        assert_eq!(listener_host("00000000").unwrap(), "127.0.0.1");
        assert_eq!(
            listener_host("00000000000000000000000001000000").unwrap(),
            "[::1]"
        );
        assert_eq!(
            listener_host("00000000000000000000000000000000").unwrap(),
            "[::1]"
        );
        assert!(listener_host("not-an-address").is_err());
    }

    #[test]
    fn classifies_user_waits_and_foreground_task_chains() {
        let fixture: Value = serde_json::from_str(r#"{
          "status":{"background":{"type":"busy"},"busy":{"type":"busy"},"idle":{"type":"idle"},"leaf":{"type":"busy"},"malformed":"oops","malformed_task":{"type":"busy"},"middle":{"type":"busy"},"mixed":{"type":"busy"},"other":{"type":"busy"},"partial":{"type":"busy"},"permission":{"type":"busy"},"root":{"type":"busy"}},
          "questions":[{"sessionID":"leaf"}],"permissions":[{"sessionID":"permission"}],
          "assistants":{
            "background":{
              "info":{"role":"assistant"},
              "content":[{"type":"tool","name":"subagent","sessionID":"background","state":{"status":"running","metadata":{"sessionID":"leaf","background":true}}}]
            },
            "malformed_task":{
              "info":{"role":"assistant"},
              "content":[{"type":"tool","name":"subagent","sessionID":"malformed_task","state":{"status":"running","metadata":{"sessionID":"leaf","background":"false"}}}]
            },
            "middle":
              {"info":{"role":"assistant"},"content":[{"type":"tool","name":"subagent","sessionID":"middle","state":{"status":"running","metadata":{"sessionID":"leaf"}}}]}
            ,
            "mixed":{
              "info":{"role":"assistant"},
              "content":[
                {"type":"tool","name":"subagent","sessionID":"mixed","state":{"status":"running","metadata":{"sessionID":"leaf"}}},
                {"type":"tool","name":"shell","state":{"status":"running"}}
              ]
            },
            "partial":{
              "info":{"role":"assistant"},
              "content":[
                {"type":"tool","name":"subagent","sessionID":"partial","state":{"status":"running","metadata":{"sessionID":"leaf"}}},
                {"type":"tool","name":"subagent","sessionID":"partial","state":{"status":"running","metadata":{"sessionID":"other"}}}
              ]
            },
            "root":{
              "info":{"role":"assistant"},
              "content":[{"type":"tool","name":"subagent","sessionID":"root","state":{"status":"running","metadata":{"sessionID":"middle","background":false}}}]
            }
          }
        }"#).unwrap();
        assert_eq!(
            blockers_of(&workspace_state(&fixture), &HashSet::new(), &HashSet::new()),
            [
                "background:busy",
                "busy:busy",
                "malformed:unknown",
                "malformed_task:busy",
                "mixed:busy",
                "other:busy",
                "partial:busy",
            ]
        );
    }

    #[test]
    fn subagent_background_uses_v2_input_and_legacy_metadata() {
        let input = |value: Value| {
            json!({
                "type": "tool",
                "name": "subagent",
                "state": {"status": "running", "input": {"background": value}}
            })
        };
        assert!(subagent_background(&input(json!(true))));
        assert!(!subagent_background(&input(json!(false))));
        assert!(subagent_background(&input(json!("false"))));
        assert!(!subagent_background(&json!({
            "state": {"status": "running", "input": {}}
        })));
        assert!(subagent_background(&json!({
            "state": {"status": "running", "metadata": {"background": true}}
        })));

        let fixture = |background: Value| {
            json!({
                "status": {"parent": {"type": "busy"}, "leaf": {"type": "busy"}},
                "questions": [{"sessionID": "leaf"}],
                "assistants": {
                    "parent": {
                        "content": [{
                            "type": "tool",
                            "name": "subagent",
                            "state": {
                                "status": "running",
                                "input": {"background": background},
                                "metadata": {"sessionID": "leaf"}
                            }
                        }]
                    }
                }
            })
        };
        assert_eq!(
            blockers_of(
                &workspace_state(&fixture(json!(true))),
                &HashSet::new(),
                &HashSet::new()
            ),
            ["parent:busy"]
        );
        assert!(
            blockers_of(
                &workspace_state(&fixture(json!(false))),
                &HashSet::new(),
                &HashSet::new()
            )
            .is_empty()
        );
    }

    #[test]
    fn dashboard_scrolls_within_bounds() {
        let dashboard = Dashboard {
            termios: None,
            action: None,
            terminal_size: std::cell::Cell::new((80, 24)),
            scroll: std::cell::Cell::new(0),
            page: std::cell::Cell::new(4),
            max_scroll: std::cell::Cell::new(10),
        };
        assert!(dashboard.scroll_by(3));
        assert_eq!(dashboard.scroll.get(), 3);
        assert!(dashboard.scroll_by(-10));
        assert_eq!(dashboard.scroll.get(), 0);
        assert!(dashboard.scroll_by(100));
        assert_eq!(dashboard.scroll.get(), 10);
        assert!(!dashboard.scroll_by(1));
        assert_eq!(dashboard.page.get(), 4);
    }

    #[test]
    fn excludes_sessions_running_the_watchdog() {
        let mut message: Value = serde_json::from_str(
            r#"{
          "content":[{
            "type":"tool",
            "name":"shell",
            "state":{
              "status":"running",
              "input":{"command":"/home/me/.local/bin/opencode-idle-watchdog poweroff"}
            }
          }]
        }"#,
        )
        .unwrap();
        assert!(has_running_watchdog(&message));
        message["content"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "type": "tool",
                "name": "shell",
                "state": { "status": "running", "input": { "command": "sleep 30" } }
            }));
        assert!(!has_running_watchdog(&message));

        let mut excluded = HashSet::new();
        excluded.insert("self".to_owned());
        let status: serde_json::Map<String, Value> =
            serde_json::from_str(r#"{"other":{"type":"busy"},"self":{"type":"busy"}}"#).unwrap();
        let state = WorkspaceState {
            status,
            sessions: serde_json::Map::new(),
            assistants: serde_json::Map::new(),
            unresolved_busy: Vec::new(),
            questions: Vec::new(),
            permissions: Vec::new(),
        };
        assert_eq!(
            blockers_of(&state, &excluded, &HashSet::new()),
            ["other:busy"]
        );
    }

    #[test]
    fn unresolved_busy_blocks_until_stuck() {
        let fixture = serde_json::json!({
            "status": {"wedged": {"type": "busy"}, "idle": {"type": "idle"}},
            "questions": [],
        });
        let mut state = workspace_state(&fixture);
        state.unresolved_busy = vec![UnresolvedBusy {
            session: "wedged".to_owned(),
            updated: Some(1),
            reason: "timeout".to_owned(),
        }];
        assert_eq!(
            blockers_of(&state, &HashSet::new(), &HashSet::new()),
            ["wedged:busy-unresolved"]
        );
        let mut stuck = HashSet::new();
        stuck.insert("wedged".to_owned());
        assert!(blockers_of(&state, &HashSet::new(), &stuck).is_empty());
    }

    #[test]
    fn proven_wait_without_detail_stays_idle() {
        let fixture = serde_json::json!({
            "status": {"wedged": {"type": "busy"}},
            "questions": [{"sessionID": "wedged"}],
        });
        let mut state = workspace_state(&fixture);
        state.unresolved_busy = vec![UnresolvedBusy {
            session: "wedged".to_owned(),
            updated: Some(1),
            reason: "timeout".to_owned(),
        }];
        // A proven wait is never tracked as stuck, so no event can claim it blocks.
        let scan = ApiScan {
            snapshots: vec![ApiSnapshot {
                pid: 7,
                url: "http://127.0.0.1:1234".to_owned(),
                directory: "/tmp/project".to_owned(),
                state: Ok(state),
            }],
            failures: Vec::new(),
        };
        let mut tracker = HashMap::new();
        let mut seen_active = HashSet::new();
        let report = |_: String| {};
        let evaluation = evaluate(
            &scan,
            &[],
            Duration::from_secs(60),
            &mut tracker,
            &mut seen_active,
            &report,
        );
        assert!(evaluation.blockers.is_empty());
        assert_eq!(evaluation.workspaces[0].sessions.len(), 1);
        assert_eq!(evaluation.workspaces[0].sessions[0].label, "WAITING");
        assert!(evaluation.workspaces[0].sessions[0].blocker.is_none());
        assert!(tracker.is_empty());
    }

    #[test]
    fn classifies_transient_request_errors() {
        assert!(is_transient(&ureq::Error::ConnectionFailed));
        assert!(is_transient(&ureq::Error::StatusCode(500)));
        assert!(is_transient(&ureq::Error::StatusCode(429)));
        assert!(!is_transient(&ureq::Error::StatusCode(404)));
        assert!(!is_transient(&ureq::Error::HostNotFound));
    }

    #[test]
    fn stuck_tracker_concedes_sessions_without_progress() {
        let mut scan = ApiScan {
            snapshots: vec![ApiSnapshot {
                pid: 7,
                url: "http://127.0.0.1:1234".to_owned(),
                directory: "/tmp/project".to_owned(),
                state: Ok(WorkspaceState {
                    status: serde_json::from_str(r#"{"wedged":{"type":"busy"}}"#).unwrap(),
                    sessions: serde_json::Map::new(),
                    assistants: serde_json::Map::new(),
                    unresolved_busy: vec![UnresolvedBusy {
                        session: "wedged".to_owned(),
                        updated: Some(1),
                        reason: "timeout".to_owned(),
                    }],
                    questions: Vec::new(),
                    permissions: Vec::new(),
                }),
            }],
            failures: Vec::new(),
        };
        let mut tracker = HashMap::new();
        let mut seen_active = HashSet::new();
        let after = Duration::from_secs(60);
        let report = |_: String| {};
        let evaluation = evaluate(&scan, &[], after, &mut tracker, &mut seen_active, &report);
        assert_eq!(evaluation.workspaces[0].sessions[0].label, "RUNNING");
        assert!(!evaluation.blockers.is_empty());
        tracker.get_mut("wedged").unwrap().first_seen -= after;
        let evaluation = evaluate(&scan, &[], after, &mut tracker, &mut seen_active, &report);
        assert_eq!(evaluation.workspaces[0].sessions[0].label, "STUCK");
        assert!(evaluation.blockers.is_empty());
        // A failed scan observes nothing: the entry survives with its timer intact.
        let failed = ApiScan {
            snapshots: vec![ApiSnapshot {
                pid: 7,
                url: "http://127.0.0.1:1234".to_owned(),
                directory: "/tmp/project".to_owned(),
                state: Err(anyhow::anyhow!("boom")),
            }],
            failures: Vec::new(),
        };
        let evaluation = evaluate(&failed, &[], after, &mut tracker, &mut seen_active, &report);
        assert!(!evaluation.blockers.is_empty());
        assert!(evaluation.workspaces.is_empty());
        assert!(tracker.contains_key("wedged"));
        // The surviving timer still concedes the session on the next good scan.
        let evaluation = evaluate(&scan, &[], after, &mut tracker, &mut seen_active, &report);
        assert_eq!(evaluation.workspaces[0].sessions[0].label, "STUCK");
        // A session the scan sees idle is affirmatively resolved and dropped.
        if let Ok(resolved) = &mut scan.snapshots[0].state {
            resolved.status = serde_json::from_str(r#"{"wedged":{"type":"idle"}}"#).unwrap();
            resolved.unresolved_busy.clear();
        }
        let evaluation = evaluate(&scan, &[], after, &mut tracker, &mut seen_active, &report);
        assert!(evaluation.blockers.is_empty());
        assert_eq!(evaluation.workspaces[0].sessions[0].label, "IDLE");
        assert!(!tracker.contains_key("wedged"));
    }

    #[test]
    fn incomplete_workspace_scan_blocks_idle() {
        let scan = ApiScan {
            snapshots: vec![ApiSnapshot {
                pid: 42,
                url: "http://127.0.0.1:1234".to_owned(),
                directory: "/tmp/project".to_owned(),
                state: Err(anyhow::anyhow!("bad response")),
            }],
            failures: Vec::new(),
        };
        let mut tracker = HashMap::new();
        let mut seen_active = HashSet::new();
        let report = |_: String| {};
        assert_eq!(
            evaluate(
                &scan,
                &[],
                Duration::from_secs(60),
                &mut tracker,
                &mut seen_active,
                &report,
            )
            .blockers,
            ["pid=42 http://127.0.0.1:1234 directory=/tmp/project workspace-scan:bad response"]
        );
    }

    #[test]
    fn trailing_command_owns_its_flags() {
        let options = Options::try_parse_from([NAME, "notify-send", "--urgency", "low"]).unwrap();
        assert_eq!(options.command, ["notify-send", "--urgency", "low"]);
    }

    #[test]
    fn parses_short_control_flags() {
        let options = Options::try_parse_from([NAME, "-i", "5", "-r", "2"]).unwrap();
        assert_eq!(options.interval, 5);
        assert_eq!(options.required_idle_checks, 2);
        assert_eq!(options.stuck_after, DEFAULT_STUCK_AFTER);
        let options = Options::try_parse_from([NAME, "--stuck-after", "0"]).unwrap();
        assert_eq!(options.stuck_after, 0);
    }

    #[test]
    fn parses_ignored_sessions() {
        let options = Options::try_parse_from([
            NAME,
            "--ignore-session",
            "ses_old",
            "--ignore-session",
            "ses_other",
        ])
        .unwrap();
        assert_eq!(options.ignored_sessions, ["ses_old", "ses_other"]);
    }

    #[test]
    fn excludes_the_launching_session_without_duplicates() {
        let ignored = vec!["ses_manual".to_owned()];
        assert_eq!(
            session_exclusions(&ignored, Some("ses_self")),
            ["ses_manual", "ses_self"]
        );
        assert_eq!(
            session_exclusions(&ignored, Some("ses_manual")),
            ["ses_manual"]
        );
        assert_eq!(session_exclusions(&ignored, Some("")), ["ses_manual"]);
        assert_eq!(session_exclusions(&ignored, None), ["ses_manual"]);
    }

    #[test]
    fn resolves_shortcuts_and_preserves_direct_commands() {
        for (shortcut, action) in [
            ("sleep", "suspend"),
            ("suspend", "suspend"),
            ("poweroff", "poweroff"),
            ("hibernate", "hibernate"),
        ] {
            assert_eq!(
                resolve_command(&[shortcut.into()]),
                ["/usr/bin/systemctl", action]
            );
        }
        let direct = ["sleep", "30"].map(OsString::from);
        assert_eq!(resolve_command(&direct), direct);
        let direct = ["poweroff", "--force"].map(OsString::from);
        assert_eq!(resolve_command(&direct), direct);
    }

    #[test]
    fn command_display_shows_resolved_action() {
        let command = resolve_command(&["sleep".into()]);
        assert_eq!(
            command_display(&command),
            "\"/usr/bin/systemctl\" \"suspend\""
        );
    }

    #[test]
    fn rejects_zero_controls() {
        assert!(Options::try_parse_from([NAME, "--interval", "0"]).is_err());
        assert!(Options::try_parse_from([NAME, "--required-idle-checks", "0"]).is_err());
    }

    #[test]
    fn summarizes_overflow() {
        let details = vec!["one".into(), "two".into(), "three".into()];
        assert_eq!(summarize(&details, 2), "one | two | ... +1 more");
    }

    #[test]
    fn cleans_terminal_text() {
        assert_eq!(clean_text("hello\nworld"), "helloworld");
    }
}
