//! Owner-operated Hermes secret provisioning and service/backup operations.
mod restore;

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    env, fs,
    io::{Read, Write},
    os::unix::{
        fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

const TANK_BACKUP_DIRECTORY: &str = "/srv/backup/hermes";
const LEGACY_BACKUP_DIRECTORY: &str = "/var/lib/hermes-backups";
const BACKUP_RETENTION: usize = 14;
const BACKUP_TIMESTAMP_FORMAT: &str = "%Y%m%dT%H%M%S%.9fZ";

#[derive(clap::Subcommand)]
pub enum Operation {
    Status,
    Logs,
    Start,
    Stop,
    Restart,
    Backup,
    Restore {
        #[arg(long)]
        archive: PathBuf,
    },
}

fn fill_defaults(current: &mut Value, defaults: &Value) -> bool {
    let (Some(current), Some(defaults)) = (current.as_object_mut(), defaults.as_object()) else {
        return false;
    };
    let mut changed = false;
    for (key, value) in defaults {
        if let Some(existing) = current.get_mut(key) {
            changed |= fill_defaults(existing, value);
        } else {
            current.insert(key.clone(), value.clone());
            changed = true;
        }
    }
    changed
}

pub fn configure(config: &Path, defaults: &Path) -> Result<()> {
    // Nix owns directory creation/ownership. Run as the service user and seed
    // only missing settings, without walking or rewriting unrelated state.
    let (mut current, missing): (Value, _) = match fs::read(config) {
        Ok(bytes) => (serde_yaml::from_slice(&bytes)?, false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (json!({}), true),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        current.is_object(),
        "Hermes configuration must be an object"
    );
    let defaults: Value = serde_json::from_slice(&fs::read(defaults)?)?;
    ensure!(defaults.is_object(), "Hermes defaults must be an object");
    if fill_defaults(&mut current, &defaults) || missing {
        crate::util::atomic_write(config, serde_yaml::to_string(&current)?.as_bytes())?;
    }
    Ok(())
}

fn worker_units(text: &str) -> Result<Vec<String>> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let unit = line
                .split_whitespace()
                .next()
                .context("Missing worker scope name")?;
            ensure!(
                unit.starts_with("hermes-worker-")
                    && unit.ends_with(".scope")
                    && unit
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte)),
                "Unexpected Hermes worker scope: {unit}"
            );
            Ok(unit.to_owned())
        })
        .collect()
}

fn stop_workers(user: &str, uid: u32) -> Result<()> {
    let user_systemctl = || {
        let mut command = Command::new("runuser");
        command
            .args(["-u", user, "--", "systemctl", "--user"])
            .env("XDG_RUNTIME_DIR", format!("/run/user/{uid}"))
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                format!("unix:path=/run/user/{uid}/bus"),
            );
        command
    };
    let output = user_systemctl()
        .args([
            "list-units",
            "--no-legend",
            "--plain",
            "--no-pager",
            "--type=scope",
            "--state=active",
            "hermes-worker-*.scope",
        ])
        .stderr(Stdio::inherit())
        .output()?;
    ensure!(
        output.status.success(),
        "Cannot discover Hermes worker scopes: {}",
        output.status
    );
    let units = worker_units(&String::from_utf8(output.stdout)?)?;
    if !units.is_empty() {
        ensure!(
            user_systemctl()
                .arg("stop")
                .args(&units)
                .status()?
                .success(),
            "Hermes worker scopes could not stop"
        );
    }
    Ok(())
}

fn capture(command: &mut Command, stdin: Option<&[u8]>) -> Result<Vec<u8>> {
    let name = command.get_program().to_string_lossy().into_owned();
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    let (output, written) = if let Some(bytes) = stdin {
        let mut child = command
            .spawn()
            .with_context(|| format!("Cannot start {name}"))?;
        let mut input = child.stdin.take().context("Command stdin was not piped")?;
        // Drain stdout while feeding input to avoid a full-pipe deadlock.
        std::thread::scope(|scope| -> Result<_> {
            let writer = scope.spawn(move || input.write_all(bytes));
            let output = child.wait_with_output();
            let written = writer
                .join()
                .map_err(|_| anyhow::anyhow!("Credential input writer panicked"))?;
            Ok((output, written))
        })?
    } else {
        (command.output(), Ok(()))
    };
    let output = output.with_context(|| format!("Cannot run {name}"))?;
    // Credential-bearing stderr is discarded, not buffered or reported.
    ensure!(
        output.status.success(),
        "{name} exited {}; diagnostic withheld because it may contain credentials",
        output.status
    );
    written.with_context(|| format!("Cannot write private input to {name}"))?;
    Ok(output.stdout)
}

