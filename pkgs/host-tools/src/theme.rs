//! Chromium and Electron resolve the system color scheme at startup and only
//! pick up the KDE Wayland value after xdg-desktop-portal emits a real
//! org.freedesktop.portal.Settings change (chromium#462191707, electron#48736).
//! Reconcile Plasma continuously, not just when an application starts. Replay
//! real light/dark transitions for new mains/renderers and restarted portals,
//! with bounded retries while Electron is still subscribing at startup.
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
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// Time the opposite value stays visible; long enough for the portal to emit.
const ROLLBACK_DELAY: Duration = Duration::from_millis(250);
const SCHEME_KEY: &str = "/org/gnome/desktop/interface/color-scheme";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scheme {
    Dark,
    Light,
}

impl Scheme {
    fn value(self) -> &'static str {
        match self {
            Self::Dark => "'prefer-dark'",
            Self::Light => "'prefer-light'",
        }
    }

    fn parse(value: &[u8]) -> Option<Self> {
        [Self::Dark, Self::Light]
            .into_iter()
            .find(|scheme| scheme.value().as_bytes() == value)
    }

    fn opposite(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
        }
    }
}

pub fn run(dconf: &str, proc_root: &Path, recover: bool) -> Result<()> {
    let state = State::resolve();
    if recover {
        let Some(_lock) = try_lock(&state.lock)? else {
            return Ok(());
        };
        return restore_interrupted(dconf, &state);
    }
    let mut watcher = Watcher::default();
    loop {
        if let Err(error) = watcher.tick(dconf, proc_root, &state) {
            // Keep pending work: a failed write or unavailable portal must not
            // consume the only opportunity to correct an already-open window.
            eprintln!("electron-theme-watch: {error:#}; retrying");
        }
        thread::sleep(POLL_INTERVAL);
    }
}

#[derive(Default)]
struct Watcher {
    previous: Processes,
    scheme: Option<Scheme>,
    replay: Option<Replay>,
}

struct Replay {
    next: Instant,
    delays: std::array::IntoIter<u64, 2>,
}

impl Watcher {
    fn tick(&mut self, dconf: &str, proc_root: &Path, state: &State) -> Result<()> {
        // Hold one lock across recovery, reconciliation and replay: another
        // watcher must not mistake an in-flight pulse for external drift.
        let Some(_lock) = try_lock(&state.lock)? else {
            return Ok(());
        };
        restore_interrupted(dconf, state)?;
        let desired = plasma_scheme().context("cannot read the Plasma color palette")?;
        let current = util::output(dconf, &["read", SCHEME_KEY])?;
        let drifted = current.trim() != desired.value();
        if drifted {
            println!(
                "electron-theme-watch: repairing portal scheme to {}",
                desired.value()
            );
            apply(dconf, desired)?;
        }

        let processes = collect_processes(proc_root)?;
        if processes.apps.is_empty() {
            self.replay = None;
        } else if drifted
            || self.scheme != Some(desired)
            || !processes.apps.is_subset(&self.previous.apps)
            || !processes.portals.is_subset(&self.previous.portals)
        {
            // Immediate, +2s, +8s: cover late subscriptions without flashing
            // applications indefinitely. Failed writes do not advance this.
            self.replay = Some(Replay {
                next: Instant::now(),
                delays: [2, 6].into_iter(),
            });
        }
        self.previous = processes;
        self.scheme = Some(desired);

        if let Some(replay) = &mut self.replay
            && Instant::now() >= replay.next
        {
            nudge(dconf, state, desired)?;
            match replay.delays.next() {
                Some(delay) => replay.next = Instant::now() + Duration::from_secs(delay),
                None => self.replay = None,
            }
        }
        Ok(())
    }
}

