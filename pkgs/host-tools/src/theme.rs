//! Chromium and Electron resolve the system color scheme at startup and only
//! pick up the KDE Wayland value after xdg-desktop-portal emits a real
//! org.freedesktop.portal.Settings change (chromium#462191707, electron#48736).
//! The flip is needed only on a dark system, where a correct scheme emits no
//! change at all; a light one is set right by the repair write, which is a
//! change in itself.
//!
//! The portal serves the scheme from gsettings, which kde-gtk-config is meant
//! to mirror from the Plasma color scheme. Its kded module can sit loaded yet
//! idle after a kded crash storm, leaving gsettings frozen at an old value, so
//! the Plasma palette is the source of truth here and gsettings is repaired
//! against it before the flip.

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
    let marker = match fs::read(&state.marker) {
        Ok(marker) => marker,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("read {}", state.marker.display()));
        }
    };
    // The marker only records that a nudge was interrupted; the scheme to
    // restore is the current Plasma one, which may have changed since the
    // scheme the interrupted nudge intended. Its recorded value is just that
    // scheme's fallback, so anything other than a known one carries no
    // information beyond the interruption itself.
    let intended = String::from_utf8(marker)
        .ok()
        .filter(|intended| matches!(intended.as_str(), "'prefer-dark'" | "'prefer-light'"));
    let scheme = match plasma_scheme() {
        Some(scheme) => scheme,
        None => match intended.as_deref() {
            Some(scheme) => scheme,
            None => {
                // Nothing to restore to: drop the marker, and with it the
                // flipped value's record, instead of failing this watcher's
                // every start.
                eprintln!("electron-theme-watch: discarding an unusable nudge marker");
                return remove(&state.marker);
            }
        },
    };
    apply(dconf, scheme)?;
    remove(&state.marker)
}

/// Flip a dark scheme to light and back so the portal re-emits it.
fn nudge(dconf: &str, state: &State) -> Result<()> {
    let current = util::output(dconf, &["read", SCHEME_KEY])?
        .trim()
        .to_owned();
    let desired = match plasma_scheme() {
        Some(desired) => desired,
        // No readable palette outside a Plasma session: keep trusting gsettings.
        None => current.as_str(),
    };
    // A stale value pins T3 Code to the previous appearance across system theme
    // switches even when the flip below is perfect, so repair it first. The
    // repair is idempotent, so it needs no lock.
    if desired != current {
        println!(
            "electron-theme-watch: portal scheme is stale ({current}), repairing to {desired}"
        );
        apply(dconf, desired)?;
    }
    if desired != "'prefer-dark'" {
        println!("electron-theme-watch: system is light, no nudge needed");
        return Ok(());
    }
    let Some(_lock) = try_lock(&state.lock)? else {
        // Another nudge is running and covers this instance.
        return Ok(());
    };
    // Record the value before changing it: an interrupted or failed nudge is
    // then repaired on the next --recover call or watcher start.
    util::atomic_write(&state.marker, desired.as_bytes())?;
    println!("electron-theme-watch: new T3 Code instance on a dark system, nudging system theme");
    apply(dconf, "'default'")?;
    thread::sleep(ROLLBACK_DELAY);
    apply(dconf, desired)?;
    remove(&state.marker)
}

/// The scheme the portal must serve, taken from the Plasma palette in
/// kdeglobals: the resolved colors Plasma itself renders with, independent of
/// whether kde-gtk-config's kded module is alive to mirror them.
fn plasma_scheme() -> Option<&'static str> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    scheme_for_palette(&fs::read_to_string(config.join("kdeglobals")).ok()?)
}

/// Decide dark or light from a kdeglobals palette. Window is what the desktop
/// renders against; View and Button answer when it is missing.
fn scheme_for_palette(kdeglobals: &str) -> Option<&'static str> {
    for group in ["Colors:Window", "Colors:View", "Colors:Button"] {
        if let Some(rgb) = palette_background(kdeglobals, group) {
            return Some(if is_dark(rgb) {
                "'prefer-dark'"
            } else {
                "'prefer-light'"
            });
        }
    }
    None
}

fn palette_background(kdeglobals: &str, group: &str) -> Option<(u32, u32, u32)> {
    let mut in_group = false;
    for line in kdeglobals.lines() {
        let line = line.trim();
        if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_group = section == group;
        } else if in_group {
            if let Some(value) = line.strip_prefix("BackgroundNormal=") {
                let mut channels = value
                    .trim()
                    .split(',')
                    .map(|c| c.trim().parse::<u32>().ok());
                let rgb = (channels.next()??, channels.next()??, channels.next()??);
                return channels.next().is_none().then_some(rgb);
            }
        }
    }
    None
}