fn decrypt(path: &Path, converter: &Path) -> Result<Value> {
    let key = env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is absent")?
        .join(".ssh/id_ed25519");
    // SOPS executes this using a shell. Reject shell characters in the two paths.
    for path in [converter, key.as_path()] {
        ensure!(
            path.to_string_lossy()
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c)),
            "SOPS identity path contains shell characters"
        );
    }
    let bytes = capture(
        Command::new("sops")
            .args(["decrypt", "--output-type", "json"])
            .arg(path)
            .env(
                "SOPS_AGE_KEY_CMD",
                format!("{} -private-key -i {}", converter.display(), key.display()),
            ),
        None,
    )?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn valid_environment_key(key: &str) -> bool {
    let mut bytes = key.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn parse_environment(text: &str) -> Result<BTreeMap<String, String>> {
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .map(|line| {
            let (key, value) = line
                .split_once('=')
                .context("Malformed saved environment")?;
            let key = key.trim().strip_prefix("export ").unwrap_or(key.trim());
            ensure!(valid_environment_key(key), "Malformed environment key");
            Ok((
                key.to_string(),
                value.trim().trim_matches(['\'', '"']).to_string(),
            ))
        })
        .collect()
}

fn environment<K: AsRef<str>, V: AsRef<str>>(
    values: impl IntoIterator<Item = (K, V)>,
) -> Result<String> {
    let mut result = String::new();
    for (key, value) in values {
        let key = key.as_ref();
        let value = value.as_ref();
        ensure!(valid_environment_key(key), "Malformed environment key");
        ensure!(!value.contains(['\n', '\r']), "Multiline value for {key}");
        result.push_str(key);
        result.push('=');
        result.push_str(value);
        result.push('\n');
    }
    Ok(result)
}

fn random() -> Result<String> {
    let mut bytes = [0u8; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn stage_private(path: &Path, bytes: &[u8]) -> Result<tempfile::NamedTempFile> {
    let parent = path.parent().context("Private file has no parent")?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(bytes)?;
    staged.as_file().sync_all()?;
    Ok(staged)
}

// These outputs live in different directories. Stage both before publishing,
// and roll back only this attempt's newly published login if secrets fail.
fn publish_fresh_credentials(login: (&Path, &[u8]), secrets: (&Path, &[u8])) -> Result<()> {
    let staged_login = stage_private(login.0, login.1)?;
    let staged_secrets = stage_private(secrets.0, secrets.1)?;
    let _login = staged_login
        .persist_noclobber(login.0)
        .map_err(|error| error.error)
        .context("Cannot publish private viewer login")?;
    if let Err(error) = staged_secrets.persist_noclobber(secrets.0) {
        let error =
            anyhow::Error::new(error.error).context("Cannot publish encrypted service secrets");
        if let Err(cleanup) = fs::remove_file(login.0) {
            return Err(error.context(format!(
                "Could not roll back this attempt's viewer login: {cleanup}"
            )));
        }
        return Err(error);
    }
    Ok(())
}

fn go_gateway_environment(text: &str) -> Result<BTreeMap<String, String>> {
    let mut gateway = parse_environment(text)?;
    ensure!(
        gateway
            .get("OPENCODE_GO_API_KEY")
            .is_some_and(|key| !key.is_empty()),
        "OpenCode Go API key is absent"
    );
    // Hermes spends only the Go subscription. Reuse must not reintroduce
    // inference credentials from the earlier multi-provider configuration.
    for key in [
        "OPENCODE_ZEN_API_KEY",
        "OPENAI_API_KEY",
        "OPENROUTER_API_KEY",
    ] {
        gateway.remove(key);
    }
    Ok(gateway)
}

fn ensure_keyring_password(values: &mut Value) -> Result<()> {
    let values = values
        .as_object_mut()
        .context("Service secrets must be an object")?;
    if values.get("hermes_keyring_password").is_none() {
        values.insert("hermes_keyring_password".into(), json!(random()?));
    }
    ensure!(
        values["hermes_keyring_password"]
            .as_str()
            .is_some_and(|password| !password.is_empty() && !password.contains(['\n', '\r'])),
        "Saved keyring password is invalid"
    );
    Ok(())
}

fn reuse_telegram(gateway: &mut BTreeMap<String, String>, token: &str, owner: &str) {
    gateway.remove("HERMES_ALLOW_ALL_USERS");
    for (key, value) in [
        ("TELEGRAM_BOT_TOKEN", token),
        ("TELEGRAM_ALLOWED_USERS", owner),
        ("TELEGRAM_HOME_CHANNEL", owner),
        ("TELEGRAM_ALLOW_ALL_USERS", "false"),
        ("GATEWAY_ALLOW_ALL_USERS", "false"),
    ] {
        gateway.insert(key.to_string(), value.to_string());
    }
}

pub fn provision(root: &Path, converter: &Path, reuse: bool) -> Result<()> {
    let root = root.canonicalize()?;
    let destination = root.join("secrets/hermes.yaml");
    let mut pending_login = None;
    let mut values = if destination.exists() {
        ensure!(
            reuse,
            "Secrets already exist; never regenerate deployed credentials"
        );
        decrypt(&destination, converter)?
    } else {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .context("HOME is absent")?;
        let auth: Value =
            serde_json::from_slice(&fs::read(home.join(".local/share/opencode/auth.json"))?)?;
        let model_key = auth["opencode-go"]["key"]
            .as_str()
            .context("OpenCode Go API key is absent")?;
        let access = random()?;
        let admin = random()?;
        let cookies = random()?;
        let gate = random()?;
        let viewer = random()?;
        let password = random()?;
        let hash = String::from_utf8(capture(
            Command::new("caddy").args(["hash-password", "--algorithm", "bcrypt"]),
            Some(format!("{password}\n").as_bytes()),
        )?)?;
        ensure!(
            hash.trim().starts_with("$2"),
            "Caddy did not return a bcrypt hash"
        );
        let credentials = home.join(".config/hermes/viewer.json");
        pending_login = Some((
            credentials,
            serde_json::to_vec_pretty(
                &json!({"url":"https://mirsella.mooo.com/browser/","username":"mirsella","password":password}),
            )?,
        ));
        json!({
            "hermes_gateway_env":environment([("OPENCODE_GO_API_KEY",model_key),("CAMOFOX_API_KEY",&gate),("HERMES_BROWSER_KEY",&gate),("TELEGRAM_ALLOW_ALL_USERS","false"),("GATEWAY_ALLOW_ALL_USERS","false")])?,
            "hermes_browser_env":environment([("CAMOFOX_ACCESS_KEY",access.as_str()),("CAMOFOX_ADMIN_KEY",&admin),("CAMOFOX_API_KEY",&cookies)])?,
            "hermes_control_env":environment([("CAMOFOX_ACCESS_KEY",access.as_str()),("CAMOFOX_ADMIN_KEY",&admin),("HERMES_BROWSER_KEY",&gate),("HERMES_VIEWER_KEY",&viewer)])?,
            "hermes_caddy_env":environment([("HERMES_VIEWER_PASSWORD_HASH",hash.trim()),("HERMES_VIEWER_KEY",&viewer)])?
        })
    };
    let mut gateway = go_gateway_environment(
        values["hermes_gateway_env"]
            .as_str()
            .context("Gateway environment is absent")?,
    )?;
    ensure_keyring_password(&mut values)?;
    if reuse {
        let saved = decrypt(&root.join("secrets/services.yaml"), converter)?;
        let saved = parse_environment(
            saved["telegram_env"]
                .as_str()
                .context("Saved Telegram environment is absent")?,
        )?;
        let token = saved.get("TgToken").context("TgToken is absent")?;
        let owner = saved.get("TgId").context("TgId is absent")?;
        ensure!(
            (4..=15).contains(&owner.len())
                && !owner.starts_with('0')
                && owner.bytes().all(|b| b.is_ascii_digit()),
            "Saved Telegram destination must be a private positive numeric owner"
        );
        let client: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .max_redirects(0)
            .proxy(None)
            .http_status_as_error(false)
            .build()
            .into();
        let api = |method: &str| -> Result<Value> {
            let response = client
                .get(format!("https://api.telegram.org/bot{token}/{method}"))
                .call()
                .map_err(|_| {
                    anyhow::anyhow!("Telegram verification failed; credential withheld")
                })?;
            ensure!(
                response.status().is_success(),
                "Telegram verification was rejected"
            );
            let value: Value = response
                .into_body()
                .read_json()
                .map_err(|_| anyhow::anyhow!("Telegram returned malformed verification data"))?;
            ensure!(value["ok"] == true, "Telegram verification failed");
            Ok(value["result"].clone())
        };
        ensure!(
            api("getMe")?["username"] == "mirsellabot",
            "Saved token belongs to another bot"
        );
        ensure!(
            api("getWebhookInfo")?["url"] == "",
            "Existing webhook must be resolved before long polling"
        );
        let chat = api(&format!("getChat?chat_id={owner}"))?;
        ensure!(
            chat["type"] == "private"
                && chat["id"].as_u64().map(|id| id.to_string()).as_deref() == Some(owner),
            "Saved destination is not the owner's private chat"
        );
        reuse_telegram(&mut gateway, token, owner);
        println!("Verified @mirsellabot, private owner {owner}; existing credentials preserved");
    }
    values["hermes_gateway_env"] = json!(environment(&gateway)?);
    let encrypted = capture(
        Command::new("sops").current_dir(&root).args([
            "encrypt",
            "--filename-override",
            "secrets/hermes.yaml",
            "--input-type",
            "json",
            "--output-type",
            "yaml",
            "/dev/stdin",
        ]),
        Some(&serde_json::to_vec(&values)?),
    )?;
    if let Some((credentials, bytes)) = pending_login {
        publish_fresh_credentials((&credentials, &bytes), (&destination, &encrypted))?;
        println!("Private viewer login: {}", credentials.display());
    } else {
        stage_private(&destination, &encrypted)?
            .persist(&destination)
            .map_err(|error| error.error)
            .context("Cannot replace encrypted service secrets")?;
    }
    println!("Encrypted service secrets: {}", destination.display());
    Ok(())
}

// Pair effectful stops with recovery, including partially failed stops.
fn with_service_recovery(
    operation: impl FnOnce() -> Result<()>,
    restart: impl FnOnce() -> Result<()>,
) -> Result<()> {
    match (operation(), restart()) {
        (result, Ok(())) => result,
        (Ok(_), Err(error)) => Err(error.context("Service recovery failed")),
        (Err(error), Err(recovery)) => {
            Err(error.context(format!("Service recovery also failed: {recovery:#}")))
        }
    }
}

fn operation_lock() -> Result<fs::File> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open("/run/lock/hermes-operations.lock")?;
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.uid() == 0 && metadata.mode() & 0o077 == 0,
        "Hermes operation lock must be a private root-owned file"
    );
    match file.try_lock() {
        Ok(()) => {}
        Err(fs::TryLockError::WouldBlock) => {
            bail!("Another Hermes lifecycle/backup/restore operation is running");
        }
        Err(fs::TryLockError::Error(error)) => {
            return Err(error).context("Cannot lock Hermes lifecycle operations");
        }
    }
    Ok(file)
}

fn require_tank_backup_mount() -> Result<()> {
    crate::util::run(
        "findmnt",
        &[
            "--noheadings",
            "--source",
            "tank/backup",
            "--mountpoint",
            "/srv/backup",
        ],
    )
    .context("Tank backup dataset is not mounted at /srv/backup")
}

fn prune_backups(current: &Path) -> Result<()> {
    let directory = current.parent().context("Backup has no parent directory")?;
    let current = current.file_name().context("Backup has no file name")?;
    let mut archives = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        if name == current {
            continue;
        }
        let timestamp = name
            .to_str()
            .and_then(|name| name.strip_prefix("hermes-")?.strip_suffix(".tar.gz"))
            .and_then(|timestamp| {
                chrono::NaiveDateTime::parse_from_str(timestamp, BACKUP_TIMESTAMP_FORMAT).ok()
            });
        if let Some(timestamp) = timestamp {
            archives.push((timestamp, name));
        }
    }
    archives.sort_unstable();
    let excess = archives.len().saturating_sub(BACKUP_RETENTION - 1);
    for (_, name) in archives.into_iter().take(excess) {
        fs::remove_file(directory.join(name))?;
    }
    Ok(())
}