/// Repair an interrupted nudge. The caller holds the state lock.
fn restore_interrupted(dconf: &str, state: &State) -> Result<()> {
    let marker = match fs::read(&state.marker) {
        Ok(marker) => marker,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("read {}", state.marker.display()));
        }
    };
    // Plasma may have switched since the pulse began; use the recorded scheme
    // only when the current palette is unavailable.
    let scheme = match (plasma_scheme(), Scheme::parse(&marker)) {
        (Some(scheme), _) => scheme,
        (None, Some(scheme)) => {
            eprintln!(
                "electron-theme-watch: Plasma palette unavailable; recovering recorded scheme {}",
                scheme.value()
            );
            scheme
        }
        (None, None) => {
            eprintln!("electron-theme-watch: discarding an unusable nudge marker");
            return remove(&state.marker);
        }
    };
    apply(dconf, scheme)?;
    remove(&state.marker)
}

/// Explicit opposites work in both directions; 'default' need not mean light.
fn nudge(dconf: &str, state: &State, desired: Scheme) -> Result<()> {
    // Record the value before changing it: an interrupted or failed nudge is
    // then repaired on the next tick or --recover call.
    util::atomic_write(&state.marker, desired.value().as_bytes())?;
    println!(
        "electron-theme-watch: replaying system theme {}",
        desired.value()
    );
    apply(dconf, desired.opposite())?;
    thread::sleep(ROLLBACK_DELAY);
    // Plasma may have switched again during the pulse. Never roll it back to
    // the old palette; recovery uses this same source of truth after failure.
    apply(
        dconf,
        plasma_scheme().context("Plasma palette disappeared during replay")?,
    )?;
    remove(&state.marker)
}

/// The scheme the portal must serve, taken from the Plasma palette in
/// kdeglobals: the resolved colors Plasma itself renders with, independent of
/// whether kde-gtk-config's kded module is alive to mirror them.
fn plasma_scheme() -> Option<Scheme> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    scheme_for_palette(&fs::read_to_string(config.join("kdeglobals")).ok()?)
}