/// Rec. 709 luma of a Qt color triple on a 0-255 scale; dark below 128.
fn is_dark((red, green, blue): (u32, u32, u32)) -> bool {
    2126 * red + 7152 * green + 722 * blue < 1_280_000
}

fn apply(dconf: &str, value: &str) -> Result<()> {
    util::run(dconf, &["write", SCHEME_KEY, value])
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
    use super::{is_dark, is_t3_main, scheme_for_palette};

    #[test]
    fn t3_main_matching() {
        let matched: [&[u8]; 3] = [
            // AppImage parent and mounted child, NUL-separated or merged.
            b"/home/mirsella/.local/share/t3code/app/T3-Code-nightly.AppImage",
            b"/home/mirsella/dev/.cache/agent-tools/.mount_T3-CodBfcaIK/t3code",
            b"/opt/t3code-nightly-bin/t3code\0--ozone-platform=wayland\0",
        ];
        let unmatched: [&[u8]; 10] = [
            // Helpers and snapshot workers never match, in either argv shape.
            b"/home/mirsella/dev/.cache/agent-tools/.mount_T3-CodBfcaIK/t3code\0--type=renderer\0--user-data-dir=/home/mirsella/.config/t3code-v2",
            b"/home/mirsella/dev/.cache/agent-tools/.mount_T3-CodBfcaIK/t3code --type=zygote",
            b"/opt/t3code-nightly-bin/t3code --type=gpu-process",
            b"/home/mirsella/dev/.cache/agent-tools/.mount_T3-CodBfcaIK/t3code /home/mirsella/dev/.cache/agent-tools/.mount_T3-CodBfcaIK/resources/app.asar/apps/desktop/dist-electron/snapShot/SnapShotAccessibilityWorker.cjs read",
            // argv[0] rules out bystanders merely mentioning a t3code path.
            b"/usr/bin/bash\0-c\0exec -a /opt/t3code-nightly-bin/t3code sleep 30",
            b"less /home/mirsella/.local/share/t3code/app/log",
            // Kernel threads have an empty cmdline, and other apps are out of scope.
            b"",
            b"firefox",
            b"/usr/lib/chromium/chromium",
            b"/usr/lib/chromium/chromium\0--type=renderer",
        ];
        for cmdline in matched {
            assert!(is_t3_main(cmdline), "{}", String::from_utf8_lossy(cmdline));
        }
        for cmdline in unmatched {
            assert!(!is_t3_main(cmdline), "{}", String::from_utf8_lossy(cmdline));
        }
    }

    #[test]
    fn palette_scheme_follows_the_kde_background() {
        let cases = [
            // The palette Plasma itself renders with, Breeze dark vs light values.
            (
                "[Colors:Window]\nBackgroundNormal=32,35,38\nForegroundNormal=252,252,252\n[ColorEffects:Disabled]\nColor=56,56,56\n",
                Some("'prefer-dark'"),
            ),
            (
                "[Colors:Window]\nBackgroundNormal=239,240,243\nForegroundNormal=35,38,41\n",
                Some("'prefer-light'"),
            ),
            // View and Button answer when Window is missing, in that order.
            (
                "[Colors:View]\nBackgroundNormal=20,22,24\n",
                Some("'prefer-dark'"),
            ),
            (
                "[Colors:Button]\nBackgroundNormal=239,240,243\n",
                Some("'prefer-light'"),
            ),
            // No palette for the group, or no palette at all: no verdict.
            ("[Colors:Window]\nForegroundNormal=1,2,3\n", None),
            ("", None),
            // Keys before the target group belong to no group and never answer;
            // kdeglobals places [General] among the color groups.
            (
                "BackgroundNormal=255,255,255\n[Colors:Button]\nBackgroundNormal=32,35,38\n",
                Some("'prefer-dark'"),
            ),
            // A malformed triple is no verdict rather than a wrong one.
            ("[Colors:Window]\nBackgroundNormal=32,35\n", None),
            ("[Colors:Window]\nBackgroundNormal=32,35,38,1\n", None),
            // Mid gray lands on light; the threshold is exclusive.
            (
                "[Colors:Window]\nBackgroundNormal=128,128,128\n",
                Some("'prefer-light'"),
            ),
        ];
        for (kdeglobals, expected) in cases {
            assert_eq!(
                scheme_for_palette(kdeglobals),
                expected,
                "for {kdeglobals:?}"
            );
        }
    }

    #[test]
    fn luma_threshold_splits_dark_from_light() {
        assert!(is_dark((0, 0, 0)));
        assert!(!is_dark((255, 255, 255)));
    }
}