pub fn operations(action: Operation) -> Result<()> {
    const UNITS: [&str; 4] = [
        "hermes-backend.service",
        "hermes-agent.service",
        "hermes-browser-control.service",
        "camofox-browser.service",
    ];
    let systemctl = |action: &str| -> Result<()> {
        ensure!(
            Command::new("systemctl")
                .arg(action)
                .args(UNITS)
                .status()?
                .success(),
            "Service operation {action} failed"
        );
        Ok(())
    };
    if matches!(action, Operation::Status) {
        return systemctl("status");
    }
    if matches!(action, Operation::Logs) {
        let mut command = Command::new("journalctl");
        command.arg("--follow");
        for unit in UNITS {
            command.arg("--unit").arg(unit);
        }
        return Err(command.exec().into());
    }
    ensure!(
        unsafe { libc::geteuid() } == 0,
        "Lifecycle operations require the administrator"
    );
    let _operation_lock = operation_lock()?;
    let worker_owner = || -> Result<(String, u32)> {
        let user = crate::util::output(
            "systemctl",
            &["show", "hermes-agent.service", "--property=User", "--value"],
        )?;
        let user = user.trim();
        ensure!(!user.is_empty(), "Hermes unit has no service user");
        let uid = crate::util::output("id", &["-u", user])?
            .trim()
            .parse::<u32>()
            .context("Invalid Hermes service UID")?;
        Ok((user.to_owned(), uid))
    };
    let stop = |(user, uid): &(String, u32)| -> Result<()> {
        systemctl("stop")?;
        // Stop native detached worker scopes, never the owner's user manager.
        stop_workers(user, *uid)
    };
    match action {
        Operation::Start => systemctl("start"),
        Operation::Stop => stop(&worker_owner()?),
        Operation::Restart => {
            let owner = worker_owner()?;
            with_service_recovery(|| stop(&owner), || systemctl("start"))
        }
        Operation::Backup => {
            require_tank_backup_mount()?;
            let owner = worker_owner()?;
            let directory = Path::new(TANK_BACKUP_DIRECTORY);
            fs::DirBuilder::new()
                .mode(0o700)
                .recursive(true)
                .create(directory)?;
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
            let name = format!(
                "hermes-{}.tar.gz",
                chrono::Utc::now().format(BACKUP_TIMESTAMP_FORMAT)
            );
            let final_path = directory.join(name);
            let mut previously_active = Vec::new();
            for unit in UNITS {
                let status = Command::new("systemctl")
                    .args(["is-active", "--quiet"])
                    .arg(unit)
                    .status()?;
                match status.code() {
                    Some(0) => previously_active.push(unit),
                    Some(3) => {} // Known unit, not active.
                    _ => bail!("Cannot determine backup service state for {unit}: {status}"),
                }
            }
            // Prepare the private destination before changing service state.
            let (file, temporary) = tempfile::Builder::new()
                .prefix("hermes-")
                .suffix(".tar.gz.partial")
                .tempfile_in(directory)?
                .keep()?;
            with_service_recovery(
                || {
                    stop(&owner)?;
                    let result = Command::new("tar")
                        .args([
                            "--create",
                            "--gzip",
                            "--xattrs",
                            "--acls",
                            "--hard-dereference",
                            "--directory",
                            "/var/lib",
                            "--exclude=hermes/.hermes/cache",
                            "--exclude=camofox/cache",
                            "--file",
                        ])
                        .arg(&temporary)
                        .args(restore::ROOTS)
                        .status()?;
                    ensure!(
                        result.success(),
                        "Backup failed; partial archive retained at {}",
                        temporary.display()
                    );
                    restore::validate_backup(&temporary)
                        .context("Backup cannot be restored; partial archive retained")?;
                    Ok(())
                },
                || {
                    if !previously_active.is_empty() {
                        ensure!(
                            Command::new("systemctl")
                                .arg("start")
                                .args(&previously_active)
                                .status()?
                                .success(),
                            "Previously active services could not restart"
                        );
                    }
                    Ok(())
                },
            )?;
            file.sync_all()?;
            // Publish atomically without replacing an existing backup, even
            // across a clock adjustment or simultaneous administrator run.
            fs::hard_link(&temporary, &final_path)?;
            fs::remove_file(temporary)?;
            fs::File::open(directory)?.sync_all()?;
            prune_backups(&final_path).with_context(|| {
                format!(
                    "Backup saved at {}, but retention failed",
                    final_path.display()
                )
            })?;
            println!("Private backup: {}", final_path.display());
            Ok(())
        }
        Operation::Restore { archive } => {
            let archive = archive.canonicalize()?;
            if archive.starts_with(TANK_BACKUP_DIRECTORY) {
                require_tank_backup_mount()?;
            } else {
                ensure!(
                    archive.starts_with(LEGACY_BACKUP_DIRECTORY),
                    "Restore archives must be in a root-only Hermes backup directory"
                );
            }
            // Validate and extract before stopping anything. Replace complete
            // directories, rather than merging old files into the live state.
            let staged = restore::stage(&archive, Path::new("/var/lib"))?;
            stop(&worker_owner()?)?;
            let previous = restore::replace(Path::new("/var/lib"), staged)?;
            println!(
                "State restored; previous state retained at {}. Rebuild the reviewed flake to reinstall managed configuration, then explicitly start services.",
                previous.display()
            );
            Ok(())
        }
        Operation::Status | Operation::Logs => {
            unreachable!("read-only operations returned before the administrator lock")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_capture_drains_stdout_and_discards_stderr_while_sending_large_input() {
        let directory = tempfile::tempdir().unwrap();
        let client = directory.path().join("duplex-client");
        fs::write(&client, "#!/bin/sh\ndd if=/dev/zero bs=65536 count=4 2>/dev/null\ndd if=/dev/zero bs=65536 count=4 >&2 2>/dev/null\ndd bs=65536 2>/dev/null\n").unwrap();
        fs::set_permissions(&client, fs::Permissions::from_mode(0o700)).unwrap();
        let input = vec![b'x'; 512 * 1024];
        let output = capture(Command::new("timeout").arg("5s").arg(&client), Some(&input)).unwrap();
        let prefix = 4 * 65536;
        assert_eq!(output.len(), prefix + input.len());
        assert!(output[..prefix].iter().all(|byte| *byte == 0));
        assert_eq!(&output[prefix..], input);
    }

    #[test]
    fn credential_capture_reports_failed_child_without_private_diagnostics() {
        let directory = tempfile::tempdir().unwrap();
        let client = directory.path().join("failed-client");
        fs::write(
            &client,
            "#!/bin/sh\nprintf 'private-credential-marker' >&2\nexit 7\n",
        )
        .unwrap();
        fs::set_permissions(&client, fs::Permissions::from_mode(0o700)).unwrap();
        let input = vec![b'x'; 512 * 1024];
        for stdin in [None, Some(input.as_slice())] {
            let error = capture(&mut Command::new(&client), stdin)
                .unwrap_err()
                .to_string();
            assert!(error.contains("exit status: 7"));
            assert!(!error.contains("private-credential-marker"));
        }
    }

    #[test]
    fn telegram_reuse_restores_private_intake_without_rotating_credentials() {
        let mut gateway = parse_environment("OPENCODE_GO_API_KEY=existing-go\nOPENCODE_ZEN_API_KEY=existing-zen\nHERMES_BROWSER_KEY=existing-browser\nTELEGRAM_ALLOW_ALL_USERS=true\nGATEWAY_ALLOW_ALL_USERS=true\nHERMES_ALLOW_ALL_USERS=true\nTELEGRAM_ALLOWED_USERS=other\n").unwrap();
        reuse_telegram(&mut gateway, "verified-bot", "932980505");
        assert_eq!(gateway["TELEGRAM_ALLOW_ALL_USERS"], "false");
        assert_eq!(gateway["GATEWAY_ALLOW_ALL_USERS"], "false");
        assert!(!gateway.contains_key("HERMES_ALLOW_ALL_USERS"));
        assert_eq!(gateway["TELEGRAM_BOT_TOKEN"], "verified-bot");
        assert_eq!(gateway["TELEGRAM_ALLOWED_USERS"], "932980505");
        assert_eq!(gateway["TELEGRAM_HOME_CHANNEL"], "932980505");
        assert_eq!(gateway["OPENCODE_GO_API_KEY"], "existing-go");
        assert_eq!(gateway["OPENCODE_ZEN_API_KEY"], "existing-zen");
        assert_eq!(gateway["HERMES_BROWSER_KEY"], "existing-browser");
        let saved = gateway.clone();
        reuse_telegram(&mut gateway, "verified-bot", "932980505");
        assert_eq!(gateway, saved);
    }

    #[test]
    fn go_gateway_removes_other_inference_keys_without_rotating_shared_credentials() {
        let gateway = go_gateway_environment("OPENCODE_GO_API_KEY=existing-go\nOPENCODE_ZEN_API_KEY=old-zen\nOPENAI_API_KEY=old-openai\nOPENROUTER_API_KEY=old-router\nHERMES_BROWSER_KEY=existing-browser\n").unwrap();
        assert_eq!(gateway["OPENCODE_GO_API_KEY"], "existing-go");
        assert!(!gateway.contains_key("OPENCODE_ZEN_API_KEY"));
        assert!(!gateway.contains_key("OPENAI_API_KEY"));
        assert!(!gateway.contains_key("OPENROUTER_API_KEY"));
        assert_eq!(gateway["HERMES_BROWSER_KEY"], "existing-browser");
        assert_eq!(
            go_gateway_environment(&environment(&gateway).unwrap()).unwrap(),
            gateway
        );
        for invalid in ["", "OPENCODE_GO_API_KEY=\n", "OPENCODE_ZEN_API_KEY=zen\n"] {
            assert!(go_gateway_environment(invalid).is_err());
        }
    }

    #[test]
    fn backup_retention_keeps_only_the_newest_complete_archives() {
        let directory = tempfile::tempdir().unwrap();
        for index in 0..16 {
            fs::write(
                directory
                    .path()
                    .join(format!("hermes-20261009T{index:06}.000000000Z.tar.gz")),
                b"archive",
            )
            .unwrap();
        }
        fs::write(
            directory.path().join("hermes-999.tar.gz.partial"),
            b"partial",
        )
        .unwrap();
        fs::write(directory.path().join("unrelated.tar.gz"), b"unrelated").unwrap();
        fs::write(
            directory.path().join("hermes-not-a-backup.tar.gz"),
            b"unrelated",
        )
        .unwrap();
        fs::create_dir(
            directory
                .path()
                .join("hermes-20261010T000000.000000000Z.tar.gz"),
        )
        .unwrap();
        std::os::unix::fs::symlink(
            "unrelated.tar.gz",
            directory
                .path()
                .join("hermes-20261011T000000.000000000Z.tar.gz"),
        )
        .unwrap();

        prune_backups(
            &directory
                .path()
                .join("hermes-20261009T000015.000000000Z.tar.gz"),
        )
        .unwrap();

        let remaining = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(remaining.len(), 19);
        assert!(remaining.contains("hermes-20261009T000015.000000000Z.tar.gz"));
        assert!(!remaining.contains("hermes-20261009T000000.000000000Z.tar.gz"));
        assert!(!remaining.contains("hermes-20261009T000001.000000000Z.tar.gz"));
        assert!(remaining.contains("hermes-999.tar.gz.partial"));
        assert!(remaining.contains("unrelated.tar.gz"));
        assert!(remaining.contains("hermes-not-a-backup.tar.gz"));
        assert!(remaining.contains("hermes-20261010T000000.000000000Z.tar.gz"));
        assert!(remaining.contains("hermes-20261011T000000.000000000Z.tar.gz"));
    }

    #[test]
    fn backup_retention_preserves_current_archive_after_clock_moves_backwards() {
        let directory = tempfile::tempdir().unwrap();
        for index in 0..16 {
            fs::write(
                directory
                    .path()
                    .join(format!("hermes-20261009T{index:06}.000000000Z.tar.gz")),
                b"archive",
            )
            .unwrap();
        }
        let current = directory
            .path()
            .join("hermes-20261009T000000.000000000Z.tar.gz");
        prune_backups(&current).unwrap();
        assert!(current.exists());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 14);
        assert!(
            !directory
                .path()
                .join("hermes-20261009T000001.000000000Z.tar.gz")
                .exists()
        );
        assert!(
            !directory
                .path()
                .join("hermes-20261009T000002.000000000Z.tar.gz")
                .exists()
        );
        assert!(
            directory
                .path()
                .join("hermes-20261009T000015.000000000Z.tar.gz")
                .exists()
        );
    }

    #[test]
    fn keyring_password_is_added_once_without_rotating_existing_values() {
        let mut values = json!({"hermes_control_env":"existing-control"});
        ensure_keyring_password(&mut values).unwrap();
        assert_eq!(values["hermes_control_env"], "existing-control");
        assert_eq!(
            values["hermes_keyring_password"].as_str().unwrap().len(),
            64
        );
        let saved = values.clone();
        ensure_keyring_password(&mut values).unwrap();
        assert_eq!(values, saved);
        for invalid in ["", "invalid\npassword", "invalid\rpassword"] {
            values["hermes_keyring_password"] = json!(invalid);
            assert!(ensure_keyring_password(&mut values).is_err());
            assert_eq!(values["hermes_keyring_password"], invalid);
        }
        for invalid in [json!(null), json!(false), json!([])] {
            let mut malformed = invalid.clone();
            assert!(ensure_keyring_password(&mut malformed).is_err());
            assert_eq!(malformed, invalid);
            values["hermes_keyring_password"] = invalid.clone();
            assert!(ensure_keyring_password(&mut values).is_err());
            assert_eq!(values["hermes_keyring_password"], invalid);
        }
    }

    #[test]
    fn defaults_preserve_runtime_model_and_other_owner_settings() {
        let mut settings =
            json!({"model":{"default":"chosen","provider":"openai-codex"},"custom":true});
        assert!(fill_defaults(
            &mut settings,
            &json!({"model":{"default":"seed","provider":"opencode-go"},"browser":{"backend":"browserbase"}})
        ));
        assert_eq!(settings["model"]["default"], "chosen");
        assert_eq!(settings["model"]["provider"], "openai-codex");
        assert_eq!(settings["custom"], true);
        assert_eq!(settings["browser"]["backend"], "browserbase");
        assert!(!fill_defaults(
            &mut settings,
            &json!({"model":{"default":"seed"}})
        ));
    }

    #[test]
    fn worker_quiescing_never_selects_the_owner_manager_or_other_services() {
        assert_eq!(worker_units("hermes-worker-cron-123.scope loaded active running job\nhermes-worker-kanban-4.scope loaded active running task\n").unwrap().len(), 2);
        for unit in [
            "user@1000.service",
            "sleev-gateway.service",
            "unrelated.scope",
            "hermes-worker-123.scope;evil",
        ] {
            assert!(worker_units(unit).is_err());
        }
    }

    #[test]
    fn configuration_seeding_preserves_owner_settings_and_does_not_rewrite_unchanged_files() {
        use std::os::unix::fs::MetadataExt;
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("config.yaml");
        let defaults = directory.path().join("defaults.json");
        fs::write(&defaults, br#"{"model":{"provider":"opencode-go","default":"seed"},"browser":{"backend":"browserbase"}}"#).unwrap();
        fs::write(
            &config,
            "model:\n  provider: openai-codex\n  default: chosen\n",
        )
        .unwrap();
        configure(&config, &defaults).unwrap();
        let settings: Value = serde_yaml::from_slice(&fs::read(&config).unwrap()).unwrap();
        assert_eq!(settings["model"]["default"], "chosen");
        assert_eq!(settings["browser"]["backend"], "browserbase");
        let config_file = fs::File::open(&config).unwrap();
        configure(&config, &defaults).unwrap();
        assert_eq!(
            fs::metadata(&config).unwrap().ino(),
            config_file.metadata().unwrap().ino()
        );
        assert_eq!(fs::metadata(&config).unwrap().mode() & 0o777, 0o600);
        fs::remove_file(&config).unwrap();
        configure(&config, &defaults).unwrap();
        let seeded: Value = serde_yaml::from_slice(&fs::read(&config).unwrap()).unwrap();
        assert_eq!(seeded["model"]["default"], "seed");
        let before = fs::read(&config).unwrap();
        fs::write(&defaults, b"[]").unwrap();
        assert!(configure(&config, &defaults).is_err());
        assert_eq!(fs::read(&config).unwrap(), before);
    }

    #[test]
    fn fresh_credential_publication_cleans_failure_and_preserves_existing_outputs() {
        let directory = tempfile::tempdir().unwrap();
        let login = directory.path().join("login/viewer.json");
        let secrets = directory.path().join("secrets/hermes.yaml");
        fs::create_dir_all(secrets.parent().unwrap()).unwrap();
        fs::write(&secrets, b"existing ciphertext").unwrap();
        assert!(
            publish_fresh_credentials((&login, b"new login"), (&secrets, b"new ciphertext"))
                .is_err()
        );
        assert!(!login.exists());
        assert_eq!(fs::read(&secrets).unwrap(), b"existing ciphertext");
        assert_eq!(fs::read_dir(secrets.parent().unwrap()).unwrap().count(), 1);
        assert_eq!(fs::read_dir(login.parent().unwrap()).unwrap().count(), 0);
        fs::remove_file(&secrets).unwrap();
        publish_fresh_credentials((&login, b"new login"), (&secrets, b"new ciphertext")).unwrap();
        assert_eq!(
            fs::metadata(&login).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&secrets).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(
            publish_fresh_credentials((&login, b"replacement"), (&secrets, b"replacement"))
                .is_err()
        );
        assert_eq!(fs::read(&login).unwrap(), b"new login");
        assert_eq!(fs::read(&secrets).unwrap(), b"new ciphertext");
    }

    #[test]
    fn fresh_credential_staging_failure_publishes_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let login = directory.path().join("login/viewer.json");
        let invalid_parent = directory.path().join("not-a-directory");
        fs::write(&invalid_parent, b"keep").unwrap();
        let secrets = invalid_parent.join("hermes.yaml");
        assert!(publish_fresh_credentials((&login, b"login"), (&secrets, b"ciphertext")).is_err());
        assert!(!login.exists());
        assert_eq!(fs::read(&invalid_parent).unwrap(), b"keep");
        assert_eq!(fs::read_dir(login.parent().unwrap()).unwrap().count(), 0);
    }

    #[test]
    fn backup_restores_services_after_failed_stop_or_archive() {
        use std::cell::RefCell;
        for failed_step in ["stop", "archive", "restart"] {
            let calls = RefCell::new(Vec::new());
            let step = |name| {
                calls.borrow_mut().push(name);
                ensure!(name != failed_step, "{name} failed");
                Ok(())
            };
            assert!(
                with_service_recovery(
                    || {
                        step("stop")?;
                        step("archive")
                    },
                    || step("restart"),
                )
                .is_err()
            );
            assert_eq!(calls.borrow().last(), Some(&"restart"));
            assert_eq!(calls.borrow().contains(&"archive"), failed_step != "stop");
        }
        let error =
            with_service_recovery(|| bail!("stop failed"), || bail!("restart failed")).unwrap_err();
        let diagnostic = format!("{error:#}");
        assert!(diagnostic.contains("stop failed") && diagnostic.contains("restart failed"));
    }

    #[test]
    fn restart_recovers_after_worker_stop_failure_and_preserves_recovery_errors() {
        use std::cell::Cell;
        for recovery_fails in [false, true] {
            let running = Cell::new(true);
            let error = with_service_recovery(
                || {
                    running.set(false);
                    bail!("worker stop failed");
                },
                || {
                    ensure!(!recovery_fails, "unit start failed");
                    running.set(true);
                    Ok(())
                },
            )
            .unwrap_err();
            assert_eq!(running.get(), !recovery_fails);
            let diagnostic = format!("{error:#}");
            assert!(diagnostic.contains("worker stop failed"));
            assert_eq!(diagnostic.contains("unit start failed"), recovery_fails);
        }
    }

    #[test]
    fn environment_round_trip_and_newline_denial() {
        let values = BTreeMap::from([
            ("TOKEN".into(), "x=y$z".into()),
            ("OWNER".into(), "932980505".into()),
        ]);
        assert_eq!(
            parse_environment(&environment(&values).unwrap()).unwrap(),
            values
        );
        assert!(environment([("TOKEN", "one\ntwo")]).is_err());
        for key in ["", "1TOKEN", "BAD-KEY", "TOKEN\nOTHER"] {
            assert!(environment([(key, "value")]).is_err());
            assert!(parse_environment(&format!("{key}=value\n")).is_err());
        }
    }
}
