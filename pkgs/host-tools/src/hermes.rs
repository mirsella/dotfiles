//! Owner-operated Hermes secret provisioning and service/backup operations.
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

fn private_agent(seconds: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(seconds)))
        .max_redirects(0)
        .proxy(None)
        .http_status_as_error(false)
        .build()
        .into()
}

fn capture(command: &mut Command, stdin: Option<&[u8]>) -> Result<Vec<u8>> {
    let name = command.get_program().to_string_lossy().into_owned();
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    let mut child = command
        .spawn()
        .with_context(|| format!("Cannot start {name}"))?;
    if let Some(bytes) = stdin {
        child
            .stdin
            .take()
            .context("Command stdin was not piped")?
            .write_all(bytes)?;
    }
    let output = child.wait_with_output()?;
    // Never include subprocess stderr: credential-bearing requests may appear there.
    ensure!(
        output.status.success(),
        "{name} failed; diagnostic withheld because it may contain credentials"
    );
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

fn parse_environment(text: &str) -> Result<BTreeMap<String, String>> {
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .map(|line| {
            let (key, value) = line
                .split_once('=')
                .context("Malformed saved environment")?;
            let key = key.trim().strip_prefix("export ").unwrap_or(key.trim());
            ensure!(
                key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "Malformed environment key"
            );
            Ok((
                key.to_string(),
                value.trim().trim_matches(['\'', '"']).to_string(),
            ))
        })
        .collect()
}

fn environment(values: &BTreeMap<String, String>) -> Result<String> {
    let mut result = String::new();
    for (key, value) in values {
        ensure!(!value.contains(['\n', '\r']), "Multiline value for {key}");
        result.push_str(&format!("{key}={value}\n"));
    }
    Ok(result)
}

fn random() -> Result<String> {
    let mut bytes = [0u8; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn private_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("Private file has no parent")?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
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
        let env_block = |pairs: &[(&str, &str)]| -> Result<String> {
            environment(
                &pairs
                    .iter()
                    .map(|(key, value)| (key.to_string(), value.to_string()))
                    .collect(),
            )
        };
        json!({
            "hermes_gateway_env":env_block(&[("OPENCODE_GO_API_KEY",model_key),("CAMOFOX_API_KEY",&gate),("HERMES_BROWSER_KEY",&gate),("TELEGRAM_ALLOW_ALL_USERS","false"),("HERMES_ALLOW_ALL_USERS","false")])?,
            "hermes_browser_env":env_block(&[("CAMOFOX_ACCESS_KEY",&access),("CAMOFOX_ADMIN_KEY",&admin),("CAMOFOX_API_KEY",&cookies)])?,
            "hermes_control_env":env_block(&[("CAMOFOX_ACCESS_KEY",&access),("CAMOFOX_ADMIN_KEY",&admin),("HERMES_BROWSER_KEY",&gate),("HERMES_VIEWER_KEY",&viewer)])?,
            "hermes_caddy_env":env_block(&[("HERMES_VIEWER_PASSWORD_HASH",hash.trim()),("HERMES_VIEWER_KEY",&viewer)])?
        })
    };
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
        let api = |method: &str| -> Result<Value> {
            let response = private_agent(20)
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
        let mut gateway = parse_environment(
            values["hermes_gateway_env"]
                .as_str()
                .context("Gateway environment is absent")?,
        )?;
        for (key, value) in [
            ("TELEGRAM_BOT_TOKEN", token),
            ("TELEGRAM_ALLOWED_USERS", owner),
            ("TELEGRAM_HOME_CHANNEL", owner),
        ] {
            gateway.insert(key.to_string(), value.clone());
        }
        values["hermes_gateway_env"] = json!(environment(&gateway)?);
        println!("Verified @mirsellabot, private owner {owner}; existing credentials preserved");
    }
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

// Even a partially failed stop or archive must restore the previously active
// services. Keep recovery paired with the effects rather than scattered returns.
fn quiesced_backup<T>(
    stop: impl FnOnce() -> Result<()>,
    archive: impl FnOnce() -> Result<T>,
    restart: impl FnOnce() -> Result<()>,
) -> Result<T> {
    let result = stop().and_then(|_| archive());
    match (result, restart()) {
        (result, Ok(())) => result,
        (Ok(_), Err(error)) => Err(error.context("Backup finished but service recovery failed")),
        (Err(error), Err(recovery)) => {
            Err(error.context(format!("Service recovery also failed: {recovery:#}")))
        }
    }
}

pub fn operations(action: &str, archive: Option<&Path>) -> Result<()> {
    let mut units = vec![
        "hermes-agent.service".to_string(),
        "hermes-browser-control.service".to_string(),
        "camofox-browser.service".to_string(),
    ];
    if matches!(action, "start" | "stop" | "restart" | "backup" | "restore") {
        ensure!(
            unsafe { libc::geteuid() } == 0,
            "Lifecycle operations require the administrator"
        );
        let uid = String::from_utf8(capture(
            Command::new("id").args(["-u", "hermes-agent"]),
            None,
        )?)?;
        let uid = uid
            .trim()
            .parse::<u32>()
            .context("Invalid Hermes service UID")?;
        // Restart-safe cron workers live in this dedicated user's manager,
        // outside the gateway unit. Quiesce them for stop/consistent backups.
        units.push(format!("user@{uid}.service"));
    }
    let systemctl = |action: &str| -> Result<()> {
        ensure!(
            Command::new("systemctl")
                .arg(action)
                .args(&units)
                .status()?
                .success(),
            "Service operation {action} failed"
        );
        Ok(())
    };
    if action == "status" {
        return systemctl("status");
    }
    if action == "logs" {
        let mut command = Command::new("journalctl");
        command.arg("--follow");
        for unit in &units {
            command.arg("--unit").arg(unit);
        }
        return Err(command.exec().into());
    }
    match action {
        "start" | "stop" | "restart" => systemctl(action),
        "backup" => {
            let directory = Path::new("/var/lib/hermes-backups");
            fs::DirBuilder::new()
                .mode(0o700)
                .recursive(true)
                .create(directory)?;
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
            let name = format!(
                "hermes-{}.tar.gz",
                chrono::Utc::now().format("%Y%m%dT%H%M%S%.9fZ")
            );
            let final_path = directory.join(name);
            let temporary = final_path.with_extension("tar.gz.partial");
            let mut previously_active = Vec::new();
            for unit in &units {
                if Command::new("systemctl")
                    .args(["is-active", "--quiet"])
                    .arg(unit)
                    .status()?
                    .success()
                {
                    previously_active.push(unit);
                }
            }
            // Prepare the private destination before changing service state.
            private_write(&temporary, b"")?;
            quiesced_backup(
                || systemctl("stop"),
                || {
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
                        .args(["hermes", "camofox", "hermes-browser-control"])
                        .status()?;
                    ensure!(
                        result.success(),
                        "Backup failed; partial archive retained at {}",
                        temporary.display()
                    );
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
            fs::File::open(&temporary)?.sync_all()?;
            // Publish atomically without replacing an existing backup, even
            // across a clock adjustment or simultaneous administrator run.
            fs::hard_link(&temporary, &final_path)?;
            fs::remove_file(temporary)?;
            println!("Private backup: {}", final_path.display());
            Ok(())
        }
        "restore" => {
            let archive = archive
                .context("Restore requires --archive")?
                .canonicalize()?;
            ensure!(
                archive.starts_with("/var/lib/hermes-backups"),
                "Restore archives must be in the root-only backup directory"
            );
            let members = capture(
                Command::new("tar")
                    .args(["--list", "--gzip", "--file"])
                    .arg(&archive),
                None,
            )?;
            for member in String::from_utf8(members)?.lines() {
                let path = Path::new(member);
                ensure!(path.components().all(|part|matches!(part,std::path::Component::Normal(_))) && path.components().next().is_some_and(|part|matches!(part,std::path::Component::Normal(name) if matches!(name.to_str(),Some("hermes"|"camofox"|"hermes-browser-control")))), "Backup member escapes approved state directories");
            }
            systemctl("stop")?;
            ensure!(
                Command::new("tar")
                    .args([
                        "--extract",
                        "--gzip",
                        "--xattrs",
                        "--acls",
                        "--same-owner",
                        "--directory",
                        "/var/lib",
                        "--file"
                    ])
                    .arg(archive)
                    .status()?
                    .success(),
                "Restore failed; services remain stopped"
            );
            println!(
                "State restored. Rebuild the reviewed flake to reinstall managed configuration, then explicitly start services."
            );
            Ok(())
        }
        _ => bail!("Unknown Hermes operation"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_credential_publication_cleans_failure_and_preserves_existing_outputs() {
        let directory = tempfile::tempdir().unwrap();
        let login = directory.path().join("login/viewer.json");
        let secrets = directory.path().join("secrets/hermes.yaml");
        private_write(&secrets, b"existing ciphertext").unwrap();
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
                quiesced_backup(|| step("stop"), || step("archive"), || step("restart")).is_err()
            );
            assert_eq!(calls.borrow().last(), Some(&"restart"));
            assert_eq!(calls.borrow().contains(&"archive"), failed_step != "stop");
        }
        let error = quiesced_backup(
            || bail!("stop failed"),
            || Ok(()),
            || bail!("restart failed"),
        )
        .unwrap_err();
        let diagnostic = format!("{error:#}");
        assert!(diagnostic.contains("stop failed") && diagnostic.contains("restart failed"));
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
        assert!(environment(&BTreeMap::from([("TOKEN".into(), "one\ntwo".into())])).is_err());
    }
}
