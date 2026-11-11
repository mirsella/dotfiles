//! Exercise the real chezmoi command lifecycle, without a server or Git remote.

#[path = "../src/test_support.rs"]
mod test_support;

use std::{
    fs,
    io::Write as _,
    path::PathBuf,
    process::{Command, Stdio},
};

struct Fixture {
    root: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let fixture = Self {
            root: tempfile::tempdir().unwrap(),
        };
        for dir in ["source/dot_config/t3code", "home/.config/t3code", "bin"] {
            fs::create_dir_all(fixture.path(dir)).unwrap();
        }
        fs::write(fixture.source_bundle(), "repository\n").unwrap();
        fs::write(fixture.path("source/dot_other"), "other\n").unwrap();
        let ignore = option_env!("HOST_TOOLS_TEST_CHEZMOI_IGNORE")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dotfiles/.chezmoiignore")
            });
        fs::copy(ignore, fixture.path("source/.chezmoiignore")).unwrap();
        fs::write(fixture.path("captured"), "captured\n").unwrap();
        fs::write(fixture.path("config.toml"), "").unwrap();
        fixture.script(
            "t3-settings",
            r#"
printf '%s\n' "$*" >> "$TEST_LOG"
case "$1" in
capture) cp "$TEST_CAPTURED" "$TEST_TARGET" ;;
sync)
    case " $* " in *' --dry-run '*) exit 0;; esac
    if [ "${TEST_FAIL_SYNC:-0}" = 1 ]; then exit 42; fi
    cp "$TEST_TARGET" "$TEST_LIVE" ;;
*) exit 2 ;;
esac
"#,
        );
        fixture.script("hex-settings", "exit 0\n");
        let template = option_env!("HOST_TOOLS_TEST_CHEZMOI_TEMPLATE")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../dotfiles/private_dot_config/chezmoi/chezmoi.toml.tmpl")
            });
        let mut child = fixture
            .command()
            .arg("execute-template")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&fs::read(template).unwrap())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let config = String::from_utf8(output.stdout)
            .unwrap()
            .replace("autoCommit = true", "autoCommit = false")
            .replace("autoPush = true", "autoPush = false");
        let shell = option_env!("HOST_TOOLS_TEST_SHELL").unwrap_or("/bin/sh");
        fs::write(
            fixture.path("config.toml"),
            format!("{config}\n[update]\ncommand = {shell:?}\nargs = [\"-c\", \"true\"]\n"),
        )
        .unwrap();
        fixture
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }
    fn source_bundle(&self) -> PathBuf {
        self.path("source/dot_config/t3code/private_portable-settings.json")
    }
    fn target_bundle(&self) -> PathBuf {
        self.path("home/.config/t3code/portable-settings.json")
    }

    fn command(&self) -> Command {
        let mut command = Command::new(option_env!("HOST_TOOLS_TEST_CHEZMOI").unwrap_or("chezmoi"));
        command
            .args(["--config"])
            .arg(self.path("config.toml"))
            .arg("--source")
            .arg(self.path("source"))
            .arg("--working-tree")
            .arg(self.path("source"))
            .arg("--destination")
            .arg(self.path("home"))
            .arg("--persistent-state")
            .arg(self.path("state.boltdb"))
            .args(["--no-tty", "--force"])
            .env("HOME", self.path("home"))
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.path("bin").display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .env("TEST_LOG", self.path("calls"))
            .env("TEST_CAPTURED", self.path("captured"))
            .env("TEST_TARGET", self.target_bundle())
            .env("TEST_LIVE", self.path("live"));
        command
    }

    fn script(&self, name: &str, body: &str) {
        let path = self.path(&format!("bin/{name}"));
        let shell = option_env!("HOST_TOOLS_TEST_SHELL").unwrap_or("/bin/sh");
        test_support::executable(&path, format!("#!{shell}\nset -e\n{body}"));
    }

    fn run(&self, args: &[&str]) {
        let output = self.command().args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn read(&self, name: &str) -> String {
        fs::read_to_string(self.path(name)).unwrap()
    }
}

