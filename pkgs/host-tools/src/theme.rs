use crate::util;
use anyhow::{Context, Result};
use std::{
    collections::HashSet,
    fs, io,
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

/// Chromium and Electron resolve the system color scheme at startup and only
/// pick up the KDE Wayland value after xdg-desktop-portal emits a real
/// org.freedesktop.portal.Settings change (chromium#462191707, electron#48736).
/// Only a T3 Code cold start on a dark system needs the re-assert: the
/// mis-resolution always lands on light, so with a light system T3 boots
/// correct and any flip would only flash an already-correct desktop.
const STARTUP_DELAY: Duration = Duration::from_secs(3);
const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// Two mains (AppImage parent + mounted child) starting together need one nudge.
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
    // An empty set makes the first scan treat already-running mains as new.
    let mut previous = HashSet::new();
    let mut pending = false;
    let mut last_nudge: Option<Instant> = None;
    loop {
        let mains = collect_t3_mains()?;
        pending |= !mains.is_subset(&previous);
        previous = mains;
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

/// Flip a dark scheme to light and back so the portal re-emits it. Skipped on
/// a light system, where the mis-resolved boot value is already correct.
fn nudge(dconf: &str, state: &State) -> Result<()> {
    let current = util::output(dconf, &["read", SCHEME_KEY])?
        .trim()
        .to_owned();
    if current != "'prefer-dark'" {
        println!("electron-theme-watch: system is light, no nudge needed");
        return Ok(());
    }
    let Some(_lock) = try_lock(&state.lock)? else {
        // Another nudge is running and covers this instance.
        return Ok(());
    };
    // Record the value before changing it: an interrupted or failed nudge is
    // then repaired on the next --recover call or watcher start.
    util::atomic_write(&state.marker, current.as_bytes())?;
    println!("electron-theme-watch: new T3 Code instance on a dark system, nudging system theme");
    apply(dconf, "'default'")?;
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

fn collect_t3_mains() -> Result<HashSet<u32>> {
    let mut mains = HashSet::new();
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
        if is_t3_main(&cmdline) {
            mains.insert(pid);
        }
    }
    Ok(mains)
}

fn is_t3_main(cmdline: &[u8]) -> bool {
    // argv is NUL-separated, except Chromium rewrites helper titles into one
    // NUL-free field; splitting on both covers either shape. argv[0] must be
    // the app binary itself, so a shell merely mentioning a t3code path (or a
    // kernel thread with an empty cmdline) never matches; helpers carry
    // --type= and snapshot workers run a .cjs script.
    let mut args = cmdline
        .split(|byte| *byte == 0 || *byte == b' ')
        .filter(|arg| !arg.is_empty());
    args.next()
        .and_then(|argv0| argv0.rsplit(|byte| *byte == b'/').next())
        .is_some_and(|base| {
            base == b"t3code".as_slice()
                || base.starts_with(b"t3code-")
                || base.starts_with(b"T3-Code")
        })
        && args.all(|arg| !arg.starts_with(b"--type=") && !arg.ends_with(b".cjs"))
}

fn remove(path: &Path) -> Result<()> {
    fs::remove_file(path).with_context(|| format!("remove {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::is_t3_main;

    #[test]
    fn t3_main_matching() {
        // AppImage parent and mounted child, NUL-separated or merged.
        assert!(is_t3_main(
            b"/home/mirsella/.local/share/t3code/app/T3-Code-nightly.AppImage"
        ));
        assert!(is_t3_main(
            b"/home/mirsella/dev/.cache/agent-tools/.mount_T3-CodBfcaIK/t3code"
        ));
        assert!(is_t3_main(
            b"/opt/t3code-nightly-bin/t3code\0--ozone-platform=wayland\0"
        ));
        // Helpers and snapshot workers never match, in either argv shape.
        assert!(!is_t3_main(
            b"/home/mirsella/dev/.cache/agent-tools/.mount_T3-CodBfcaIK/t3code\0--type=renderer\0--user-data-dir=/home/mirsella/.config/t3code-v2"
        ));
        assert!(!is_t3_main(
            b"/home/mirsella/dev/.cache/agent-tools/.mount_T3-CodBfcaIK/t3code --type=zygote"
        ));
        assert!(!is_t3_main(
            b"/opt/t3code-nightly-bin/t3code --type=gpu-process"
        ));
        assert!(!is_t3_main(
            b"/home/mirsella/dev/.cache/agent-tools/.mount_T3-CodBfcaIK/t3code /home/mirsella/dev/.cache/agent-tools/.mount_T3-CodBfcaIK/resources/app.asar/apps/desktop/dist-electron/snapShot/SnapShotAccessibilityWorker.cjs read"
        ));
        // argv[0] rules out bystanders merely mentioning a t3code path.
        assert!(!is_t3_main(
            b"/usr/bin/bash\0-c\0exec -a /opt/t3code-nightly-bin/t3code sleep 30"
        ));
        assert!(!is_t3_main(
            b"less /home/mirsella/.local/share/t3code/app/log"
        ));
        // Kernel threads have an empty cmdline, and other apps are out of scope.
        assert!(!is_t3_main(b""));
        assert!(!is_t3_main(b"firefox"));
        assert!(!is_t3_main(b"/usr/lib/chromium/chromium"));
        assert!(!is_t3_main(b"/usr/lib/chromium/chromium\0--type=renderer"));
    }
}
