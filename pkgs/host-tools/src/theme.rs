use crate::util;
use anyhow::{Context, Result, bail};
use std::{
    fs, io,
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

/// Chromium and Electron resolve the system color scheme at startup and only
/// pick up the KDE Wayland value after xdg-desktop-portal emits a real
/// org.freedesktop.portal.Settings change (chromium#462191707, electron#48736).
const STARTUP_DELAY: Duration = Duration::from_secs(3);
const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// A burst of new renderers still needs only one nudge.
const DEBOUNCE: Duration = Duration::from_secs(10);
/// Time the opposite value stays visible; long enough for the portal to emit.
const ROLLBACK_DELAY: Duration = Duration::from_millis(250);
const SCHEME_KEY: &str = "/org/gnome/desktop/interface/color-scheme";

pub fn run(dconf: &str, recover: bool) -> Result<()> {
    let state = State::resolve();
    restore_interrupted(dconf, &state)?;
    if recover {
        return Ok(());
    }
    // Let the session's portal services come up before the first scan.
    thread::sleep(STARTUP_DELAY);
    let mut previous = None;
    let mut pending = false;
    let mut last_nudge: Option<Instant> = None;
    loop {
        let count = count_renderers()?;
        // Apps already running when the first scan happens still need the nudge.
        pending |= previous.map_or(count > 0, |last| count > last);
        previous = Some(count);
        if pending && last_nudge.is_none_or(|last| last.elapsed() >= DEBOUNCE) {
            pending = false;
            last_nudge = Some(Instant::now());
            if let Err(error) = nudge(dconf, &state) {
                eprintln!("electron-theme-watch: {error:#}");
            }
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// Repair a color scheme left flipped by a nudge that died between its writes.
fn restore_interrupted(dconf: &str, state: &State) -> Result<()> {
    let Some(_lock) = try_lock(&state.lock)? else {
        // A nudge is running and will restore the value itself.
        return Ok(());
    };
    let original = match fs::read(&state.marker) {
        Ok(original) => original,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("read {}", state.marker.display()));
        }
    };
    let original =
        String::from_utf8(original).with_context(|| format!("parse {}", state.marker.display()))?;
    apply(dconf, &original)?;
    remove(&state.marker)
}

fn nudge(dconf: &str, state: &State) -> Result<()> {
    let Some(_lock) = try_lock(&state.lock)? else {
        // Another nudge is running and covers this renderer.
        return Ok(());
    };
    let current = util::output(dconf, &["read", SCHEME_KEY])?
        .trim()
        .to_owned();
    // An unset key reads as empty and behaves like the explicit "default" value.
    let scheme = if current.is_empty() {
        "'default'"
    } else {
        &current
    };
    let Some(other) = next_scheme(scheme) else {
        bail!("unexpected color-scheme value: {scheme}");
    };
    // Record the value before changing it: an interrupted or failed nudge is
    // then repaired on the next --recover call or watcher start.
    util::atomic_write(&state.marker, current.as_bytes())?;
    println!("electron-theme-watch: new Chromium/Electron renderer, nudging system theme");
    apply(dconf, other)?;
    thread::sleep(ROLLBACK_DELAY);
    apply(dconf, &current)?;
    remove(&state.marker)
}

fn apply(dconf: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        util::run(dconf, &["reset", SCHEME_KEY])
    } else {
        util::run(dconf, &["write", SCHEME_KEY, value])
    }
}

fn next_scheme(current: &str) -> Option<&'static str> {
    match current {
        "'prefer-dark'" => Some("'default'"),
        "'default'" | "'prefer-light'" => Some("'prefer-dark'"),
        _ => None,
    }
}

struct State {
    lock: PathBuf,
    marker: PathBuf,
}

impl State {
    fn resolve() -> Self {
        let (dir, tag) = match std::env::var_os("XDG_RUNTIME_DIR") {
            // Manual runs may fall back to a shared temp dir, so keep users apart.
            None => (
                std::env::temp_dir(),
                format!("-{}", unsafe { libc::geteuid() }),
            ),
            Some(dir) => (PathBuf::from(dir), String::new()),
        };
        Self {
            lock: dir.join(format!("electron-theme-nudge{tag}.lock")),
            marker: dir.join(format!("electron-theme-nudge{tag}.original")),
        }
    }
}

fn try_lock(path: &Path) -> Result<Option<fs::File>> {
    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(path)
        .with_context(|| format!("open {}", path.display()))?;
    // The lock lives as long as the handle; a busy lock means a nudge is running.
    let locked = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    Ok(locked.then_some(file))
}

fn count_renderers() -> Result<usize> {
    let mut count = 0;
    for entry in fs::read_dir("/proc").context("read /proc")? {
        let Ok(entry) = entry else { continue };
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|pid| pid.parse::<u32>().ok())
        else {
            continue;
        };
        // Exited processes vanish between read_dir and read.
        let Ok(cmdline) = fs::read(format!("/proc/{pid}/cmdline")) else {
            continue;
        };
        if is_renderer(&cmdline) {
            count += 1;
        }
    }
    Ok(count)
}

fn is_renderer(cmdline: &[u8]) -> bool {
    // Match substrings: Chromium rewrites argv for its process title and the
    // kernel then reports the whole command line as one field without NULs.
    contains(cmdline, b"--type=renderer") && !contains(cmdline, b"--headless")
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn remove(path: &Path) -> Result<()> {
    fs::remove_file(path).with_context(|| format!("remove {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{is_renderer, next_scheme};

    #[test]
    fn next_scheme_always_changes_the_value() {
        assert_eq!(next_scheme("'prefer-dark'"), Some("'default'"));
        assert_eq!(next_scheme("'default'"), Some("'prefer-dark'"));
        assert_eq!(next_scheme("'prefer-light'"), Some("'prefer-dark'"));
        assert_eq!(next_scheme("'other'"), None);
    }

    #[test]
    fn renderer_matching_handles_split_and_merged_cmdlines() {
        assert!(is_renderer(b"electron\0--type=renderer\0--no-sandbox"));
        // Chromium rewrites argv into one NUL-free field for its process title.
        assert!(is_renderer(
            b"electron --type=renderer --user-data-dir=/tmp"
        ));
        assert!(!is_renderer(b"electron\0--type=zygote\0"));
        assert!(!is_renderer(b"chrome\0--headless\0--type=renderer\0"));
    }
}
