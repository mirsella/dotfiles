//! End-to-end coverage for theme-watch's repair, flip and recovery paths,
//! driven through the real binary against a dconf shim and Plasma files.

use std::{
    fs,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const BIN: &str = env!("CARGO_BIN_EXE_host-tools");
const DARK: &str = "[Colors:Window]\nBackgroundNormal=32,35,38\n";
const LIGHT: &str = "[Colors:Window]\nBackgroundNormal=239,240,243\n";

/// A fake session: an isolated runtime dir holding the watcher's state, a
/// config dir holding the Plasma palette, and a watcher killed on drop.
struct Studio {
    root: tempfile::TempDir,
    watcher: Option<Child>,
}

impl Studio {
    fn new(name: &str) -> Self {
        let root = tempfile::Builder::new()
            .prefix(&format!("host-tools-theme-{name}-"))
            .tempdir()
            .unwrap();
        fs::create_dir(root.path().join("runtime")).unwrap();
        fs::create_dir(root.path().join("config")).unwrap();
        fs::create_dir(root.path().join("proc")).unwrap();
        Self {
            root,
            watcher: None,
        }
    }

    fn kdeglobals(&self, contents: &str) {
        fs::write(self.root.path().join("config/kdeglobals"), contents).unwrap();
    }

    fn marker(&self) -> PathBuf {
        self.root
            .path()
            .join("runtime/electron-theme-nudge.original")
    }

    fn set_marker(&self, contents: &str) {
        fs::write(self.marker(), contents).unwrap();
    }

    fn writes(&self) -> Vec<String> {
        let contents = match fs::read_to_string(self.root.path().join("writes.log")) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
            Err(error) => panic!("read dconf writes: {error}"),
        };
        contents.lines().map(str::to_owned).collect()
    }

    fn stored_scheme(&self, value: &str) {
        fs::write(self.root.path().join("scheme"), value).unwrap();
    }

    /// Stateful dconf stand-in: successful writes are visible to later reads.
    fn dconf(&self, read_value: &str) -> PathBuf {
        self.stored_scheme(read_value);
        let shim = self.root.path().join("dconf");
        // Nix supplies its sandbox shell; native Linux builds use /bin/sh.
        let shell = option_env!("HOST_TOOLS_TEST_SHELL").unwrap_or("/bin/sh");
        fs::write(
            &shim,
            format!(
                r#"#!{shell}
case "$1" in
read) cat '{value}' ;;
write)
printf '%s' "$3" > '{attempt}'
if [ -f '{fail}' ] || {{ [ -f '{reject}' ] && [ "$(cat '{reject}')" = "$3" ]; }}; then exit 1; fi
printf '%s' "$3" > '{value}'
echo "$2 $3" >> '{log}' ;;
esac
"#,
                value = self.root.path().join("scheme").display(),
                attempt = self.root.path().join("attempt").display(),
                fail = self.root.path().join("fail-write").display(),
                reject = self.root.path().join("reject-value").display(),
                log = self.root.path().join("writes.log").display()
            ),
        )
        .unwrap();
        fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
        shim
    }

    fn command(&self, dconf: &Path) -> Command {
        let mut command = Command::new(BIN);
        command
            .args(["theme-watch", "--dconf"])
            .arg(dconf)
            .arg("--proc-root")
            .arg(self.root.path().join("proc"))
            .env("XDG_RUNTIME_DIR", self.root.path().join("runtime"))
            .env("XDG_CONFIG_HOME", self.root.path().join("config"));
        command
    }

    fn recover(&self, dconf: &Path) -> std::process::Output {
        self.command(dconf).arg("--recover").output().unwrap()
    }

    /// The watcher, watching this studio instead of the real session.
    fn watcher(&mut self, dconf: &Path) {
        assert!(self.watcher.is_none(), "watcher already running");
        self.watcher = Some(
            self.command(dconf)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
    }

    fn process(&self, pid: u32, cmdline: &[u8]) {
        let process = self.root.path().join(format!("proc/{pid}"));
        fs::create_dir(&process).unwrap();
        fs::write(process.join("cmdline"), cmdline).unwrap();
    }

    fn t3_main(&self) {
        self.process(100, b"/app/t3code\0");
    }

    fn await_writes(&self, count: usize, limit: Duration) -> Vec<String> {
        let start = Instant::now();
        loop {
            let writes = self.writes();
            if writes.len() >= count && !self.marker().exists() {
                return writes;
            }
            assert!(
                start.elapsed() < limit,
                "expected {count} completed writes within {limit:?}, got {writes:?}"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn await_scheme(&self, expected: &str) {
        let start = Instant::now();
        loop {
            let scheme = fs::read_to_string(self.root.path().join("scheme")).unwrap();
            if scheme == expected && !self.marker().exists() {
                return;
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "expected {expected}, got {scheme}"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn await_attempt(&self, expected: &str) {
        let start = Instant::now();
        while fs::read_to_string(self.root.path().join("attempt")).unwrap_or_default() != expected {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "no write of {expected} attempted"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

impl Drop for Studio {
    fn drop(&mut self) {
        if let Some(child) = &mut self.watcher {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The palette is the source of truth, so a scheme switch since an
/// interrupted nudge wins over what that nudge wrote.
#[test]
fn recovers_the_palette_scheme_over_a_stale_marker_value() {
    let studio = Studio::new("palette-wins");
    studio.kdeglobals(LIGHT);
    studio.set_marker("'prefer-dark'");
    let dconf = studio.dconf("'prefer-dark'");

    let output = studio.recover(&dconf);

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        studio.writes(),
        ["/org/gnome/desktop/interface/color-scheme 'prefer-light'"]
    );
    assert!(!studio.marker().exists());
}

/// Without a palette the recorded value is all there is; without either, the
/// marker is dropped instead of failing every watcher start.
#[test]
fn discards_an_unusable_marker_without_a_palette() {
    let studio = Studio::new("garbage-marker");
    studio.set_marker("garbage\n");
    let dconf = studio.dconf("'prefer-dark'");

    let output = studio.recover(&dconf);

    assert!(output.status.success(), "{output:?}");
    assert_eq!(studio.writes(), Vec::<String>::new());
    assert!(!studio.marker().exists());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "electron-theme-watch: discarding an unusable nudge marker"
    );
}

#[test]
fn falls_back_to_the_marker_value_without_a_palette() {
    let studio = Studio::new("marker-fallback");
    studio.set_marker("'prefer-dark'");
    let dconf = studio.dconf("'prefer-dark'");

    let output = studio.recover(&dconf);

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        studio.writes(),
        ["/org/gnome/desktop/interface/color-scheme 'prefer-dark'"]
    );
    assert!(!studio.marker().exists());
}

/// A cold start repairs stale settings immediately and replays after startup,
/// rather than relying on one pulse before Electron has subscribed.
#[test]
fn a_cold_start_repairs_the_stale_scheme_then_nudges() {
    let mut studio = Studio::new("cold-start");
    studio.kdeglobals(DARK);
    let dconf = studio.dconf("'prefer-light'");
    studio.watcher(&dconf);
    studio.t3_main();

    let writes = studio.await_writes(3, Duration::from_secs(20));

    assert_eq!(
        &writes[..3],
        [
            "/org/gnome/desktop/interface/color-scheme 'prefer-dark'",
            "/org/gnome/desktop/interface/color-scheme 'prefer-light'",
            "/org/gnome/desktop/interface/color-scheme 'prefer-dark'",
        ]
    );
    assert!(
        !studio.marker().exists(),
        "the completed nudge cleared its marker"
    );
    let writes = studio.await_writes(7, Duration::from_secs(15));
    assert_eq!(writes.len(), 7, "one repair and three bounded replays");
}

#[test]
fn tracks_palette_changes_and_external_drift_without_a_new_t3_process() {
    let mut studio = Studio::new("continuous");
    studio.kdeglobals(DARK);
    let dconf = studio.dconf("'prefer-light'");
    studio.t3_main();
    studio.watcher(&dconf);
    studio.await_writes(7, Duration::from_secs(15));

    studio.kdeglobals(LIGHT);
    studio.await_scheme("'prefer-light'");
    studio.kdeglobals(DARK);
    studio.await_scheme("'prefer-dark'");

    studio.stored_scheme("'prefer-light'");
    studio.await_scheme("'prefer-dark'");
}

#[test]
fn retries_failed_writes_without_a_new_process() {
    let mut studio = Studio::new("retry");
    studio.kdeglobals(DARK);
    let dconf = studio.dconf("'prefer-light'");
    let fail = studio.root.path().join("fail-write");
    fs::write(&fail, "").unwrap();
    studio.watcher(&dconf);
    // Wait for the attempted write, not an arbitrary startup sleep.
    // The shim records attempts separately from successful writes.
    studio.await_attempt("'prefer-dark'");
    assert!(studio.writes().is_empty());
    fs::remove_file(fail).unwrap();
    studio.await_scheme("'prefer-dark'");
}

#[test]
fn recovers_a_failed_replay_without_restarting_the_watcher() {
    let mut studio = Studio::new("failed-restore");
    studio.kdeglobals(DARK);
    let dconf = studio.dconf("'prefer-dark'");
    let reject = studio.root.path().join("reject-value");
    fs::write(&reject, "'prefer-dark'").unwrap();
    studio.t3_main();
    studio.watcher(&dconf);
    studio.await_attempt("'prefer-dark'");
    assert!(studio.marker().exists());
    fs::remove_file(reject).unwrap();
    studio.await_scheme("'prefer-dark'");
}

#[test]
fn a_late_renderer_replays_even_on_a_light_system() {
    let mut studio = Studio::new("late-renderer");
    studio.kdeglobals(LIGHT);
    let dconf = studio.dconf("'prefer-light'");
    studio.t3_main();
    studio.watcher(&dconf);
    studio.await_writes(6, Duration::from_secs(15));
    studio.process(101, b"/app/t3code\0--type=renderer\0");
    let writes = studio.await_writes(8, Duration::from_secs(5));
    assert_eq!(
        &writes[6..8],
        [
            "/org/gnome/desktop/interface/color-scheme 'prefer-dark'",
            "/org/gnome/desktop/interface/color-scheme 'prefer-light'",
        ]
    );
}

#[test]
fn a_restarted_portal_replays_for_an_existing_window() {
    let mut studio = Studio::new("portal-restart");
    studio.kdeglobals(DARK);
    let dconf = studio.dconf("'prefer-dark'");
    studio.t3_main();
    studio.watcher(&dconf);
    studio.await_writes(6, Duration::from_secs(15));
    studio.process(200, b"/usr/lib/xdg-desktop-portal-gtk\0");
    let writes = studio.await_writes(8, Duration::from_secs(5));
    assert_eq!(
        &writes[6..8],
        [
            "/org/gnome/desktop/interface/color-scheme 'prefer-light'",
            "/org/gnome/desktop/interface/color-scheme 'prefer-dark'",
        ]
    );
}
