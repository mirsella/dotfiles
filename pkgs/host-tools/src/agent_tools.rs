use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    env,
    ffi::{CString, OsStr, OsString},
    fmt::Write,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Config {
    claude_installer: PathBuf,
    claude_version: String,
    curl: PathBuf,
    nix_store: PathBuf,
    shell: PathBuf,
    installer_path: String,
}

fn home() -> Result<PathBuf> {
    let home = PathBuf::from(env::var_os("HOME").context("HOME is unset")?);
    ensure!(home.is_absolute(), "HOME must be absolute");
    ensure!(
        unsafe { libc::geteuid() } != 0,
        "run agent bootstrap as the user, not root"
    );
    Ok(home)
}

pub(crate) fn state_dir(home: &Path) -> PathBuf {
    home.join(".local/state/agent-tools")
}

fn native_version(program: &Path, root: &Path) -> Result<Option<String>> {
    let resolve = |path: &Path| -> Result<Option<PathBuf>> {
        match fs::canonicalize(path) {
            Ok(path) => Ok(Some(path)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).with_context(|| format!("resolve {}", path.display())),
        }
    };
    let Some(path) = resolve(program)? else {
        return Ok(None);
    };
    let Some(root) = resolve(root)? else {
        return Ok(None);
    };
    if !path.starts_with(root) {
        return Ok(None);
    }
    let output = crate::util::command_output(Command::new(path).arg("--version"))?;
    Ok(Some(String::from_utf8(output)?.trim().to_owned()))
}

/// Keep any existing standalone installation, regardless of its version.
fn ensure_native(
    program: &Path,
    root: &Path,
    name: &str,
    install: impl FnOnce() -> Result<()>,
) -> Result<String> {
    if let Some(version) = native_version(program, root)? {
        return Ok(version);
    }
    install()?;
    native_version(program, root)?
        .with_context(|| format!("{name} installer did not select a standalone binary"))
}

fn install(
    config: &Config,
    home: &Path,
    installer: &Path,
    configure: impl FnOnce(&mut Command),
) -> Result<()> {
    let path = env::join_paths(
        std::iter::once(home.join(".local/bin")).chain(env::split_paths(&config.installer_path)),
    )?;
    let mut command = Command::new(&config.shell);
    command
        .arg(installer)
        .env("HOME", home)
        .env("PATH", path)
        .stdin(Stdio::null());
    configure(&mut command);
    let status = command
        .status()
        .with_context(|| format!("start {}", installer.display()))?;
    ensure!(status.success(), "{} exited {status}", installer.display());
    Ok(())
}

fn link(source: &Path, destination: &Path) -> Result<()> {
    match fs::read_link(destination) {
        Ok(existing) if existing == source => return Ok(()),
        Ok(_) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::InvalidInput
            ) => {}
        Err(error) => return Err(error).with_context(|| format!("read {}", destination.display())),
    }
    let parent = destination.parent().context("link has no parent")?;
    fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".agent-tools-link-")
        .tempdir_in(parent)?;
    let temporary = staging.path().join("link");
    symlink(source, &temporary)?;
    fs::rename(&temporary, destination)
        .with_context(|| format!("install link {}", destination.display()))
}

fn occupied(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("stat {}", path.display())),
    }
}

