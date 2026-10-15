//! End-to-end coverage for theme-watch's repair, flip and recovery paths,
//! driven through the real binary against a dconf shim and Plasma files.

use std::{
    fs,
    io::Write as _,
    os::unix::{fs::PermissionsExt as _, process::CommandExt as _},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const BIN: &str = env!("CARGO_BIN_EXE_host-tools");
const DARK: &str = "[Colors:Window]\nBackgroundNormal=32,35,38\n";
const LIGHT: &str = "[Colors:Window]\nBackgroundNormal=239,240,243\n";

/// A fake session: an isolated runtime dir holding the watcher's state, a
/// config dir holding the Plasma palette, and children killed on drop.
struct Studio {
    root: tempfile::TempDir,
    children: Vec<Child>,
}

impl Studio {
    fn new(name: &str) -> Self {
        let root = tempfile::Builder::new()
            .prefix(&format!("host-tools-theme-{name}-"))
            .tempdir()
            .unwrap();
        fs::create_dir(root.path().join("runtime")).unwrap();
        fs::create_dir(root.path().join("config")).unwrap();
        Self {
            root,
            children: Vec::new(),
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

    /// A dconf stand-in: `read` answers with `read_value`, writes are logged.
    fn dconf(&self, read_value: &str) -> PathBuf {
        let shim = self.root.path().join("dconf");
        let log = self.root.path().join("writes.log");
        // Nix supplies its sandbox shell; native Linux builds use /bin/sh.
        let shell = option_env!("HOST_TOOLS_TEST_SHELL").unwrap_or("/bin/sh");
        let mut script = fs::File::create(&shim).unwrap();
        write!(
            script,
            "#!{shell}\ncase \"$1\" in\nread) echo '{read_value}' ;;\nwrite) echo \"$2 $3\" >> \"{}\" ;;\nesac\n",
            log.display()
        )
        .unwrap();
        drop(script);
        fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
        shim
    }

    fn spawn(&mut self, command: &mut Command) -> &mut Child {
        self.children.push(command.spawn().unwrap());
        self.children.last_mut().unwrap()
    }

    fn command(&self, dconf: &Path) -> Command {
        let mut command = Command::new(BIN);
        command
            .args(["theme-watch", "--dconf"])
            .arg(dconf)
            .env("XDG_RUNTIME_DIR", self.root.path().join("runtime"))
            .env("XDG_CONFIG_HOME", self.root.path().join("config"));
        command
    }

    fn recover(&self, dconf: &Path) -> std::process::Output {
        self.command(dconf).arg("--recover").output().unwrap()
    }

    /// The watcher, watching this studio instead of the real session.
    fn watcher(&mut self, dconf: &Path) -> &mut Child {
        self.spawn(
            self.command(dconf)
                .stdout(Stdio::null())
                .stderr(Stdio::null()),
        )
    }

    /// A process the watcher must read as a new T3 Code main.
    fn t3_main(&mut self) -> &mut Child {
        // Nix's multicall sleep dispatches by argv[0], so use a shell builtin.
        // Keeping the pipe open lets read wait until the child is killed on drop.
        self.spawn(
            Command::new("bash")
                .arg0("t3code")
                .args(["-c", "read -r -t 60"])
                .stdin(Stdio::piped()),
        )
    }

    fn await_writes(&self, count: usize, limit: Duration) -> Vec<String> {
        let start = Instant::now();
        while self.writes().len() < count && start.elapsed() < limit {
            std::thread::sleep(Duration::from_millis(100));
        }
        let writes = self.writes();
        assert!(
            writes.len() >= count,
            "expected {count} writes within {limit:?}, got {writes:?}"
        );
        writes
    }
}

impl Drop for Studio {
    fn drop(&mut self) {
        for child in &mut self.children {
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

/// The whole point: a T3 Code cold start on a dark system repairs the portal's
/// stale value, then flips it so the portal re-emits the scheme.
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
            "/org/gnome/desktop/interface/color-scheme 'default'",
            "/org/gnome/desktop/interface/color-scheme 'prefer-dark'",
        ]
    );
    assert!(
        !studio.marker().exists(),
        "the completed nudge cleared its marker"
    );
}
