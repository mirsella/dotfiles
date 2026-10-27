use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{
    env,
    ffi::CString,
    fmt::Write,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Config {
    codex_installer: PathBuf,
    opencode_installer: PathBuf,
    opencode_version: String,
    t3_installer: PathBuf,
    t3_version: String,
    desktop_image: PathBuf,
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
    match fs::canonicalize(program) {
        Ok(path) if path.starts_with(root) => {
            let output = crate::util::command_output(Command::new(path).arg("--version"))?;
            Ok(Some(String::from_utf8(output)?.trim().to_owned()))
        }
        Ok(_) => Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("resolve {}", program.display())),
    }
}

fn ensure_native(
    program: &Path,
    root: &Path,
    expected: &str,
    accepts: impl Fn(&str) -> bool,
    install: impl FnOnce() -> Result<()>,
) -> Result<String> {
    if let Some(version) = native_version(program, root)?.filter(|version| accepts(version)) {
        return Ok(version);
    }
    install()?;
    let version = native_version(program, root)?
        .with_context(|| format!("{expected} installer did not select a standalone binary"))?;
    ensure!(accepts(&version), "expected {expected}, got {version}");
    Ok(version)
}

fn install(
    config: &Config,
    home: &Path,
    installer: &Path,
    vars: &[(&str, &str)],
    args: &[&str],
) -> Result<()> {
    let status = Command::new(&config.shell)
        .arg(installer)
        .args(args)
        .env("HOME", home)
        .env(
            "PATH",
            format!(
                "{}:{}",
                home.join(".local/bin").display(),
                config.installer_path
            ),
        )
        .envs(vars.iter().copied())
        .stdin(Stdio::null())
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

fn seed_desktop(source: &Path, destination: &Path) -> Result<()> {
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
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    std::io::copy(&mut fs::File::open(source)?, temporary.as_file_mut())?;
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
    ensure!(
        config.t3_version.contains("-nightly."),
        "T3's bootstrap release must be nightly"
    );
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

    let opencode_root = home.join(".opencode/bin");
    let opencode = opencode_root.join("opencode");
    let is_v2 = |version: &str| {
        version
            .split_whitespace()
            .last()
            .is_some_and(|version| version.trim_start_matches('v').starts_with("2."))
    };
    let opencode_version = ensure_native(&opencode, &opencode_root, "OpenCode v2", is_v2, || {
        eprintln!("agent-tools: installing official standalone OpenCode v2");
        install(
            &config,
            &home,
            &config.opencode_installer,
            &[],
            &["--version", &config.opencode_version, "--no-modify-path"],
        )
    })?;
    link(&opencode, &bin.join("opencode"))?;
    eprintln!("agent-tools: OpenCode {opencode_version}");

    let codex = bin.join("codex");
    let codex_home = home.join(".codex");
    let codex_releases = codex_home.join("packages/standalone/releases");
    let codex_version = ensure_native(
        &codex,
        &codex_releases,
        "Codex",
        |_| true,
        || {
            eprintln!("agent-tools: installing official standalone Codex (latest stable)");
            install(
                &config,
                &home,
                &config.codex_installer,
                &[
                    ("CODEX_RELEASE", "latest"),
                    ("CODEX_NON_INTERACTIVE", "true"),
                    ("CODEX_HOME", crate::util::path(&codex_home)?),
                    ("CODEX_INSTALL_DIR", crate::util::path(&bin)?),
                ],
                &[],
            )
        },
    )?;
    eprintln!("agent-tools: {codex_version}");

    let t3 = bin.join("t3");
    let t3_home = home.join(".t3");
    let t3_releases = t3_home.join("runtime/versions");
    let t3_version = ensure_native(
        &t3,
        &t3_releases,
        "nightly T3",
        |version| version.contains("-nightly."),
        || {
            eprintln!("agent-tools: installing standalone T3 CLI (nightly)");
            install(
                &config,
                &home,
                &config.t3_installer,
                &[
                    ("T3CODE_CHANNEL", "nightly"),
                    ("T3CODE_VERSION", &config.t3_version),
                    ("T3CODE_HOME", crate::util::path(&t3_home)?),
                    ("T3CODE_INSTALL_BIN_DIR", crate::util::path(&bin)?),
                ],
                &[],
            )
        },
    )?;
    eprintln!("agent-tools: {t3_version}");

    seed_desktop(
        &config.desktop_image,
        &home.join(".local/share/t3code/app/T3-Code-nightly.AppImage"),
    )?;
    let installed = state.join("installed");
    pin_package(&config.nix_store, &package, &installed)?;
    link_launchers(&installed, &home)?;
    eprintln!(
        "agent-tools: protected installed package at {}",
        installed.display()
    );
    ensure_t3_service(&home)?;
    Ok(())
}

fn ensure_t3_service(home: &Path) -> Result<()> {
    if home.join(".config/systemd/user/t3code.service").exists() {
        return Ok(());
    }
    let available = Command::new("systemctl")
        .args(["--user", "show-environment"])
        .output()?;
    if !available.status.success() {
        eprintln!("agent-tools: T3 service setup will finish when the user manager starts");
        return Ok(());
    }
    crate::util::command_output(
        Command::new(home.join(".local/bin/t3"))
            .args(["service", "install"])
            .stdin(Stdio::null()),
    )
    .context("install T3 background service")?;
    eprintln!("agent-tools: installed T3 background service");
    Ok(())
}

/// Point the CLI link at the binary of the server that is currently running.
///
/// T3 updates its server runtime on its own, while the installer-provided link
/// keeps whatever version it selected. A stale link cannot open a scoped
/// session against the newer server.
fn align_t3_cli(home: &Path, base_dir: &Path) -> Result<()> {
    if !base_dir.join("userdata").is_dir() {
        return Ok(());
    }
    let runtime = crate::t3_settings::running_server_runtime(base_dir)?;
    let Some(server_version) = runtime["serverVersion"].as_str() else {
        return Ok(());
    };
    let versions = base_dir.join("runtime/versions");
    let Ok(root) = fs::canonicalize(&versions) else {
        return Ok(());
    };
    let link = home.join(".local/bin/t3");
    let current = match fs::canonicalize(&link) {
        Ok(current) if current.starts_with(&root) => Some(current),
        Ok(_) => return Ok(()),
        Err(_) => {
            // A dangling link is still ours when its stored target names the runtime store.
            let Ok(target) = fs::read_link(&link) else {
                return Ok(());
            };
            let target = if target.is_absolute() {
                target
            } else {
                link.parent().unwrap_or(Path::new("/")).join(target)
            };
            if !target.starts_with(&root) && !target.starts_with(&versions) {
                return Ok(());
            }
            None
        }
    };
    let target = versions.join(server_version).join("t3");
    let Ok(target) = fs::canonicalize(target) else {
        eprintln!("agent-tools: no CLI for the running T3 {server_version}");
        return Ok(());
    };
    ensure!(
        target.starts_with(&root),
        "T3 server version {server_version} resolves outside {}",
        versions.display()
    );
    if current.as_ref() == Some(&target) {
        return Ok(());
    }
    let temporary = link.with_file_name(format!("t3.align.{}", std::process::id()));
    let _ = fs::remove_file(&temporary);
    symlink(&target, &temporary).with_context(|| format!("link {}", temporary.display()))?;
    fs::rename(&temporary, &link).with_context(|| format!("replace {}", link.display()))?;
    eprintln!(
        "agent-tools: pointed {} at T3 {server_version}",
        link.display()
    );
    Ok(())
}

pub fn setup() -> Result<()> {
    let home = home()?;
    ensure_t3_service(&home)?;
    crate::util::run("systemctl", &["--user", "start", "t3code.service"])?;
    align_t3_cli(&home, &crate::t3_settings::resolve_base_dir(&home, None))?;
    let input = home.join(".config/t3code/portable-settings.json");
    if !input.is_file() {
        eprintln!("agent-tools: apply editable dotfiles to restore T3 preferences");
        return Ok(());
    }
    crate::t3_settings::run(
        None,
        crate::t3_settings::Operation::Sync {
            input: Some(input),
            dry_run: false,
            force_apply: false,
            force_capture: false,
            allow_version_mismatch: false,
        },
    )
}

pub fn environment(file: &Path) -> Result<()> {
    let home = home()?;
    let local_bin = home.join(".local/bin");
    let mut paths = vec![local_bin.clone()];
    paths.extend(
        env::split_paths(&env::var_os("PATH").context("PATH is unset")?)
            .filter(|path| path != &local_bin),
    );
    let path = env::join_paths(paths)?;
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
    use std::{os::unix::process::CommandExt, thread, time::Duration};

    fn scratch() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(".agent-tools-test-")
            .tempdir()
            .unwrap()
    }

    #[test]
    fn standalone_setup_preserves_updates_and_validates_installer_output() -> Result<()> {
        let root = scratch();
        let program = root.path().join("agent");
        let install_version = |version| -> Result<()> {
            fs::write(&program, format!("#!/bin/sh\necho {version}\n"))?;
            fs::set_permissions(&program, fs::Permissions::from_mode(0o755))?;
            Ok(())
        };
        let accepts = |version: &str| version.starts_with("2.");
        assert_eq!(
            ensure_native(&program, root.path(), "v2 agent", accepts, || {
                install_version("2.0.29")
            })?,
            "2.0.29"
        );
        assert_eq!(
            ensure_native(
                &program,
                root.path(),
                "v2 agent",
                accepts,
                || anyhow::bail!("updated binary must be kept")
            )?,
            "2.0.29"
        );
        install_version("1.0.0")?;
        assert!(
            ensure_native(&program, root.path(), "v2 agent", accepts, || {
                install_version("1.1.0")
            })
            .unwrap_err()
            .to_string()
            .contains("expected v2 agent")
        );
        fs::remove_file(&program)?;
        assert!(
            ensure_native(&program, root.path(), "v2 agent", accepts, || Ok(()))
                .unwrap_err()
                .to_string()
                .contains("did not select a standalone binary")
        );
        Ok(())
    }

    #[test]
    fn activation_preserves_a_self_updated_appimage() {
        let scratch = scratch();
        let seed = scratch.path().join("seed");
        let installed = scratch.path().join("app/T3-Code-nightly.AppImage");
        fs::write(&seed, "initial nightly").unwrap();
        seed_desktop(&seed, &installed).unwrap();
        assert_eq!(fs::read_to_string(&installed).unwrap(), "initial nightly");
        assert_eq!(
            fs::metadata(&installed).unwrap().permissions().mode() & 0o777,
            0o755
        );
        fs::write(&installed, "newer nightly installed by T3").unwrap();
        seed_desktop(&seed, &installed).unwrap();
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
        assert!(seed_desktop(&seed, &installed).is_err());
        fs::remove_file(&installed).unwrap();
        fs::write(&installed, "read only").unwrap();
        fs::set_permissions(&installed, fs::Permissions::from_mode(0o555)).unwrap();
        assert!(seed_desktop(&seed, &installed).is_err());
    }

    #[test]
    fn activation_preserves_a_running_appimage() {
        let scratch = scratch();
        let installed = scratch.path().join("T3-Code-nightly.AppImage");
        let executable = env::split_paths(&env::var_os("PATH").unwrap())
            .map(|path| path.join("cat"))
            .find(|path| path.is_file())
            .expect("cat must be on the test PATH");
        fs::copy(executable, &installed).unwrap();
        fs::set_permissions(&installed, fs::Permissions::from_mode(0o755)).unwrap();
        // A sandbox can briefly report the freshly copied file as busy while the
        // kernel is still closing the copy's handle. That error is transient, so
        // retry it instead of failing the build on the race.
        let mut child = None;
        let mut last_busy = None;
        for _ in 0..50 {
            match Command::new(&installed)
                .arg0("cat")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(spawned) => {
                    child = Some(spawned);
                    break;
                }
                Err(error) if error.raw_os_error() == Some(libc::ETXTBSY) => {
                    last_busy = Some(error);
                    thread::sleep(Duration::from_millis(100));
                }
                Err(error) => panic!("spawning the installed AppImage failed: {error}"),
            }
        }
        let mut child = child.unwrap_or_else(|| {
            let error = last_busy.expect("only a busy spawn can exhaust the retries");
            panic!("the installed AppImage stayed busy: {error}")
        });
        let write_error = fs::File::options()
            .write(true)
            .open(&installed)
            .err()
            .and_then(|error| error.raw_os_error());
        let result = seed_desktop(&scratch.path().join("unused-seed"), &installed);
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
        fs::write(&binary, "#!/bin/sh\nprintf 'native version\\n'\n").unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        let command = scratch.path().join("codex");
        link(&binary, &command).unwrap();
        assert_eq!(
            native_version(&command, &releases).unwrap().as_deref(),
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
    fn t3_cli_follows_the_running_server_version() -> Result<()> {
        let scratch = scratch();
        let home = scratch.path().join("home");
        let base_dir = home.join(".t3");
        let versions = base_dir.join("runtime/versions");
        for version in ["0.1.0", "0.1.1"] {
            let binary = versions.join(version).join("t3");
            fs::create_dir_all(binary.parent().unwrap())?;
            fs::write(&binary, "")?;
        }
        let link = home.join(".local/bin/t3");
        fs::create_dir_all(link.parent().unwrap())?;
        symlink(versions.join("0.1.0/t3"), &link)?;
        fs::create_dir_all(base_dir.join("userdata"))?;
        fs::write(
            base_dir.join("userdata/server-runtime.json"),
            r#"{"serverVersion":"0.1.1"}"#,
        )?;

        align_t3_cli(&home, &base_dir)?;
        assert_eq!(
            fs::read_link(&link)?,
            fs::canonicalize(versions.join("0.1.1/t3"))?
        );

        // The link dangles when its version directory disappears.
        fs::remove_file(versions.join("0.1.1/t3"))?;
        let newer = versions.join("0.1.2/t3");
        fs::create_dir_all(newer.parent().unwrap())?;
        fs::write(&newer, "")?;
        fs::write(
            base_dir.join("userdata/server-runtime.json"),
            r#"{"serverVersion":"0.1.2"}"#,
        )?;
        align_t3_cli(&home, &base_dir)?;
        assert_eq!(fs::read_link(&link)?, fs::canonicalize(&newer)?);

        fs::write(
            base_dir.join("userdata/server-runtime.json"),
            r#"{"serverVersion":".."}"#,
        )?;
        fs::write(base_dir.join("runtime/t3"), "")?;
        let error = align_t3_cli(&home, &base_dir).unwrap_err().to_string();
        assert!(error.contains("outside"), "{error}");

        fs::remove_file(&link)?;
        symlink("/bin/sh", &link)?;
        align_t3_cli(&home, &base_dir)?;
        assert_eq!(fs::read_link(&link)?, PathBuf::from("/bin/sh"));
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