/// Decide dark or light from a kdeglobals palette. Window is what the desktop
/// renders against; View and Button answer when it is missing.
fn scheme_for_palette(kdeglobals: &str) -> Option<Scheme> {
    for group in ["Colors:Window", "Colors:View", "Colors:Button"] {
        if let Some(rgb) = palette_background(kdeglobals, group) {
            return Some(if is_dark(rgb) {
                Scheme::Dark
            } else {
                Scheme::Light
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
                    .map(|c| c.trim().parse::<u8>().ok().map(u32::from));
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

fn apply(dconf: &str, scheme: Scheme) -> Result<()> {
    util::run(dconf, &["write", SCHEME_KEY, scheme.value()])
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
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(fs::TryLockError::WouldBlock) => Ok(None),
        Err(fs::TryLockError::Error(error)) => Err(error).context("lock theme replay"),
    }
}

#[derive(Default)]
struct Processes {
    apps: HashSet<u32>,
    portals: HashSet<u32>,
}

fn collect_processes(proc_root: &Path) -> Result<Processes> {
    let mut processes = Processes::default();
    let uid = unsafe { libc::geteuid() };
    for entry in fs::read_dir(proc_root).with_context(|| format!("read {}", proc_root.display()))? {
        let Ok(entry) = entry else { continue };
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|pid| pid.parse::<u32>().ok())
        else {
            continue;
        };
        if !entry.metadata().is_ok_and(|metadata| metadata.uid() == uid) {
            continue;
        }
        // Exited processes vanish between read_dir and read.
        let Ok(cmdline) = fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        if is_t3_window_process(&cmdline) {
            processes.apps.insert(pid);
        }
        let argv0 = cmdline.split(|byte| *byte == 0).next().unwrap_or_default();
        if matches!(
            argv0.rsplit(|byte| *byte == b'/').next(),
            Some(b"xdg-desktop-portal" | b"xdg-desktop-portal-gtk")
        ) {
            processes.portals.insert(pid);
        }
    }
    Ok(processes)
}

fn is_t3_window_process(cmdline: &[u8]) -> bool {
    // argv is NUL-separated, except Chromium rewrites helper titles into one
    // NUL-free field; splitting on both covers either shape. argv[0] must be
    // the app binary itself, so a shell merely mentioning a t3code path (or a
    // kernel thread with an empty cmdline) never matches. Renderers matter too:
    // a new window can subscribe long after the main process has started.
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
        && args.all(|arg| {
            (!arg.starts_with(b"--type=") || arg == b"--type=renderer") && !arg.ends_with(b".cjs")
        })
}

fn remove(path: &Path) -> Result<()> {
    fs::remove_file(path).with_context(|| format!("remove {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{
        Scheme, State, Watcher, is_dark, is_t3_window_process, scheme_for_palette, try_lock,
    };

    #[test]
    fn scheme_values_roundtrip_and_have_explicit_opposites() {
        for scheme in [Scheme::Dark, Scheme::Light] {
            assert_eq!(Scheme::parse(scheme.value().as_bytes()), Some(scheme));
            assert_ne!(scheme.opposite(), scheme);
            assert_eq!(scheme.opposite().opposite(), scheme);
        }
        assert_eq!(Scheme::parse(b"'default'"), None);
        assert_eq!(Scheme::parse(b"\xff"), None);
    }

    #[test]
    fn busy_state_lock_defers_the_entire_reconciliation() {
        let root = tempfile::tempdir().unwrap();
        let state = State {
            lock: root.path().join("lock"),
            marker: root.path().join("marker"),
        };
        let lock = try_lock(&state.lock).unwrap().unwrap();
        assert!(try_lock(&state.lock).unwrap().is_none());
        // A busy watcher must not inspect processes or call dconf at all.
        Watcher::default()
            .tick("missing-dconf", &root.path().join("missing-proc"), &state)
            .unwrap();
        drop(lock);
        assert!(try_lock(&state.lock).unwrap().is_some());
    }

    #[test]
    fn t3_main_matching() {
        let matched: [&[u8]; 4] = [
            // AppImage parent and mounted child, NUL-separated or merged.
            b"/home/mirsella/.local/share/t3code/app/T3-Code-nightly.AppImage",
            b"/home/mirsella/dev/.cache/agent-tools/.mount_T3-CodBfcaIK/t3code",
            b"/opt/t3code-nightly-bin/t3code\0--ozone-platform=wayland\0",
            b"/opt/t3code-nightly-bin/t3code\0--type=renderer\0",
        ];
        let unmatched: [&[u8]; 9] = [
            // Helpers and snapshot workers never match, in either argv shape.
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
            assert!(
                is_t3_window_process(cmdline),
                "{}",
                String::from_utf8_lossy(cmdline)
            );
        }
        for cmdline in unmatched {
            assert!(
                !is_t3_window_process(cmdline),
                "{}",
                String::from_utf8_lossy(cmdline)
            );
        }
    }

    #[test]
    fn palette_scheme_follows_the_kde_background() {
        let cases = [
            // The palette Plasma itself renders with, Breeze dark vs light values.
            (
                "[Colors:Window]\nBackgroundNormal=32,35,38\nForegroundNormal=252,252,252\n[ColorEffects:Disabled]\nColor=56,56,56\n",
                Some(Scheme::Dark),
            ),
            (
                "[Colors:Window]\nBackgroundNormal=239,240,243\nForegroundNormal=35,38,41\n",
                Some(Scheme::Light),
            ),
            // View and Button answer when Window is missing, in that order.
            (
                "[Colors:View]\nBackgroundNormal=20,22,24\n",
                Some(Scheme::Dark),
            ),
            (
                "[Colors:Button]\nBackgroundNormal=239,240,243\n",
                Some(Scheme::Light),
            ),
            // No palette for the group, or no palette at all: no verdict.
            ("[Colors:Window]\nForegroundNormal=1,2,3\n", None),
            ("", None),
            // Keys before the target group belong to no group and never answer;
            // kdeglobals places [General] among the color groups.
            (
                "BackgroundNormal=255,255,255\n[Colors:Button]\nBackgroundNormal=32,35,38\n",
                Some(Scheme::Dark),
            ),
            // A malformed triple is no verdict rather than a wrong one.
            ("[Colors:Window]\nBackgroundNormal=32,35\n", None),
            ("[Colors:Window]\nBackgroundNormal=32,35,38,1\n", None),
            ("[Colors:Window]\nBackgroundNormal=256,35,38\n", None),
            // Mid gray lands on light; the threshold is exclusive.
            (
                "[Colors:Window]\nBackgroundNormal=128,128,128\n",
                Some(Scheme::Light),
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
