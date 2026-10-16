//! Exercise continuous Plasma-to-dconf repair without synthetic theme changes.

use std::{
    fs,
    os::unix::fs::PermissionsExt as _,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const BIN: &str = env!("CARGO_BIN_EXE_host-tools");
const KEY: &str = "/org/gnome/desktop/interface/color-scheme";
const DARK: &str = "[Colors:Window]\nBackgroundNormal=32,35,38\n";
const LIGHT: &str = "[Colors:Window]\nBackgroundNormal=239,240,243\n";

struct Studio {
    root: tempfile::TempDir,
    watcher: Option<Child>,
}

impl Studio {
    fn new() -> Self {
        let root = tempfile::Builder::new()
            .prefix("host-tools-theme-")
            .tempdir()
            .unwrap();
        fs::create_dir(root.path().join("config")).unwrap();
        Self {
            root,
            watcher: None,
        }
    }

    fn kdeglobals(&self, contents: &str) {
        fs::write(self.root.path().join("config/kdeglobals"), contents).unwrap();
    }

    fn log(&self, name: &str) -> String {
        match fs::read_to_string(self.root.path().join(name)) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => panic!("read {name}: {error}"),
        }
    }

    fn writes(&self) -> Vec<String> {
        self.log("writes.log").lines().map(str::to_owned).collect()
    }

    fn stored_scheme(&self, value: &str) {
        fs::write(self.root.path().join("scheme"), value).unwrap();
    }

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
read)
echo read >> '{root}/reads.log'
cat '{root}/scheme' ;;
write)
printf '%s' "$3" > '{root}/attempt'
if [ -f '{root}/fail-write' ]; then exit 1; fi
printf '%s' "$3" > '{root}/scheme'
echo "$2 $3" >> '{root}/writes.log' ;;
*) exit 2 ;;
esac
"#,
                root = self.root.path().display(),
            ),
        )
        .unwrap();
        fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
        shim
    }

    fn watcher(&mut self, read_value: &str) {
        assert!(self.watcher.is_none(), "watcher already running");
        self.watcher = Some(
            Command::new(BIN)
                .args(["theme-watch", "--dconf"])
                .arg(self.dconf(read_value))
                .env("XDG_CONFIG_HOME", self.root.path().join("config"))
                .stdout(Stdio::null())
                .stderr(fs::File::create(self.root.path().join("errors.log")).unwrap())
                .spawn()
                .unwrap(),
        );
    }

    fn wait_for(&self, description: &str, ready: impl Fn() -> bool) {
        let start = Instant::now();
        while !ready() {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "timed out waiting for {description}; writes: {:?}; errors: {}",
                self.writes(),
                self.log("errors.log"),
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

#[test]
fn repairs_stale_schemes_without_replaying_them() {
    for (palette, stale, desired) in [
        (DARK, "'prefer-light'", "'prefer-dark'"),
        (LIGHT, "'prefer-dark'", "'prefer-light'"),
    ] {
        let mut studio = Studio::new();
        studio.kdeglobals(palette);
        studio.watcher(stale);
        studio.wait_for("repeated reconciliation", || {
            studio.log("reads.log").lines().count() >= 3
        });
        assert_eq!(studio.writes(), [format!("{KEY} {desired}")]);
        assert_eq!(studio.log("scheme"), desired);
    }
}

#[test]
fn leaves_matching_schemes_unchanged() {
    for (palette, desired) in [(DARK, "'prefer-dark'"), (LIGHT, "'prefer-light'")] {
        let mut studio = Studio::new();
        studio.kdeglobals(palette);
        studio.watcher(desired);
        studio.wait_for("repeated reconciliation", || {
            studio.log("reads.log").lines().count() >= 3
        });
        assert!(studio.writes().is_empty());
        assert_eq!(studio.log("scheme"), desired);
    }
}

#[test]
fn tracks_palette_changes_and_external_drift() {
    let mut studio = Studio::new();
    studio.kdeglobals(DARK);
    studio.watcher("'prefer-light'");
    studio.wait_for("initial repair", || !studio.writes().is_empty());

    studio.kdeglobals(LIGHT);
    studio.wait_for("light scheme", || studio.writes().len() >= 2);
    studio.kdeglobals(DARK);
    studio.wait_for("dark scheme", || studio.writes().len() >= 3);
    studio.stored_scheme("'prefer-light'");
    studio.wait_for("drift repair", || studio.writes().len() >= 4);

    assert_eq!(
        studio.writes(),
        [
            "'prefer-dark'",
            "'prefer-light'",
            "'prefer-dark'",
            "'prefer-dark'"
        ]
        .map(|scheme| format!("{KEY} {scheme}")),
    );
    assert_eq!(studio.log("scheme"), "'prefer-dark'");
}

#[test]
fn retries_failed_writes() {
    let mut studio = Studio::new();
    studio.kdeglobals(DARK);
    let fail = studio.root.path().join("fail-write");
    fs::write(&fail, "").unwrap();
    studio.watcher("'prefer-light'");
    studio.wait_for("failed write", || {
        studio.log("errors.log").contains("retrying")
    });
    assert!(studio.writes().is_empty());
    assert_eq!(studio.log("attempt"), "'prefer-dark'");
    assert_eq!(studio.log("scheme"), "'prefer-light'");

    fs::remove_file(fail).unwrap();
    studio.wait_for("successful retry", || !studio.writes().is_empty());
    assert_eq!(studio.writes(), [format!("{KEY} 'prefer-dark'")]);
}

#[test]
fn waits_for_an_available_palette_without_guessing() {
    for (palette, error) in [
        (None, "read "),
        (
            Some("[Colors:Window]\nBackgroundNormal=invalid\n"),
            "no usable background",
        ),
    ] {
        let mut studio = Studio::new();
        if let Some(palette) = palette {
            studio.kdeglobals(palette);
        }
        studio.watcher("'prefer-light'");
        studio.wait_for("palette error", || {
            let errors = studio.log("errors.log");
            errors.contains(error) && errors.contains("kdeglobals") && errors.contains("retrying")
        });
        assert!(studio.writes().is_empty());
        assert_eq!(studio.log("scheme"), "'prefer-light'");

        studio.kdeglobals(DARK);
        studio.wait_for("palette recovery", || !studio.writes().is_empty());
        assert_eq!(studio.writes(), [format!("{KEY} 'prefer-dark'")]);
    }
}

#[test]
fn rejects_a_missing_config_environment() {
    let output = Command::new(BIN)
        .arg("theme-watch")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("HOME")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("XDG_CONFIG_HOME and HOME are unset"));
}
