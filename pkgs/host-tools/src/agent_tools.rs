use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{
    env,
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
    t3_installer: PathBuf,
    t3_version: String,
    desktop_image: PathBuf,
    desktop_launcher: PathBuf,
    desktop_entry: PathBuf,
    service_environment: PathBuf,
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

fn install(config: &Config, home: &Path, installer: &Path, vars: &[(&str, &str)]) -> Result<()> {
    // Upstream downloads can be large; never stage them in RAM-backed /tmp.
    let temporary = tempfile::Builder::new()
        .prefix(".agent-tools-install-")
        .tempdir_in(home.join("dev"))?;
    let status = Command::new(&config.shell)
        .arg(installer)
        .env("HOME", home)
        .env("TMPDIR", temporary.path())
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
            fs::File::options()
                .write(true)
                .open(destination)
                .context("desktop install must be writable")?;
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

pub fn bootstrap(config: &Path) -> Result<()> {
    let config: Config = serde_json::from_str(&crate::util::read(config)?)?;
    ensure!(
        config.t3_version.contains("-nightly."),
        "T3's bootstrap release must be nightly"
    );
    let home = home()?;
    let state = home.join(".local/state/agent-tools");
    fs::create_dir_all(&state)?;
    let lock = fs::File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(state.join("install.lock"))?;
    eprintln!("agent-tools: waiting for installation lock");
    lock.lock()?;
    fs::create_dir_all(home.join("dev/.cache/agent-tools"))?;
    let bin = home.join(".local/bin");
    fs::create_dir_all(&bin)?;

    let codex = bin.join("codex");
    let codex_home = home.join(".codex");
    let codex_releases = codex_home.join("packages/standalone/releases");
    let codex_version = match native_version(&codex, &codex_releases)? {
        Some(version) => version,
        None => {
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
            )?;
            native_version(&codex, &codex_releases)?
                .context("Codex installer did not select a standalone package")?
        }
    };
    eprintln!("agent-tools: {codex_version}");

    let t3 = bin.join("t3");
    let t3_home = home.join(".t3");
    let t3_releases = t3_home.join("runtime/versions");
    let t3_version = match native_version(&t3, &t3_releases)? {
        Some(version) if version.contains("-nightly.") => version,
        _ => {
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
            )?;
            let version = native_version(&t3, &t3_releases)?
                .context("T3 installer did not select a standalone runtime")?;
            ensure!(
                version.contains("-nightly."),
                "expected nightly T3, got {version}"
            );
            version
        }
    };
    eprintln!("agent-tools: {t3_version}");

    seed_desktop(
        &config.desktop_image,
        &home.join(".local/share/t3code/app/T3-Code-nightly.AppImage"),
    )?;
    for name in ["t3code", "t3code-nightly"] {
        link(&config.desktop_launcher, &bin.join(name))?;
    }
    link(
        &config.desktop_entry,
        &home.join(".local/share/applications/t3code.desktop"),
    )?;
    link(
        &config.service_environment,
        &home.join(".config/systemd/user/t3code.service.d/zz-native-agent-tools.conf"),
    )?;
    Ok(())
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
    let tmp = home.join("dev/.cache/agent-tools");
    fs::create_dir_all(&tmp)?;
    let existing = match fs::read_to_string(file) {
        Ok(existing) => existing,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let contents =
        environment_contents(&existing, path.to_str().context("PATH is not UTF-8")?, &tmp)?;
    crate::util::atomic_write(file, contents.as_bytes())
}

fn environment_contents(existing: &str, path: &str, tmp: &Path) -> Result<String> {
    let mut contents = String::with_capacity(existing.len() + path.len() + 64);
    for line in existing
        .lines()
        .filter(|line| !line.starts_with("PATH=") && !line.starts_with("TMPDIR="))
    {
        writeln!(contents, "{line}")?;
    }
    for (name, value) in [("PATH", path), ("TMPDIR", crate::util::path(tmp)?)] {
        ensure!(
            !value.contains(['\n', '\r']),
            "{name} must not contain a newline"
        );
        let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
        writeln!(contents, "{name}=\"{escaped}\"")?;
    }
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

    fn scratch() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(".agent-tools-test-")
            .tempdir()
            .unwrap()
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
    fn service_environment_keeps_other_exports_and_replaces_stale_paths() {
        let existing = "PATH=\"/old/path\"\nEXAMPLE=\"keep this export\"\nTMPDIR=\"/tmp\"\n";
        let output = environment_contents(
            existing,
            "/home/user/.local/bin:/usr/bin",
            Path::new("/home/user/dev/cache"),
        )
        .unwrap();
        assert_eq!(
            output,
            "EXAMPLE=\"keep this export\"\nPATH=\"/home/user/.local/bin:/usr/bin\"\nTMPDIR=\"/home/user/dev/cache\"\n"
        );
        assert_eq!(
            environment_contents(
                &output,
                "/home/user/.local/bin:/usr/bin",
                Path::new("/home/user/dev/cache")
            )
            .unwrap(),
            output
        );
        assert!(environment_contents("", "bad\nPATH", Path::new("/cache")).is_err());
    }
}