#[test]
fn generated_dependencies_with_broken_links_do_not_block_sync() {
    let fixture = Fixture::new();
    let bin = fixture.path("source/dot_config/opencode/node_modules/.bin");
    fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink("missing-prebuild", bin.join("download-msgpackr-prebuilds"))
        .unwrap();
    fixture.run(&["apply"]);
    fixture.run(&["re-add", fixture.target_bundle().to_str().unwrap()]);
    assert_eq!(fixture.read("live"), "captured\n");
    assert!(bin.join("download-msgpackr-prebuilds").is_symlink());
}

#[test]
fn apply_and_update_sync_after_copying_the_bundle() {
    let fixture = Fixture::new();
    fixture.run(&["apply"]);
    assert_eq!(fixture.read("live"), "repository\n");
    fs::write(fixture.source_bundle(), "updated repository\n").unwrap();
    fixture.run(&["update"]);
    assert_eq!(fixture.read("live"), "updated repository\n");
    assert_eq!(
        fixture.read("calls").lines().collect::<Vec<_>>(),
        ["sync --if-running --no-capture"; 2]
    );
}

#[test]
fn re_add_captures_before_saving_to_source() {
    let fixture = Fixture::new();
    fixture.run(&["apply"]);
    fixture.run(&["re-add", fixture.target_bundle().to_str().unwrap()]);
    assert_eq!(
        fs::read_to_string(fixture.source_bundle()).unwrap(),
        "captured\n"
    );
    assert_eq!(
        fixture.read("calls"),
        "sync --if-running --no-capture\ncapture --if-running\nsync --if-running --no-capture\n"
    );
}

#[test]
fn unrelated_re_add_does_not_acknowledge_unsaved_t3_settings() {
    let fixture = Fixture::new();
    fixture.run(&["apply"]);
    fixture.run(&["re-add", fixture.path("home/.other").to_str().unwrap()]);
    assert_eq!(
        fs::read_to_string(fixture.source_bundle()).unwrap(),
        "repository\n"
    );
    assert_eq!(
        fixture.read("calls"),
        "sync --if-running --no-capture\ncapture --if-running\n"
    );
}

#[test]
fn dry_run_re_add_does_not_acknowledge_unsaved_t3_settings() {
    let fixture = Fixture::new();
    fixture.run(&["apply"]);
    fixture.run(&[
        "re-add",
        "--dry-run",
        fixture.target_bundle().to_str().unwrap(),
    ]);
    assert_eq!(
        fs::read_to_string(fixture.source_bundle()).unwrap(),
        "repository\n"
    );
    assert_eq!(
        fixture
            .read("calls")
            .lines()
            .filter(|line| line.starts_with("sync "))
            .count(),
        1
    );
}

#[test]
fn dry_run_matching_re_add_does_not_acknowledge_settings() {
    let fixture = Fixture::new();
    fixture.run(&["apply"]);
    fs::write(fixture.path("captured"), "repository\n").unwrap();
    fixture.run(&[
        "re-add",
        "--dry-run",
        fixture.target_bundle().to_str().unwrap(),
    ]);
    assert_eq!(
        fixture.read("calls"),
        "sync --if-running --no-capture\ncapture --if-running\n"
    );
}

#[test]
fn update_without_apply_does_not_sync_settings() {
    for flag in ["--apply=false", "-a=false"] {
        let fixture = Fixture::new();
        fixture.run(&["apply"]);
        fs::write(fixture.source_bundle(), "unapplied repository\n").unwrap();
        fixture.run(&["update", flag]);
        assert_eq!(fixture.read("live"), "repository\n");
        assert_eq!(fixture.read("calls"), "sync --if-running --no-capture\n");
    }
}

#[test]
fn dry_run_does_not_write_live_settings() {
    let fixture = Fixture::new();
    fixture.run(&["apply"]);
    fs::write(fixture.source_bundle(), "unapplied repository\n").unwrap();
    fixture.run(&["update", "--dry-run"]);
    assert_eq!(fixture.read("live"), "repository\n");
}

#[test]
fn sync_errors_stop_update() {
    let fixture = Fixture::new();
    fixture.run(&["apply"]);
    let output = fixture
        .command()
        .arg("update")
        .env("TEST_FAIL_SYNC", "1")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("exit status 42"));
}
