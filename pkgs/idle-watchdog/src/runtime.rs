use std::env;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use jiff::Zoned;
use signal_hook::consts::{SIGINT, SIGTERM};

pub const DEFAULT_INTERVAL: u64 = 30;
pub const DEFAULT_IDLE_CHECKS: u32 = 3;
pub const REQUEST_TIMEOUT_SECS: u64 = 10;
pub const REQUEST_ATTEMPTS: u32 = 3;

pub struct State {
    root: PathBuf,
    log_file: PathBuf,
}

impl State {
    pub fn discover(name: &str) -> Result<Self> {
        let home = env::var_os("HOME").context("HOME is not set")?;
        let cache = env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(home).join(".cache"));
        let root = cache.join(name);
        Ok(Self {
            log_file: root.join("watchdog.log"),
            root,
        })
    }

    pub fn log(&self, message: impl AsRef<str>) {
        let line = self.timestamped(message);
        println!("{line}");
        let _ = io::stdout().flush();
        self.write_log(&line);
    }

    pub fn record(&self, message: impl AsRef<str>) {
        self.write_log(&self.timestamped(message));
    }

    pub fn root(&self) -> &Path {
        &self.root
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

pub struct Cancellation {
    signal: Arc<AtomicUsize>,
}

impl Cancellation {
    pub fn new() -> Result<Self> {
        let signal = Arc::new(AtomicUsize::new(0));
        for number in [SIGINT, SIGTERM] {
            signal_hook::flag::register_usize(number, Arc::clone(&signal), number as usize)
                .with_context(|| format!("install signal handler for {number}"))?;
        }
        Ok(Self { signal })
    }

    pub fn wait(&self, duration: Duration) -> Option<i32> {
        let started = std::time::Instant::now();
        loop {
            let signal = self.signal.load(Ordering::Relaxed);
            if signal != 0 {
                return i32::try_from(signal).ok();
            }
            let remaining = duration.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return None;
            }
            thread::sleep(remaining.min(Duration::from_millis(10)));
        }
    }
}

pub fn exit_with(name: &str, result: Result<i32>) -> ! {
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            if let Ok(state) = State::discover(name) {
                state.log(format!("watchdog crashed: {error:#}"));
            }
            eprintln!("{name}: {error:#}");
            1
        }
    };
    std::process::exit(code)
}

pub fn signal_exit(state: &State, signal: i32) -> ! {
    state.log(if signal == 0 {
        "watchdog exited"
    } else if signal == SIGINT {
        "watchdog interrupted"
    } else {
        "watchdog terminated"
    });
    // Termination must not unwind through API-specific drop handlers. Token
    // revocation and other cleanup can block long enough to delay the shell.
    std::process::exit(if signal == 0 { 0 } else { 128 + signal });
}

pub fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

pub fn command_display(command: &[OsString]) -> String {
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

pub fn resolve_command(command: &[OsString]) -> Vec<OsString> {
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

pub fn run_command(state: &State, cancellation: &Cancellation, command: &[OsString]) -> i32 {
    if let Some(signal) = cancellation.wait(Duration::ZERO) {
        signal_exit(state, signal);
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
            signal_exit(state, signal);
        }
    }
}

pub fn http_client() -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(REQUEST_TIMEOUT_SECS)))
            .build(),
    )
}

pub fn summarize(items: &[String], limit: usize) -> String {
    let mut visible: Vec<_> = items.iter().take(limit).cloned().collect();
    if items.len() > limit {
        visible.push(format!("... +{} more", items.len() - limit));
    }
    visible.join(" | ")
}

pub fn clean_text(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_overflow() {
        let details = ["one".to_owned(), "two".to_owned(), "three".to_owned()];
        assert_eq!(summarize(&details, 2), "one | two | ... +1 more");
    }

    #[test]
    fn cleans_terminal_text() {
        assert_eq!(clean_text("hello\nworld"), "helloworld");
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
                [systemctl_command(), OsString::from(action)]
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
            format!("{:?} \"suspend\"", systemctl_command())
        );
    }
}