fn has_release(versions: &Path, executables: &[&str]) -> Result<bool> {
    let entries = match fs::read_dir(versions) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error).with_context(|| format!("list {}", versions.display())),
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let release = entry.path();
        for executable in executables {
            if occupied(&release.join(executable))? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[derive(Clone, Copy)]
enum Cli {
    OpenCode,
    Codex,
    T3,
}

const OPENCODE_BINARY: &str = ".opencode/bin/opencode";
const CODEX_STANDALONE: &str = ".codex/packages/standalone";
const T3_LAUNCHER: &str = ".local/bin/t3";
const T3_UNIT: &str = ".config/systemd/user/t3code.service";

fn seed_cli(home: &Path, cli: Cli, install: impl FnOnce(&Path) -> Result<()>) -> Result<bool> {
    let (name, launcher) = match cli {
        Cli::OpenCode => ("OpenCode", ".local/bin/opencode"),
        Cli::Codex => ("Codex", ".local/bin/codex"),
        Cli::T3 => ("T3", T3_LAUNCHER),
    };
    let launcher = home.join(launcher);
    if occupied(&launcher)? {
        eprintln!("agent-tools: leaving existing {name} launcher untouched");
        return Ok(false);
    }
    let installed = match cli {
        Cli::OpenCode => occupied(&home.join(OPENCODE_BINARY))?,
        Cli::Codex => {
            let standalone = home.join(CODEX_STANDALONE);
            occupied(&standalone.join("current"))?
                || has_release(&standalone.join("releases"), &["codex", "bin/codex"])?
        }
        Cli::T3 => {
            occupied(&home.join(T3_UNIT))?
                || occupied(&home.join(".t3/bin/t3"))?
                || occupied(&home.join(".t3/runtime/service-state.json"))?
                || has_release(&home.join(".t3/runtime/versions"), &["t3"])?
        }
    };
    if installed {
        eprintln!(
            "agent-tools: {name} is already installed but {} is missing; use its installer to restore the launcher",
            launcher.display()
        );
        return Ok(false);
    }
    eprintln!("agent-tools: installing {name} from its official installer");
    install(&launcher)?;
    ensure!(
        launcher.try_exists()?,
        "{name} installer did not create {}",
        launcher.display()
    );
    Ok(true)
}

fn fetch(curl: &Path, url: &str) -> Command {
    let mut command = Command::new(curl);
    command
        .args([
            "--disable",
            "--globoff",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "15",
            "--max-time",
            "300",
            "--retry",
            "2",
            "--retry-max-time",
            "300",
        ])
        .arg("--url")
        .arg(url);
    command
}

fn download(curl: &Path, url: &str, destination: &Path) -> Result<()> {
    crate::util::command_output(fetch(curl, url).arg("--output").arg(destination))
        .with_context(|| format!("download {url}"))?;
    Ok(())
}

fn install_online(
    config: &Config,
    home: &Path,
    url: &str,
    configure: impl FnOnce(&mut Command),
) -> Result<()> {
    let installer = tempfile::NamedTempFile::new()?;
    download(&config.curl, url, installer.path())?;
    install(config, home, installer.path(), configure)
        .with_context(|| format!("run installer from {url}"))
}

#[derive(Deserialize)]
struct T3Release {
    tag_name: String,
    assets: Vec<T3Asset>,
}

#[derive(Deserialize)]
struct T3Asset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

fn latest_desktop(releases: &[T3Release]) -> Result<&T3Asset> {
    releases
        .iter()
        .find(|release| release.tag_name.contains("-nightly."))
        .context("T3 did not publish a nightly in its recent releases")?
        .assets
        .iter()
        .find(|asset| asset.name.ends_with("-x86_64.AppImage"))
        .context("the latest T3 nightly has no x86_64 AppImage")
}

fn download_desktop(curl: &Path, destination: &Path) -> Result<()> {
    let releases: Vec<T3Release> =
        serde_json::from_slice(&crate::util::command_output(&mut fetch(
            curl,
            "https://api.github.com/repos/pingdotgg/t3code/releases?per_page=20",
        ))?)?;
    let asset = latest_desktop(&releases)?;
    let expected = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .context("the latest T3 AppImage has no SHA-256 digest")?;
    ensure!(
        asset
            .browser_download_url
            .starts_with("https://github.com/pingdotgg/t3code/releases/download/"),
        "unexpected T3 AppImage download URL"
    );
    download(curl, &asset.browser_download_url, destination)?;
    let mut hash = Sha256::new();
    std::io::copy(&mut fs::File::open(destination)?, &mut hash)?;
    ensure!(
        format!("{:x}", hash.finalize()) == expected,
        "T3 AppImage SHA-256 mismatch"
    );
    Ok(())
}

fn seed_desktop(destination: &Path, download: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    match fs::symlink_metadata(destination) {
        Ok(metadata) => {
            ensure!(
                metadata.is_file(),
                "desktop install must be a writable regular file"
            );
            ensure!(
                metadata.permissions().mode() & 0o100 != 0,
                "desktop install must be executable"
            );
            let path = CString::new(destination.as_os_str().as_encoded_bytes())?;
            // Opening a running AppImage for writing fails with ETXTBSY even
            // when its permissions allow the updater to replace it.
            ensure!(
                unsafe {
                    libc::faccessat(libc::AT_FDCWD, path.as_ptr(), libc::W_OK, libc::AT_EACCESS)
                } == 0,
                "desktop install must be writable: {}",
                std::io::Error::last_os_error()
            );
            eprintln!("agent-tools: keeping existing nightly desktop AppImage");
            return Ok(());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let parent = destination.parent().context("desktop path has no parent")?;
    fs::create_dir_all(parent)?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    download(temporary.path())?;
    temporary
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o755))?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(destination)?;
    eprintln!("agent-tools: installed writable nightly desktop AppImage");
    Ok(())
}

fn pin_package(nix_store: &Path, package: &Path, installed: &Path) -> Result<()> {
    crate::util::command_output(
        Command::new(nix_store)
            .arg("--realise")
            .arg(package)
            .arg("--add-root")
            .arg(installed)
            .arg("--indirect"),
    )?;
    ensure!(
        fs::canonicalize(installed)? == fs::canonicalize(package)?,
        "Nix did not register the installed agent-tools package"
    );
    Ok(())
}

fn link_launchers(installed: &Path, home: &Path) -> Result<()> {
    // Links go through the registered root, so package updates also update any
    // executable path cached by systemd without leaving an old store reference.
    for name in ["t3code", "t3code-nightly"] {
        link(
            &installed.join("bin/t3code-nightly"),
            &home.join(".local/bin").join(name),
        )?;
    }
    for (source, destination) in [
        ("t3code.desktop", ".local/share/applications/t3code.desktop"),
        (
            "zz-native-agent-tools.conf",
            ".config/systemd/user/t3code.service.d/zz-native-agent-tools.conf",
        ),
    ] {
        link(
            &installed.join("share/agent-tools").join(source),
            &home.join(destination),
        )?;
    }
    Ok(())
}

pub fn bootstrap(package: &Path) -> Result<()> {
    let package = fs::canonicalize(package).context("resolve agent-tools package")?;
    let config: Config = serde_json::from_str(&crate::util::read(
        package.join("share/agent-tools/config.json"),
    )?)?;
    let home = home()?;
    let state = state_dir(&home);
    fs::create_dir_all(&state)?;
    let lock = fs::File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(state.join("install.lock"))?;
    eprintln!("agent-tools: waiting for installation lock");
    lock.lock()?;
    let bin = home.join(".local/bin");
    fs::create_dir_all(&bin)?;

    seed_cli(&home, Cli::OpenCode, |launcher| {
        install_online(
            &config,
            &home,
            "https://opencode.ai/v2/install",
            |command| {
                command.env_remove("VERSION").arg("--no-modify-path");
            },
        )?;
        let opencode = home.join(OPENCODE_BINARY);
        ensure!(
            opencode.is_file(),
            "OpenCode installer did not create {}",
            opencode.display()
        );
        symlink(&opencode, launcher).with_context(|| format!("create {}", launcher.display()))
    })?;

    seed_cli(&home, Cli::Codex, |_| {
        install_online(
            &config,
            &home,
            "https://chatgpt.com/codex/install.sh",
            |command| {
                command
                    .env("CODEX_RELEASE", "latest")
                    .env("CODEX_NON_INTERACTIVE", "true")
                    .env("CODEX_INSTALL_DAEMON_ONLY", "0")
                    .env("CODEX_HOME", home.join(".codex"))
                    .env("CODEX_INSTALL_DIR", &bin);
            },
        )
    })?;

    let claude = bin.join("claude");
    let claude_root = home.join(".local/share/claude/versions");
    let claude_version = ensure_native(&claude, &claude_root, "Claude Code", || {
        eprintln!("agent-tools: installing official standalone Claude Code");
        install(&config, &home, &config.claude_installer, |command| {
            command.arg(&config.claude_version);
        })
    })?;
    eprintln!("agent-tools: Claude Code {claude_version}");

    let new_t3 = seed_cli(&home, Cli::T3, |_| {
        install_online(&config, &home, "https://t3.codes/install.sh", |command| {
            command
                .env("T3CODE_CHANNEL", "nightly")
                .env_remove("T3CODE_VERSION")
                .env_remove("T3CODE_RELEASE_BASE_URL")
                .env("T3CODE_HOME", home.join(".t3"))
                .env("T3CODE_INSTALL_BIN_DIR", &bin);
        })
    })?;

    seed_desktop(
        &home.join(".local/share/t3code/app/T3-Code-nightly.AppImage"),
        |temporary| download_desktop(&config.curl, temporary),
    )?;
    let installed = state.join("installed");
    pin_package(&config.nix_store, &package, &installed)?;
    link_launchers(&installed, &home)?;
    eprintln!(
        "agent-tools: protected installed package at {}",
        installed.display()
    );
    if new_t3 {
        install_t3_service(&home)?;
    }
    Ok(())
}

fn install_t3_service(home: &Path) -> Result<()> {
    if occupied(&home.join(T3_UNIT))? {
        return Ok(());
    }
    if let Err(error) =
        crate::util::command_output(Command::new("systemctl").args(["--user", "show-environment"]))
    {
        eprintln!(
            "agent-tools: user systemd unavailable ({error}); run `t3 service install` once it starts"
        );
        return Ok(());
    }
    crate::util::command_output(
        Command::new(home.join(T3_LAUNCHER))
            .args(["service", "install"])
            .stdin(Stdio::null()),
    )
    .context("install T3 background service")?;
    eprintln!("agent-tools: installed T3 background service");
    Ok(())
}

fn service_path(home: &Path, inherited: &OsStr, prefix: Option<&OsStr>) -> Result<OsString> {
    let mut paths = vec![home.join(".local/bin")];
    // On Arch, keep native tools ahead of the Home Manager profile. The user
    // manager can start without that profile even when Home Manager is active.
    let arch_profile = prefix.is_none().then(|| home.join(".nix-profile/bin"));
    for path in prefix
        .into_iter()
        .chain([inherited])
        .flat_map(env::split_paths)
        .chain(arch_profile)
    {
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    Ok(env::join_paths(paths)?)
}

pub fn environment(file: &Path, path_prefix: Option<&OsStr>) -> Result<()> {
    let inherited = env::var_os("PATH").context("PATH is unset")?;
    let path = service_path(&home()?, &inherited, path_prefix)?;
    let existing = match fs::read_to_string(file) {
        Ok(existing) => existing,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let contents = environment_contents(&existing, path.to_str().context("PATH is not UTF-8")?)?;
    if contents == existing {
        return Ok(());
    }
    crate::util::atomic_write(file, contents.as_bytes())
}

fn environment_contents(existing: &str, path: &str) -> Result<String> {
    let mut contents = String::with_capacity(existing.len() + path.len() + 64);
    // Drop TMPDIR left by older versions so T3 uses its normal temporary directory.
    for line in existing
        .lines()
        .filter(|line| !line.starts_with("PATH=") && !line.starts_with("TMPDIR="))
    {
        writeln!(contents, "{line}")?;
    }
    ensure!(
        !path.contains(['\n', '\r']),
        "PATH must not contain a newline"
    );
    let escaped = path.replace('\\', "\\\\").replace('"', "\\\"");
    writeln!(contents, "PATH=\"{escaped}\"")?;
    Ok(contents)
}

pub fn composio_mcp_headers() -> Result<()> {
    let path = PathBuf::from(env::var_os("HOME").context("HOME is not set")?)
        .join(".composio/user_data.json");
    let login: serde_json::Value = serde_json::from_slice(
        &fs::read(&path)
            .with_context(|| format!("Read Composio CLI login at {}", path.display()))?,
    )?;
    let key = login["api_key"]
        .as_str()
        .filter(|key| key.starts_with("uak_") && key.len() > 4)
        .context("Invalid Composio CLI login: expected a user API key (uak_)")?;
    serde_json::to_writer(
        std::io::stdout().lock(),
        &serde_json::json!({"x-user-api-key": key}),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::CommandExt;

    fn scratch() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(".agent-tools-test-")
            .tempdir()
            .unwrap()
    }

    #[test]
    fn activation_preserves_any_existing_standalone_version() -> Result<()> {
        let root = scratch();
        let program = root.path().join("agent");
        let install_version = |version| -> Result<()> {
            crate::test_support::executable(&program, format!("#!/bin/sh\necho {version}\n"));
            Ok(())
        };
        fn keep() -> Result<()> {
            anyhow::bail!("an existing binary must be kept")
        }
        assert_eq!(
            ensure_native(&program, root.path(), "agent", || {
                install_version("2.0.29")
            })?,
            "2.0.29"
        );
        // Self-updated tools may change major versions or channels. Activation
        // must preserve those binaries instead of restoring the Nix seed.
        for version in ["1.0.0", "2.0.29", "3.0.0-nightly.1"] {
            install_version(version)?;
            assert_eq!(
                ensure_native(&program, root.path(), "agent", keep)?,
                version
            );
        }
        fs::remove_file(&program)?;
        let error = ensure_native(&program, root.path(), "agent", || Ok(())).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("did not select a standalone binary")
        );
        Ok(())
    }

    #[test]
    fn bootstrap_only_installs_when_no_launcher_or_runtime_remains() -> Result<()> {
        for (cli, launcher, existing_paths) in [
            (
                Cli::OpenCode,
                ".local/bin/opencode",
                &[".opencode/bin/opencode"][..],
            ),
            (
                Cli::Codex,
                ".local/bin/codex",
                &[
                    ".codex/packages/standalone/current",
                    ".codex/packages/standalone/releases/installed/codex",
                    ".codex/packages/standalone/releases/installed/bin/codex",
                ][..],
            ),
            (
                Cli::T3,
                ".local/bin/t3",
                &[
                    ".t3/runtime/versions/self-update/t3",
                    ".t3/runtime/service-state.json",
                    ".t3/bin/t3",
                    ".config/systemd/user/t3code.service",
                ][..],
            ),
        ] {
            let scratch = scratch();
            let home = scratch.path();
            let launcher = home.join(launcher);
            fs::create_dir_all(launcher.parent().unwrap())?;
            let no_install = |_: &Path| anyhow::bail!("must not reinstall an existing CLI");
            assert!(seed_cli(home, cli, |destination| {
                assert_eq!(destination, launcher);
                fs::write(destination, "launcher")?;
                Ok(())
            })?);
            assert!(!seed_cli(home, cli, no_install)?);
            assert_eq!(fs::read_to_string(&launcher)?, "launcher");
            fs::remove_file(&launcher)?;
            let missing = home.join("missing");
            symlink(&missing, &launcher)?;
            assert!(!seed_cli(home, cli, no_install)?);
            assert_eq!(fs::read_link(&launcher)?, missing);
            fs::remove_file(&launcher)?;
            for existing_path in existing_paths {
                let path = home.join(existing_path);
                fs::create_dir_all(path.parent().unwrap())?;
                fs::write(&path, "partial install")?;
                assert!(!seed_cli(home, cli, no_install)?);
                assert!(!occupied(&launcher)?);
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }

    #[test]
    fn latest_desktop_uses_the_newest_nightly_for_this_platform() -> Result<()> {
        let releases = serde_json::from_value::<Vec<T3Release>>(serde_json::json!([
            {"tag_name": "v3.0.0", "assets": []},
            {"tag_name": "v3.1.0-preview.1", "assets": []},
            {"tag_name": "v3.0.1-nightly.20261120.3", "assets": [
                {"name": "T3-Code-Nightly-arm64.AppImage", "browser_download_url": "arm"},
                {"name": "T3-Code-Nightly-x86_64.AppImage", "browser_download_url": "new"}
            ]},
            {"tag_name": "v3.0.1-nightly.20261119.2", "assets": [
                {"name": "T3-Code-Nightly-x86_64.AppImage", "browser_download_url": "old"}
            ]}
        ]))?;
        assert_eq!(latest_desktop(&releases)?.browser_download_url, "new");
        assert!(latest_desktop(&releases[..2]).is_err());
        Ok(())
    }

    #[test]
    fn activation_preserves_a_self_updated_appimage() {
        let scratch = scratch();
        let installed = scratch.path().join("app/T3-Code-nightly.AppImage");
        seed_desktop(&installed, |file| {
            fs::write(file, "initial nightly")?;
            Ok(())
        })
        .unwrap();
        assert_eq!(fs::read_to_string(&installed).unwrap(), "initial nightly");
        assert_eq!(
            fs::metadata(&installed).unwrap().permissions().mode() & 0o777,
            0o755
        );
        fs::write(&installed, "newer nightly installed by T3").unwrap();
        seed_desktop(&installed, |_| anyhow::bail!("must not download again")).unwrap();
        assert_eq!(
            fs::read_to_string(&installed).unwrap(),
            "newer nightly installed by T3"
        );
    }

    #[test]
    fn immutable_desktop_installs_are_rejected() {
        let scratch = scratch();
        let seed = scratch.path().join("seed");
        let installed = scratch.path().join("desktop");
        fs::write(&seed, "seed").unwrap();
        symlink(&seed, &installed).unwrap();
        let no_download =
            |_: &Path| -> Result<()> { panic!("must not download over an existing AppImage") };
        assert!(seed_desktop(&installed, no_download).is_err());
        fs::remove_file(&installed).unwrap();
        fs::write(&installed, "read only").unwrap();
        fs::set_permissions(&installed, fs::Permissions::from_mode(0o555)).unwrap();
        assert!(seed_desktop(&installed, no_download).is_err());
    }

    #[test]
    fn activation_preserves_a_running_appimage() {
        let scratch = scratch();
        let installed = scratch.path().join("T3-Code-nightly.AppImage");
        let executable = env::split_paths(&env::var_os("PATH").unwrap())
            .map(|path| path.join("cat"))
            .find(|path| path.is_file())
            .expect("cat must be on the test PATH");
        crate::test_support::executable(&installed, fs::read(executable).unwrap());
        let mut child = Command::new(&installed)
            .arg0("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let write_error = fs::File::options()
            .write(true)
            .open(&installed)
            .err()
            .and_then(|error| error.raw_os_error());
        let result = seed_desktop(&installed, |_| anyhow::bail!("must not download again"));
        child.kill().unwrap();
        child.wait().unwrap();
        assert_eq!(write_error, Some(libc::ETXTBSY));
        result.unwrap();
    }

    #[test]
    fn installed_links_follow_package_updates_without_relinking() {
        let scratch = scratch();
        let home = scratch.path().join("home");
        let installed = scratch.path().join("installed");
        let first = scratch.path().join("first-package");
        let second = scratch.path().join("second-package");
        for (package, version) in [(&first, "first"), (&second, "second")] {
            fs::create_dir_all(package.join("bin")).unwrap();
            fs::create_dir_all(package.join("share/agent-tools")).unwrap();
            for file in [
                "bin/t3code-nightly",
                "bin/host-tools",
                "share/agent-tools/t3code.desktop",
                "share/agent-tools/zz-native-agent-tools.conf",
            ] {
                fs::write(package.join(file), version).unwrap();
            }
        }
        link(&first, &installed).unwrap();
        link_launchers(&installed, &home).unwrap();
        let destinations = [
            home.join(".local/bin/t3code"),
            home.join(".local/bin/t3code-nightly"),
            home.join(".local/share/applications/t3code.desktop"),
            home.join(".config/systemd/user/t3code.service.d/zz-native-agent-tools.conf"),
            installed.join("bin/host-tools"),
        ];
        for path in &destinations {
            assert_eq!(fs::read_to_string(path).unwrap(), "first");
        }
        link(&second, &installed).unwrap();
        fs::remove_dir_all(&first).unwrap();
        for path in &destinations {
            assert_eq!(fs::read_to_string(path).unwrap(), "second");
        }
    }

    #[test]
    fn failed_gc_registration_preserves_the_installed_package() {
        let scratch = scratch();
        let installed = scratch.path().join("installed");
        let first = scratch.path().join("first-package");
        let second = scratch.path().join("second-package");
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        link(&first, &installed).unwrap();
        assert!(pin_package(Path::new("false"), &second, &installed).is_err());
        assert_eq!(fs::read_link(&installed).unwrap(), first);
    }

    #[test]
    fn native_versions_are_only_probed_for_standalone_installs() {
        let scratch = scratch();
        let releases = scratch.path().join("releases");
        fs::create_dir(&releases).unwrap();
        let binary = releases.join("newer-version");
        crate::test_support::executable(&binary, "#!/bin/sh\nprintf 'native version\\n'\n");
        let aliased_releases = scratch.path().join("aliased-releases");
        symlink(&releases, &aliased_releases).unwrap();
        let command = scratch.path().join("codex");
        link(&binary, &command).unwrap();
        assert_eq!(
            native_version(&command, &aliased_releases)
                .unwrap()
                .as_deref(),
            Some("native version")
        );
        let store = scratch.path().join("nix-store-binary");
        fs::write(&store, "immutable binary").unwrap();
        link(&store, &command).unwrap();
        assert_eq!(native_version(&command, &releases).unwrap(), None);
        fs::remove_file(&store).unwrap();
        assert_eq!(native_version(&command, &releases).unwrap(), None);
    }

    #[test]
    fn service_path_prefers_writable_agents_then_platform_profiles() -> Result<()> {
        let home = Path::new("/home/user");
        let inherited = OsStr::new("/usr/bin:/home/user/.local/bin:/home/user/custom/bin");
        let nixos = OsStr::new("/etc/profiles/per-user/user/bin:/run/current-system/sw/bin");
        let arch = service_path(home, inherited, None)?;
        assert_eq!(
            arch,
            "/home/user/.local/bin:/usr/bin:/home/user/custom/bin:/home/user/.nix-profile/bin"
        );
        assert_eq!(service_path(home, &arch, None)?, arch);
        let path = service_path(home, inherited, Some(nixos))?;
        assert_eq!(
            path,
            "/home/user/.local/bin:/etc/profiles/per-user/user/bin:/run/current-system/sw/bin:/usr/bin:/home/user/custom/bin"
        );
        assert_eq!(service_path(home, &path, Some(nixos))?, path);
        Ok(())
    }

    #[test]
    fn service_environment_keeps_other_exports_and_replaces_stale_paths() {
        let existing = "PATH=\"/old/path\"\nEXAMPLE=\"keep this export\"\nTMPDIR=\"/home/user/dev/.cache/agent-tools\"\n";
        let output = environment_contents(existing, "/home/user/.local/bin:/usr/bin").unwrap();
        assert_eq!(
            output,
            "EXAMPLE=\"keep this export\"\nPATH=\"/home/user/.local/bin:/usr/bin\"\n"
        );
        assert_eq!(
            environment_contents(&output, "/home/user/.local/bin:/usr/bin").unwrap(),
            output
        );
        assert!(environment_contents("", "bad\nPATH").is_err());
    }
}
