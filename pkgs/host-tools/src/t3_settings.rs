use crate::util;
use anyhow::{Context, Result, bail, ensure};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Subcommand)]
pub enum Operation {
    /// Refresh the local portable bundle for chezmoi re-add.
    Capture {
        #[arg(long)]
        output: Option<PathBuf>,
        /// Skip capture if this machine has no T3 userdata (for the chezmoi hook).
        #[arg(long)]
        if_configured: bool,
    },
    /// Merge a portable bundle into T3, preserving credentials and device-local state.
    Apply {
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Reconcile live T3 settings with the portable bundle in either direction.
    ///
    /// When only one side changed since the last sync, that side wins. When
    /// both changed, sync does nothing and exits nonzero; resolve it with
    /// --force-apply (bundle wins) or --force-capture (live wins). Every
    /// overwrite and conflict is archived under
    /// ~/.local/state/agent-tools/t3-settings-history/ for recovery.
    Sync {
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
        /// Overwrite live settings with the bundle to resolve a conflict (bundle wins).
        #[arg(long, conflicts_with = "force_capture")]
        force_apply: bool,
        /// Overwrite the bundle with live settings to resolve a conflict (live wins).
        #[arg(long, conflicts_with = "force_apply")]
        force_capture: bool,
    },
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    version: u32,
    client: Value,
    server: Value,
    desktop: Value,
    providers: Map<String, Value>,
    keybindings: Value,
    projects: Vec<PortableProject>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PortableProject {
    workspace_root: String,
    attributes: Value,
    settings: Value,
}

struct Policy(Value);

#[derive(Default)]
struct ExcludedFields {
    unknown: BTreeSet<String>,
    ignored: BTreeSet<String>,
}

impl Policy {
    fn new() -> Self {
        Self(
            serde_json::from_str(include_str!("t3-settings-allowlist.json"))
                .expect("valid embedded T3 allowlist"),
        )
    }

    fn rule<'a>(&'a self, rule: &'a Value) -> &'a Value {
        if let Some(name) = rule.as_str() {
            return self.0["definitions"]
                .get(name.strip_prefix('$').expect("allowlist reference"))
                .expect("defined allowlist reference");
        }
        rule
    }

    fn select(
        &self,
        value: &Value,
        rule: &Value,
        path: &str,
        excluded: &mut ExcludedFields,
    ) -> Result<Value> {
        let rule = self.rule(rule);
        if value.is_null() {
            return Ok(Value::Null);
        }
        if rule == &Value::Bool(true) {
            ensure!(
                !value.is_object() && !value.is_array(),
                "Expected a scalar at {path}"
            );
            return Ok(value.clone());
        }
        if let Some(rules) = rule.as_array() {
            let values = value
                .as_array()
                .with_context(|| format!("Expected an array at {path}"))?;
            return Ok(Value::Array(
                values
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        self.select(value, &rules[0], &format!("{path}[{index}]"), excluded)
                    })
                    .collect::<Result<_>>()?,
            ));
        }
        let rules = rule
            .as_object()
            .context("Invalid embedded allowlist rule")?;
        let values = value
            .as_object()
            .with_context(|| format!("Expected an object at {path}"))?;
        let mut selected = Map::new();
        for (key, value) in values {
            let path = format!("{path}.{key}");
            match rules.get(key).or_else(|| rules.get("*")) {
                Some(Value::Bool(false)) => {
                    excluded.ignored.insert(path);
                }
                Some(rule) => {
                    selected.insert(key.clone(), self.select(value, rule, &path, excluded)?);
                }
                None => {
                    excluded.unknown.insert(path);
                }
            }
        }
        Ok(Value::Object(selected))
    }

    fn capture(&self, name: &str, value: &Value, path: &str) -> Result<Value> {
        let mut excluded = ExcludedFields::default();
        let selected = self.select(value, &self.0[name], path, &mut excluded)?;
        for field in excluded.unknown {
            eprintln!(
                "warning: Unknown field {field:?}; excluded. Update t3-settings-allowlist.json to allow it or mark it false to ignore it"
            );
        }
        Ok(selected)
    }

    fn capture_entries(&self, name: &str, server: &Value, key: &str) -> Result<Map<String, Value>> {
        let Some(entries) = server.get(key) else {
            return Ok(Map::new());
        };
        entries
            .as_object()
            .with_context(|| format!("Expected an object at server.{key}"))?
            .iter()
            .map(|(id, value)| {
                Ok((
                    id.clone(),
                    self.capture(name, value, &format!("server.{key}.{id}"))?,
                ))
            })
            .collect()
    }

    fn validate(&self, name: &str, value: &Value) -> Result<()> {
        ensure!(
            if name == "keybindings" {
                value.is_array()
            } else {
                value.is_object()
            },
            "Invalid {name} section shape"
        );
        let mut excluded = ExcludedFields::default();
        self.select(value, &self.0[name], name, &mut excluded)?;
        excluded.unknown.append(&mut excluded.ignored);
        let fields = excluded.unknown;
        ensure!(
            fields.is_empty(),
            "Bundle contains excluded fields: {fields:?}"
        );
        Ok(())
    }

    // Remove absent managed values too: T3 omits many preferences when reset to defaults.
    // Unknown object fields survive; lists are replaced as complete values.
    fn merge(&self, current: &mut Value, saved: Option<&Value>, rule: &Value) -> bool {
        let rule = self.rule(rule);
        if rule.is_object() && !current.is_object() && saved.is_some_and(Value::is_object) {
            *current = object();
        }
        if rule.is_object() && current.is_object() && saved.is_none_or(Value::is_object) {
            let rules = rule.as_object().unwrap();
            let empty = Map::new();
            let saved = saved.and_then(Value::as_object).unwrap_or(&empty);
            let current = current.as_object_mut().unwrap();
            let keys: BTreeSet<_> = current.keys().chain(saved.keys()).cloned().collect();
            for key in keys {
                if let Some(rule) = rules.get(&key).or_else(|| rules.get("*")) {
                    if rule == &Value::Bool(false) {
                        continue;
                    }
                    let value = current.entry(key.clone()).or_insert(Value::Null);
                    if !self.merge(value, saved.get(&key), rule) {
                        current.remove(&key);
                    }
                }
            }
            return !current.is_empty() || !saved.is_empty();
        }
        if let Some(saved) = saved {
            *current = saved.clone();
            return true;
        }
        false
    }

    fn merge_section(&self, name: &str, current: &mut Value, saved: &Value) {
        // A null provider config clears managed options, keeping local credentials.
        let local_config = if name == "provider" && saved.get("config").is_some_and(Value::is_null)
        {
            current.get_mut("config").map(Value::take)
        } else {
            None
        };
        self.merge(current, Some(saved), &self.0[name]);
        if let Some(mut config) = local_config {
            if self.merge(&mut config, None, &self.0["provider"]["config"]) {
                current["config"] = config;
            }
        }
    }
}

fn object() -> Value {
    json!({})
}

fn portable_path(path: &str, home: &Path) -> String {
    match Path::new(path).strip_prefix(home) {
        Ok(relative) => format!("~/{}", relative.display()),
        Err(_) => path.to_owned(),
    }
}

fn expand_path(path: &str, home: &Path) -> PathBuf {
    path.strip_prefix("~/")
        .map_or_else(|| PathBuf::from(path), |p| home.join(p))
}

fn map_paths(value: &mut Value, home: &Path, capture: bool) {
    match value {
        Value::Object(values) => {
            for (key, value) in values {
                if matches!(
                    key.as_str(),
                    "binaryPath"
                        | "homePath"
                        | "shadowHomePath"
                        | "faviconPath"
                        | "addProjectBaseDirectory"
                ) {
                    if let Some(path) = value.as_str() {
                        *value = Value::String(if capture {
                            portable_path(path, home)
                        } else {
                            expand_path(path, home).to_string_lossy().into_owned()
                        });
                    }
                } else {
                    map_paths(value, home, capture);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                map_paths(value, home, capture);
            }
        }
        _ => (),
    }
}

fn validate_provider(saved: &Value, local: Option<&Value>) -> Result<()> {
    let driver = saved
        .get("driver")
        .and_then(Value::as_str)
        .context("Provider requires a driver")?;
    if let Some(local) = local.and_then(|v| v.get("driver")).and_then(Value::as_str) {
        ensure!(
            driver == local,
            "A saved provider ID has a different local driver; resolve it before applying"
        );
    }
    if let Some(endpoint) = saved
        .pointer("/config/serverUrl")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        let url =
            url::Url::parse(endpoint).map_err(|_| anyhow::anyhow!("Invalid provider serverUrl"))?;
        ensure!(
            url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "Provider serverUrl contains credentials or query/fragment data; keep it device-local"
        );
    }
    Ok(())
}

struct SettingsFile {
    path: PathBuf,
    original: Option<Vec<u8>>,
    value: Value,
}

impl SettingsFile {
    fn read(path: PathBuf, default: Value) -> Result<Self> {
        let path = if path.is_symlink() {
            fs::canonicalize(&path).with_context(|| format!("Resolving {}", path.display()))?
        } else {
            path
        };
        let original = match fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error).with_context(|| format!("Reading {}", path.display())),
        };
        let value = match &original {
            Some(bytes) => serde_json::from_slice(bytes)
                .with_context(|| format!("Parsing {}", path.display()))?,
            None => default,
        };
        Ok(Self {
            path,
            original,
            value,
        })
    }

    fn verify_unchanged(&self) -> Result<()> {
        let latest = match fs::read(&self.path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        ensure!(
            latest == self.original,
            "{} changed during restore; retry after closing the settings editor",
            self.path.display()
        );
        Ok(())
    }

    fn write(self) -> Result<bool> {
        if let Some(bytes) = &self.original {
            if serde_json::from_slice::<Value>(bytes)? == self.value {
                return Ok(false);
            }
        }
        self.verify_unchanged()?;
        fs::create_dir_all(self.path.parent().context("Settings path needs a parent")?)?;
        util::atomic_write(&self.path, &json_bytes(&self.value)?)?;
        Ok(true)
    }
}

fn json_bytes(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

// Like T3's own project CLI, use a short-lived scoped local session and revoke it.
struct Session {
    base_dir: PathBuf,
    origin: String,
    id: String,
    authorization: String,
    agent: ureq::Agent,
}

fn cli(base_dir: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let program =
        PathBuf::from(std::env::var_os("HOME").context("HOME is unset")?).join(".local/bin/t3");
    let result = Command::new(program)
        .args(args)
        .arg("--base-dir")
        .arg(base_dir)
        .output()
        .context("Starting t3; bootstrap the native CLI first")?;
    ensure!(
        result.status.success(),
        "T3 authentication command failed ({})",
        result.status
    );
    Ok(result.stdout)
}

/// The version directory behind ~/.local/bin/t3, when it links into T3's runtime store.
fn native_cli_version(home: &Path, base_dir: &Path) -> Result<Option<String>> {
    let program = home.join(".local/bin/t3");
    let path = match fs::canonicalize(&program) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("resolve {}", program.display())),
    };
    let root = match fs::canonicalize(base_dir.join("runtime/versions")) {
        Ok(root) => root,
        Err(_) => return Ok(None),
    };
    Ok(path
        .strip_prefix(&root)
        .ok()
        .and_then(|relative| relative.components().next())
        .map(|version| version.as_os_str().to_string_lossy().into_owned()))
}

/// Ask a CLI outside T3's runtime store for the version it reports.
fn reported_cli_version(program: &Path) -> Option<String> {
    reported_cli_version_within(program, Duration::from_secs(5))
}

fn reported_cli_version_within(program: &Path, timeout: Duration) -> Option<String> {
    use std::os::unix::process::CommandExt;

    let mut child = Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = send.send(line);
    });
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => return None,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                kill_group(&mut child);
                return None;
            }
        }
    }
    let line = match receive.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(line) => line,
        Err(_) => {
            kill_group(&mut child);
            return None;
        }
    };
    line.trim().strip_prefix("t3 v").map(str::to_owned)
}

fn kill_group(child: &mut std::process::Child) {
    let _ = unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
    let _ = child.wait();
}

/// A CLI from another T3 build would migrate the server's database, so refuse it.
/// One whose version cannot be determined is reported but allowed, since it may
/// be a wrapper.
fn ensure_cli_matches_server(runtime: &Value, home: &Path, base_dir: &Path) -> Result<()> {
    let Some(server_version) = runtime["serverVersion"].as_str() else {
        eprintln!("t3-settings: T3 runtime has no server version; skipping the CLI version check");
        return Ok(());
    };
    let link = home.join(".local/bin/t3");
    let Some(cli_version) =
        native_cli_version(home, base_dir)?.or_else(|| reported_cli_version(&link))
    else {
        if link.exists() {
            eprintln!(
                "t3-settings: could not verify the T3 CLI at {} against server {server_version}; continuing",
                link.display()
            );
        }
        return Ok(());
    };
    ensure!(
        cli_version == server_version,
        "T3 CLI/server version mismatch: {} runs {cli_version}, but the server runs {server_version}; point it at {} or update T3",
        link.display(),
        base_dir
            .join("runtime/versions")
            .join(server_version)
            .join("t3")
            .display(),
    );
    Ok(())
}

/// Whether the runtime record names a process that is still alive.
///
/// T3 does not remove the file when it stops, so an old record can describe a
/// server that is gone.
fn runtime_is_live(runtime: &Value) -> bool {
    let Some(pid) = runtime["pid"].as_i64() else {
        return true;
    };
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    pid > 0
        && (unsafe { libc::kill(pid, 0) } == 0
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
}

fn read_server_runtime(path: &Path) -> Result<Option<Value>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(
            serde_json::from_slice(&bytes).context("Parsing T3 server runtime")?,
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).context("Reading T3 server runtime"),
    }
}

/// Wait for the running server to publish its endpoint and version.
pub(crate) fn server_runtime(base_dir: &Path) -> Result<Value> {
    let path = base_dir.join("userdata/server-runtime.json");
    util::wait_for_file(&path, std::time::Duration::from_secs(60), || {
        read_server_runtime(&path)
    })
}

/// Wait for a runtime record whose server process is still alive.
pub(crate) fn running_server_runtime(base_dir: &Path) -> Result<Value> {
    let path = base_dir.join("userdata/server-runtime.json");
    util::wait_for_file(&path, std::time::Duration::from_secs(60), || {
        Ok(read_server_runtime(&path)?.filter(runtime_is_live))
    })
}

pub(crate) fn resolve_base_dir(home: &Path, base_dir: Option<PathBuf>) -> PathBuf {
    base_dir
        .or_else(|| std::env::var_os("T3CODE_HOME").map(PathBuf::from))
        .unwrap_or_else(|| home.join(".t3"))
}

impl Session {
    fn open(home: &Path, base_dir: &Path, write: bool) -> Result<Self> {
        let runtime = server_runtime(base_dir)?;
        let origin = runtime["origin"]
            .as_str()
            .context("Start the T3 background server before capturing or restoring projects")?
            .to_owned();
        let url = url::Url::parse(&origin).context("Invalid T3 runtime origin")?;
        ensure!(
            url.scheme() == "http"
                && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")),
            "T3 runtime must expose a local HTTP endpoint"
        );
        ensure_cli_matches_server(&runtime, home, base_dir)?;
        let mut args = vec![
            "auth",
            "session",
            "issue",
            "--json",
            "--ttl",
            "5m",
            "--label",
            "dotfiles-settings",
            "--scope",
            "orchestration:read",
        ];
        if write {
            args.extend(["--scope", "orchestration:operate"]);
        }
        let issued: Value =
            serde_json::from_slice(&cli(base_dir, &args)?).context("Parsing scoped T3 session")?;
        Ok(Self {
            base_dir: base_dir.to_owned(),
            origin,
            id: issued["sessionId"]
                .as_str()
                .context("Missing session ID")?
                .to_owned(),
            authorization: format!(
                "Bearer {}",
                issued["token"].as_str().context("Missing session token")?
            ),
            agent: util::agent(15),
        })
    }

    fn request(&self, body: Option<&Value>) -> Result<Value> {
        let mut response = match body {
            Some(body) => self
                .agent
                .post(format!("{}/api/projects/mutate", self.origin))
                .header("authorization", &self.authorization)
                .send_json(body)?,
            None => self
                .agent
                .get(format!("{}/api/projects", self.origin))
                .header("authorization", &self.authorization)
                .call()?,
        };
        ensure!(
            response.status().is_success(),
            "T3 project API returned HTTP {}",
            response.status()
        );
        response
            .body_mut()
            .read_json()
            .context("Parsing T3 project API response")
    }

    fn projects(&self) -> Result<Vec<Value>> {
        match self.request(None)?["projects"].take() {
            Value::Array(projects) => Ok(projects),
            _ => anyhow::bail!("Project snapshot is missing its projects array"),
        }
    }

    fn mutate(&self, action: &str, id: &str, mut attributes: Value) -> Result<Value> {
        let body = attributes
            .as_object_mut()
            .context("Project attributes must be an object")?;
        body.insert("type".into(), json!(action));
        body.insert("projectId".into(), json!(id));
        body.insert("commandId".into(), json!(uuid()?));
        self.request(Some(&attributes))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Err(error) = cli(&self.base_dir, &["auth", "session", "revoke", &self.id]) {
            eprintln!(
                "Could not revoke the short-lived T3 session; it expires in five minutes: {error}"
            );
        }
    }
}

fn uuid() -> Result<String> {
    Ok(fs::read_to_string("/proc/sys/kernel/random/uuid")
        .context("Generating a Linux project/command UUID")?
        .trim()
        .to_owned())
}

fn warn_live_unknown_fields(
    policy: &Policy,
    client: &Value,
    server: &Value,
    desktop: &Value,
    keybindings: &Value,
) -> Result<()> {
    for (name, value) in [
        ("client", client),
        ("server", server),
        ("desktop", desktop),
        ("keybindings", keybindings),
    ] {
        policy.capture(name, value, name)?;
    }
    for (key, name) in [
        ("providerInstances", "provider"),
        ("projectSettingsOverrides", "projectSettings"),
    ] {
        policy.capture_entries(name, server, key)?;
    }
    Ok(())
}

// Drop values T3 supplies itself at decode time so the portable bundle only
// records deliberate changes. `Policy::merge` removes managed keys that are
// absent from the bundle, so applying a pruned bundle restores the default.
fn prune_defaults(value: &mut Value, defaults: &Value) {
    let (Some(values), Some(defaults)) = (value.as_object_mut(), defaults.as_object()) else {
        return;
    };
    values.retain(|key, value| {
        let Some(default) = defaults.get(key) else {
            return true;
        };
        if value == default {
            return false;
        }
        prune_defaults(value, default);
        !value.as_object().is_some_and(Map::is_empty)
    });
}

fn normalize_bundle_value(value: &mut Value, defaults: &crate::t3_defaults::Defaults) {
    if let Some(client) = value.get_mut("client") {
        prune_defaults(client, &defaults.client);
    }
    if let Some(server) = value.get_mut("server") {
        prune_defaults(server, &defaults.server);
    }
}

fn capture(policy: &Policy, home: &Path, base_dir: &Path) -> Result<Bundle> {
    let dir = base_dir.join("userdata");
    let mut client = policy.capture(
        "client",
        &SettingsFile::read(dir.join("client-settings.json"), object())?.value,
        "client",
    )?;
    let raw_server = SettingsFile::read(dir.join("settings.json"), object())?.value;
    let mut server = policy.capture("server", &raw_server, "server")?;
    let desktop = policy.capture(
        "desktop",
        &SettingsFile::read(dir.join("desktop-settings.json"), object())?.value,
        "desktop",
    )?;
    let keybindings = policy.capture(
        "keybindings",
        &SettingsFile::read(dir.join("keybindings.json"), json!([]))?.value,
        "keybindings",
    )?;
    if let Some(defaults) = crate::t3_defaults::load(home) {
        prune_defaults(&mut client, &defaults.client);
        prune_defaults(&mut server, &defaults.server);
    }
    let providers = policy.capture_entries("provider", &raw_server, "providerInstances")?;
    for provider in providers.values() {
        validate_provider(provider, None)?;
    }
    let mut overrides =
        policy.capture_entries("projectSettings", &raw_server, "projectSettingsOverrides")?;
    let session = Session::open(home, base_dir, false)?;
    let mut projects = Vec::new();
    for project in session.projects()? {
        if !project["deletedAt"].is_null() {
            continue;
        }
        let root = project["workspaceRoot"]
            .as_str()
            .context("Project requires a workspace root")?;
        if Path::new(root) == base_dir.join("scratch") {
            continue;
        }
        let id = project["id"].as_str().context("Project requires an ID")?;
        projects.push(PortableProject {
            workspace_root: portable_path(root, home),
            attributes: policy.capture("project", &project, &format!("projects.{id}"))?,
            settings: overrides.remove(id).unwrap_or_else(object),
        });
    }
    projects.sort_by(|a, b| a.workspace_root.cmp(&b.workspace_root));
    let mut bundle = Bundle {
        version: 1,
        client,
        server,
        desktop,
        providers,
        keybindings,
        projects,
    };
    transform_paths(&mut bundle, home, true);
    Ok(bundle)
}

fn transform_paths(bundle: &mut Bundle, home: &Path, capture: bool) {
    map_paths(&mut bundle.server, home, capture);
    for provider in bundle.providers.values_mut() {
        map_paths(provider, home, capture);
    }
    for project in &mut bundle.projects {
        map_paths(&mut project.attributes, home, capture);
    }
}

fn validate_bundle(policy: &Policy, bundle: &Bundle) -> Result<()> {
    ensure!(
        bundle.version == 1,
        "Unsupported portable T3 bundle version"
    );
    for (name, value) in [
        ("client", &bundle.client),
        ("server", &bundle.server),
        ("desktop", &bundle.desktop),
        ("keybindings", &bundle.keybindings),
    ] {
        policy.validate(name, value)?;
    }
    for provider in bundle.providers.values() {
        policy.validate("provider", provider)?;
        validate_provider(provider, None)?;
    }
    for binding in bundle.keybindings.as_array().unwrap() {
        ensure!(
            binding.get("key").and_then(Value::as_str).is_some()
                && binding.get("command").and_then(Value::as_str).is_some(),
            "Keybinding requires key and command strings"
        );
    }
    let mut roots = BTreeSet::new();
    for project in &bundle.projects {
        ensure!(
            roots.insert(&project.workspace_root),
            "Duplicate project workspace in portable bundle"
        );
        ensure!(
            project.attributes["title"]
                .as_str()
                .is_some_and(|t| !t.trim().is_empty()),
            "Project requires a title"
        );
        for (name, value) in [
            ("project", &project.attributes),
            ("projectSettings", &project.settings),
        ] {
            policy.validate(name, value)?;
        }
    }
    Ok(())
}

fn restore_projects<'a>(
    session: &Session,
    bundle: &'a Bundle,
    home: &Path,
    dry_run: bool,
) -> Result<Vec<(String, &'a PortableProject)>> {
    let mut local = BTreeMap::new();
    for project in session.projects()? {
        if !project["deletedAt"].is_null() {
            continue;
        }
        let root = PathBuf::from(
            project["workspaceRoot"]
                .as_str()
                .context("Project requires a workspace root")?,
        );
        ensure!(
            local.insert(root, project).is_none(),
            "Multiple active projects share a workspace root"
        );
    }
    let mut ids = Vec::new();
    for project in &bundle.projects {
        let root = expand_path(&project.workspace_root, home);
        ensure!(
            root.is_absolute(),
            "Project workspace must be absolute or start with ~/"
        );
        if !root.is_dir() {
            eprintln!(
                "Skipping {}: checkout is missing; clone it and apply again",
                root.display()
            );
            continue;
        }
        let current = match local.entry(root.clone()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(_) if dry_run => {
                println!("Would register {}", root.display());
                continue;
            }
            Entry::Vacant(entry) => {
                let id = uuid()?;
                let created = session.mutate(
                    "project.create",
                    &id,
                    json!({
                        "title": project.attributes["title"],
                        "workspaceRoot": root,
                        "createWorkspaceRootIfMissing": false,
                    }),
                )?;
                println!("Registered {}", root.display());
                entry.insert(created)
            }
        };
        let id = current["id"]
            .as_str()
            .context("Project requires an ID")?
            .to_owned();
        if !dry_run
            && project
                .attributes
                .as_object()
                .unwrap()
                .iter()
                .any(|(key, value)| current.get(key) != Some(value))
        {
            session.mutate("project.update", &id, project.attributes.clone())?;
        }
        ids.push((id, project));
    }
    Ok(ids)
}

fn settings_map<'a>(server: &'a mut Value, key: &str) -> Result<&'a mut Map<String, Value>> {
    server
        .as_object_mut()
        .context("Local server settings must be an object")?
        .entry(key.to_owned())
        .or_insert_with(object)
        .as_object_mut()
        .with_context(|| format!("Local {key} must be an object"))
}

fn restore_server(
    policy: &Policy,
    server: &mut Value,
    bundle: &Bundle,
    projects: &[(String, &PortableProject)],
) -> Result<()> {
    ensure!(
        server.is_object(),
        "Local server settings must be an object"
    );
    policy.merge_section("server", server, &bundle.server);
    if !bundle.providers.is_empty() {
        let local = settings_map(server, "providerInstances")?;
        for (id, saved) in &bundle.providers {
            validate_provider(saved, local.get(id))?;
            policy.merge_section(
                "provider",
                local.entry(id.clone()).or_insert_with(object),
                saved,
            );
        }
    }
    for (id, project) in projects {
        let overrides = settings_map(server, "projectSettingsOverrides")?;
        policy.merge_section(
            "projectSettings",
            overrides.entry(id.clone()).or_insert_with(object),
            &project.settings,
        );
    }
    Ok(())
}

fn configure_native_opencode(server: &mut Value, home: &Path) -> Result<()> {
    let Some(providers) = server
        .get_mut("providerInstances")
        .and_then(Value::as_object_mut)
    else {
        return Ok(());
    };
    let mut password = None;
    for provider in providers.values_mut() {
        if provider["driver"] != "opencode" || provider["enabled"] != true {
            continue;
        }
        let Some(server_url) = provider["config"]["serverUrl"].as_str() else {
            continue;
        };
        let url = url::Url::parse(server_url)?;
        if url.scheme() != "http"
            || !matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
            || url.path() != "/"
            || url.port_or_known_default() != Some(14096)
        {
            continue;
        }
        let password = match password {
            Some(ref password) => password,
            None => password.insert(crate::opencode::wait_password(
                &home.join(".config/opencode/service.json"),
            )?),
        };
        provider["config"]
            .as_object_mut()
            .context("OpenCode config requires an object")?
            .insert("serverPassword".into(), json!(password));
    }
    Ok(())
}

/// Lenient twin of the `configure_native_opencode` match: a background
/// reconciliation must skip odd providers, never fail the whole sync on them.
fn uses_native_opencode(provider: &Value) -> bool {
    if provider["driver"] != "opencode" || provider["enabled"] != true {
        return false;
    }
    let Some(server_url) = provider["config"]["serverUrl"].as_str() else {
        return false;
    };
    let Ok(url) = url::Url::parse(server_url) else {
        return false;
    };
    url.scheme() == "http"
        && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
        && url.path() == "/"
        && url.port_or_known_default() == Some(14096)
}

/// Read-only check for `ensure_native_password`, also used to preview the
/// refresh on dry runs that otherwise report no action.
fn native_password_stale(home: &Path, base_dir: &Path) -> Result<bool> {
    let server = SettingsFile::read(base_dir.join("userdata/settings.json"), object())?;
    let Some(password) = crate::opencode::password(&home.join(".config/opencode/service.json"))?
    else {
        return Ok(false);
    };
    Ok(server
        .value
        .get("providerInstances")
        .and_then(Value::as_object)
        .is_some_and(|providers| {
            providers
                .values()
                .filter(|p| uses_native_opencode(p))
                .any(|p| {
                    p["config"].get("serverPassword").and_then(Value::as_str) != Some(&password)
                })
        }))
}

/// Re-inject the native OpenCode password when only it changed. The password
/// is excluded from sync comparisons, so an otherwise-in-sync run would never
/// notice a rotation. Non-blocking: a missing service file simply skips.
fn ensure_native_password(home: &Path, base_dir: &Path) -> Result<bool> {
    if !native_password_stale(home, base_dir)? {
        return Ok(false);
    }
    let mut server = SettingsFile::read(base_dir.join("userdata/settings.json"), object())?;
    let Some(password) = crate::opencode::password(&home.join(".config/opencode/service.json"))?
    else {
        return Ok(false);
    };
    let Some(providers) = server
        .value
        .get_mut("providerInstances")
        .and_then(Value::as_object_mut)
    else {
        return Ok(false);
    };
    let mut changed = false;
    for provider in providers.values_mut() {
        if !uses_native_opencode(provider) {
            continue;
        }
        let config = provider["config"]
            .as_object_mut()
            .context("OpenCode config requires an object")?;
        if config.get("serverPassword").and_then(Value::as_str) != Some(&password) {
            config.insert("serverPassword".into(), json!(password));
            changed = true;
        }
    }
    if changed {
        server.write()?;
    }
    Ok(changed)
}

const SECTIONS: [&str; 4] = ["client", "desktop", "keybindings", "server"];

fn apply(
    policy: &Policy,
    mut bundle: Bundle,
    home: &Path,
    base_dir: &Path,
    dry_run: bool,
    record: bool,
) -> Result<()> {
    validate_bundle(policy, &bundle)?;
    transform_paths(&mut bundle, home, false);
    let dir = base_dir.join("userdata");
    let mut client = SettingsFile::read(dir.join("client-settings.json"), object())?;
    let mut desktop = SettingsFile::read(dir.join("desktop-settings.json"), object())?;
    let mut keybindings = SettingsFile::read(dir.join("keybindings.json"), json!([]))?;
    let mut server = SettingsFile::read(dir.join("settings.json"), object())?;
    warn_live_unknown_fields(
        policy,
        &client.value,
        &server.value,
        &desktop.value,
        &keybindings.value,
    )?;
    policy.merge_section("client", &mut client.value, &bundle.client);
    policy.merge_section("desktop", &mut desktop.value, &bundle.desktop);
    keybindings.value = bundle.keybindings.clone();
    // Check driver conflicts before project mutations, then recheck the latest settings.
    for (id, provider) in &bundle.providers {
        validate_provider(provider, server.value["providerInstances"].get(id))?;
    }
    let session = Session::open(home, base_dir, !dry_run)?;
    let ids = restore_projects(&session, &bundle, home, dry_run)?;
    server = SettingsFile::read(dir.join("settings.json"), object())?;
    restore_server(policy, &mut server.value, &bundle, &ids)?;
    configure_native_opencode(&mut server.value, home)?;
    if dry_run {
        println!(
            "Validated portable settings and {} existing projects; no settings or projects changed",
            ids.len()
        );
        return Ok(());
    }
    let files = [client, desktop, keybindings, server];
    // Project mutations above take seconds; recheck every file before writing
    // any of them so a concurrent editor cannot cause a partial restore.
    for file in &files {
        file.verify_unchanged()?;
    }
    let snapshots = original_snapshots(&files.iter().collect::<Vec<_>>());
    let mut changed = 0;
    for file in files {
        changed += usize::from(file.write()?);
    }
    if record && changed > 0 {
        let mut before_all = Map::new();
        let mut after_all = Map::new();
        for ((before, after), section) in snapshots.into_iter().zip(SECTIONS) {
            before_all.insert(section.to_owned(), before);
            after_all.insert(section.to_owned(), after);
        }
        let before = Value::Object(before_all);
        let after = Value::Object(after_all);
        record_history(
            home,
            "apply",
            false,
            diff_summary(&before, &after),
            &before,
            &after,
        )?;
    }
    println!(
        "Merged portable settings into {changed} files and restored {} projects. Restart the desktop to reload client preferences",
        ids.len()
    );
    Ok(())
}

fn original_snapshots(files: &[&SettingsFile]) -> Vec<(Value, Value)> {
    files
        .iter()
        .map(|file| {
            let before = file
                .original
                .as_ref()
                .and_then(|bytes| serde_json::from_slice(bytes).ok())
                .unwrap_or(Value::Null);
            (before, file.value.clone())
        })
        .collect()
}

const SYNC_STATE_VERSION: u32 = 1;
const HISTORY_KEEP: usize = 30;
const MAX_DIFF_LINES: usize = 120;
const DIFF_VALUE_LIMIT: usize = 160;

/// Last reconciled content of both sides, used to detect who changed.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SyncState {
    version: u32,
    bundle: Value,
    live: Option<Value>,
}

#[derive(Deserialize, Serialize)]
struct HistoryEntry {
    timestamp: String,
    direction: String,
    forced: bool,
    diff: Vec<String>,
    before: Value,
    after: Value,
}

#[derive(Debug, PartialEq, Eq)]
enum Decision {
    InSync,
    CaptureLive,
    ApplyBundle,
    Provision,
    /// No sync record yet, but live settings exist and differ from the bundle.
    /// Without a common ancestor this is adoption (fresh-machine onboarding),
    /// not a conflict: the live side is backed up to history, then the bundle
    /// is applied.
    AdoptBundle,
    Conflict,
}

fn decide(load: &StateLoad, live: Option<&Value>, bundle: &Value) -> Decision {
    let Some(live) = live else {
        return Decision::Provision;
    };
    // Agreeing sides are in sync even against a stale record: manual capture
    // and apply rewrite one side without recording sync state.
    if live == bundle {
        return Decision::InSync;
    }
    match load {
        // No common ancestor exists to call this a conflict: the live side
        // is backed up to history, then the bundle is applied.
        StateLoad::Absent => Decision::AdoptBundle,
        // Without a usable record neither side can be proven unchanged, so
        // stop instead of picking a winner.
        StateLoad::Unusable => Decision::Conflict,
        StateLoad::Present(state) => {
            let live_changed = state.live.as_ref() != Some(live);
            let bundle_changed = &state.bundle != bundle;
            match (live_changed, bundle_changed) {
                (false, false) => Decision::InSync,
                (true, false) => Decision::CaptureLive,
                (false, true) => Decision::ApplyBundle,
                (true, true) => Decision::Conflict,
            }
        }
    }
}

fn sync_state_path(home: &Path) -> PathBuf {
    crate::agent_tools::state_dir(home).join("t3-settings-sync.json")
}

fn history_dir(home: &Path) -> PathBuf {
    crate::agent_tools::state_dir(home).join("t3-settings-history")
}

/// How the last reconciliation record loaded. Only a missing record means
/// fresh-machine onboarding; a corrupt record or one from a newer version
/// must stop like a conflict instead of silently adopting the bundle over
/// live customizations.
enum StateLoad {
    Absent,
    Unusable,
    Present(SyncState),
}

fn load_state(home: &Path) -> StateLoad {
    let path = sync_state_path(home);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return StateLoad::Absent,
        Err(error) => {
            eprintln!(
                "warning: cannot read T3 sync state at {}: {error}",
                path.display()
            );
            return StateLoad::Unusable;
        }
    };
    match serde_json::from_slice::<SyncState>(&bytes) {
        Ok(state) if state.version == SYNC_STATE_VERSION => StateLoad::Present(state),
        Ok(_) => {
            eprintln!("warning: ignoring T3 sync state with an unsupported version");
            StateLoad::Unusable
        }
        Err(error) => {
            eprintln!("warning: ignoring corrupt T3 sync state: {error}");
            StateLoad::Unusable
        }
    }
}

fn save_state(home: &Path, state: &SyncState) -> Result<()> {
    let path = sync_state_path(home);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    util::atomic_write(&path, &json_bytes(state)?)?;
    Ok(())
}

fn record_history(
    home: &Path,
    direction: &str,
    forced: bool,
    diff: Vec<String>,
    before: &Value,
    after: &Value,
) -> Result<()> {
    let dir = history_dir(home);
    fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    }
    let entry = HistoryEntry {
        timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        direction: direction.to_owned(),
        forced,
        diff,
        before: before.clone(),
        after: after.clone(),
    };
    let name = format!(
        "{}-{}.json",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%.9fZ"),
        direction
    );
    util::atomic_write(&dir.join(&name), &json_bytes(&entry)?)?;
    let mut names: Vec<_> = fs::read_dir(&dir)?
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(".json"))
        .collect();
    names.sort();
    while names.len() > HISTORY_KEEP {
        let _ = fs::remove_file(dir.join(names.remove(0)));
    }
    Ok(())
}

/// Newest history entry, so an unchanged conflict stops re-archiving itself
/// on every trigger and evicting real recovery snapshots from retention.
fn last_history_entry(home: &Path) -> Option<HistoryEntry> {
    let dir = history_dir(home);
    let latest = fs::read_dir(&dir)
        .ok()?
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(".json"))
        .max()?;
    serde_json::from_slice(&fs::read(dir.join(latest)).ok()?).ok()
}

fn describe(value: &Value) -> String {
    let text = serde_json::to_string(value).unwrap_or_else(|_| "?".into());
    if text.chars().count() > DIFF_VALUE_LIMIT {
        let truncated: String = text.chars().take(DIFF_VALUE_LIMIT).collect();
        format!("{truncated}…")
    } else {
        text
    }
}

fn diff_values(old: &Value, new: &Value, path: &str, out: &mut Vec<String>) {
    if old == new || out.len() >= MAX_DIFF_LINES {
        return;
    }
    match (old, new) {
        (Value::Object(old), Value::Object(new)) => {
            let mut keys: BTreeSet<&String> = BTreeSet::new();
            keys.extend(old.keys());
            keys.extend(new.keys());
            for key in keys {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                match (old.get(key), new.get(key)) {
                    (Some(o), Some(n)) => diff_values(o, n, &child, out),
                    (Some(o), None) => {
                        out.push(format!("{child}: {} → (absent)", describe(o)));
                    }
                    (None, Some(n)) => {
                        out.push(format!("{child}: (absent) → {}", describe(n)));
                    }
                    (None, None) => unreachable!(),
                }
                if out.len() >= MAX_DIFF_LINES {
                    return;
                }
            }
        }
        (Value::Array(old), Value::Array(new)) => {
            if old.len() != new.len() {
                out.push(format!(
                    "{path}: [{} items → {} items]",
                    old.len(),
                    new.len()
                ));
            }
            for (index, (o, n)) in old.iter().zip(new.iter()).enumerate() {
                diff_values(o, n, &format!("{path}[{index}]"), out);
                if out.len() >= MAX_DIFF_LINES {
                    return;
                }
            }
            for (index, item) in new.iter().enumerate().skip(old.len()) {
                out.push(format!("{path}[{index}]: (absent) → {}", describe(item)));
            }
            for (index, item) in old.iter().enumerate().skip(new.len()) {
                out.push(format!("{path}[{index}]: {} → (absent)", describe(item)));
            }
        }
        _ => out.push(format!("{path}: {} → {}", describe(old), describe(new))),
    }
}

fn diff_summary(old: &Value, new: &Value) -> Vec<String> {
    let mut out = Vec::new();
    diff_values(old, new, "", &mut out);
    if out.len() >= MAX_DIFF_LINES {
        out.push(format!("… (truncated at {MAX_DIFF_LINES} lines)"));
    }
    out
}

fn print_diff(diff: &[String]) {
    if diff.is_empty() {
        println!("  (no content changes)");
    }
    for line in diff {
        println!("  {line}");
    }
}

fn read_bundle_file(path: &Path) -> Result<Option<Bundle>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(
            serde_json::from_slice(&bytes)
                .with_context(|| format!("Parsing {}", path.display()))?,
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("Reading {}", path.display())),
    }
}

fn bundle_value(bundle: &Bundle) -> Result<Value> {
    serde_json::to_value(bundle).context("Serializing portable bundle")
}

fn write_bundle_file(path: &Path, base_dir: &Path, bundle: &Bundle) -> Result<bool> {
    let output = capture_destination(path, base_dir)?;
    let bytes = json_bytes(bundle)?;
    match fs::read(&output) {
        Ok(current) if current == bytes => Ok(false),
        Ok(_) | Err(_) => {
            util::atomic_write(&output, &bytes)?;
            Ok(true)
        }
    }
}

fn sync_apply_and_snapshot(
    policy: &Policy,
    bundle: Bundle,
    bundle_val: Value,
    live_val: Option<Value>,
    home: &Path,
    base_dir: &Path,
    forced: bool,
    direction: &str,
) -> Result<()> {
    let before = live_val.clone().unwrap_or_else(|| json!({}));
    apply(policy, bundle, home, base_dir, false, false)?;
    // The common operation only needs the post-apply snapshot and the diff
    // against it; a failed re-read falls back to the intended bundle values
    // with unknown live content.
    let live = match capture(policy, home, base_dir) {
        Ok(fresh) => Some(bundle_value(&fresh)?),
        Err(error) => {
            eprintln!("warning: applied the bundle but could not re-read live settings: {error:#}");
            None
        }
    };
    let after = live.as_ref().unwrap_or(&bundle_val);
    let diff = diff_summary(&before, after);
    // A no-op forced sync still refreshes state, but an empty diff is
    // not worth a history slot.
    if !diff.is_empty() {
        print_diff(&diff);
        record_history(home, direction, forced, diff, &before, &after)?;
    }
    save_state(
        home,
        &SyncState {
            version: SYNC_STATE_VERSION,
            bundle: bundle_val,
            live,
        },
    )?;
    println!("t3-settings sync: applied the portable bundle to live T3 settings");
    Ok(())
}

fn sync(
    policy: &Policy,
    home: &Path,
    base_dir: &Path,
    input: Option<PathBuf>,
    dry_run: bool,
    force_apply: bool,
    force_capture: bool,
) -> Result<()> {
    let bundle_path = input.unwrap_or_else(|| home.join(".config/t3code/portable-settings.json"));
    let Some(mut bundle) = read_bundle_file(&bundle_path)? else {
        println!(
            "t3-settings sync: no portable bundle at {}; apply editable dotfiles to restore T3 preferences",
            bundle_path.display()
        );
        return Ok(());
    };
    validate_bundle(policy, &bundle)?;
    // Older bundles and sync records predate default pruning. Normalizing both
    // sides before comparing keeps that representation change from looking like
    // an edit on every machine that receives a pruned bundle.
    let defaults = if base_dir.join("userdata").try_exists()? {
        crate::t3_defaults::load(home)
    } else {
        None
    };
    if let Some(defaults) = &defaults {
        prune_defaults(&mut bundle.client, &defaults.client);
        prune_defaults(&mut bundle.server, &defaults.server);
    }
    let bundle_val = bundle_value(&bundle)?;
    let live_bundle = if base_dir.join("userdata").try_exists()? {
        Some(capture(policy, home, base_dir)?)
    } else {
        None
    };
    let live_val = live_bundle.as_ref().map(bundle_value).transpose()?;
    let mut load = load_state(home);
    let raw_state = match &load {
        StateLoad::Present(state) => Some((state.bundle.clone(), state.live.clone())),
        _ => None,
    };
    if let Some(defaults) = &defaults
        && let StateLoad::Present(state) = &mut load
    {
        normalize_bundle_value(&mut state.bundle, defaults);
        if let Some(live) = state.live.as_mut() {
            normalize_bundle_value(live, defaults);
        }
    }
    let state_dirty = match (&raw_state, &load) {
        (Some((bundle, live)), StateLoad::Present(state)) => {
            state.bundle != *bundle || state.live != *live
        }
        _ => false,
    };
    let mut decision = decide(&load, live_val.as_ref(), &bundle_val);
    // The force flags resolve a conflict, not a decided direction: a flag
    // aimed at the side that did not change would silently destroy the side
    // that did, so outside a conflict the natural decision stands.
    if decision == Decision::Conflict {
        if force_capture {
            ensure!(
                live_bundle.is_some(),
                "t3-settings sync --force-capture needs live T3 userdata"
            );
            decision = Decision::CaptureLive;
        } else if force_apply {
            decision = if live_bundle.is_none() {
                Decision::Provision
            } else {
                Decision::ApplyBundle
            };
        }
    } else if force_capture || force_apply {
        let flag = if force_capture {
            "--force-capture"
        } else {
            "--force-apply"
        };
        println!(
            "t3-settings sync: ignoring {flag} outside a conflict; taking the decided direction"
        );
    }
    // Adoption and plain applies push the bundle down; adoption only adds a
    // backup notice since there is no previous sync record to compare against.
    let adopting = matches!(decision, Decision::AdoptBundle);
    match decision {
        Decision::InSync => {
            if dry_run {
                println!("t3-settings sync: live settings and portable bundle match (dry run)");
                if native_password_stale(home, base_dir)? {
                    println!(
                        "t3-settings sync: would refresh the native OpenCode service password in live settings (dry run)"
                    );
                }
                return Ok(());
            }
            // The service credential is excluded from sync comparisons, so a
            // rotation would otherwise go unnoticed on this path. The refresh
            // leaves an audit entry: portable snapshots cannot show a
            // password change, and raw snapshots must not archive secrets.
            if ensure_native_password(home, base_dir)? {
                println!(
                    "t3-settings sync: refreshed the native OpenCode service password in live settings"
                );
                let snapshot = live_val
                    .as_ref()
                    .context("t3-settings sync: live T3 settings vanished")?;
                record_history(
                    home,
                    "password",
                    false,
                    vec![
                        "refreshed the native OpenCode service password in live settings"
                            .to_owned(),
                    ],
                    snapshot,
                    snapshot,
                )?;
            }
            // Drop defaults that older bundles still spell out even when no
            // setting changed, so every machine converges on the pruned form.
            if write_bundle_file(&bundle_path, base_dir, &bundle)? {
                println!("t3-settings sync: normalized the portable bundle");
            }
            // Refresh stale records (manual capture/apply converge the sides
            // without recording state) so the next change compares correctly.
            let stale = state_dirty
                || match &load {
                    StateLoad::Present(state) => {
                        state.bundle != bundle_val || state.live != live_val
                    }
                    _ => true,
                };
            if stale {
                save_state(
                    home,
                    &SyncState {
                        version: SYNC_STATE_VERSION,
                        bundle: bundle_val,
                        live: live_val,
                    },
                )?;
                println!(
                    "t3-settings sync: live settings and portable bundle match; recorded sync state"
                );
            } else {
                println!("t3-settings sync: already in sync; nothing to do");
            }
            Ok(())
        }
        Decision::Provision => {
            if dry_run {
                println!(
                    "t3-settings sync: would apply the portable bundle to fresh T3 userdata (dry run)"
                );
                return Ok(());
            }
            println!(
                "t3-settings sync: no live T3 settings; provisioning from the portable bundle"
            );
            sync_apply_and_snapshot(
                policy,
                bundle,
                bundle_val,
                None,
                home,
                base_dir,
                force_apply,
                "apply",
            )
        }
        Decision::CaptureLive => {
            let live = live_bundle.context("t3-settings sync: live T3 settings vanished")?;
            let live_v = live_val.context("t3-settings sync: live T3 settings vanished")?;
            let diff = diff_summary(&bundle_val, &live_v);
            if dry_run {
                println!(
                    "t3-settings sync: would capture live settings into the portable bundle (dry run)"
                );
                print_diff(&diff);
                return Ok(());
            }
            if write_bundle_file(&bundle_path, base_dir, &live)? {
                print_diff(&diff);
                record_history(home, "capture", force_capture, diff, &bundle_val, &live_v)?;
            }
            save_state(
                home,
                &SyncState {
                    version: SYNC_STATE_VERSION,
                    bundle: live_v.clone(),
                    live: Some(live_v),
                },
            )?;
            println!(
                "t3-settings sync: captured live T3 settings into {}",
                bundle_path.display()
            );
            Ok(())
        }
        Decision::ApplyBundle | Decision::AdoptBundle => {
            let live_v = live_val.context("t3-settings sync: live T3 settings vanished")?;
            let diff = diff_summary(&live_v, &bundle_val);
            let (direction, target) = if adopting {
                ("adopt", "over unrecorded live settings")
            } else {
                ("apply", "to live T3 settings")
            };
            if adopting {
                println!(
                    "t3-settings sync: no previous sync record; backing up live settings to history, then adopting the portable bundle"
                );
            }
            if dry_run {
                println!(
                    "t3-settings sync: would {direction} the portable bundle {target} (dry run)"
                );
                print_diff(&diff);
                return Ok(());
            }
            if adopting {
                print_diff(&diff);
            }
            sync_apply_and_snapshot(
                policy,
                bundle,
                bundle_val,
                Some(live_v),
                home,
                base_dir,
                force_apply,
                direction,
            )
        }
        Decision::Conflict => {
            let live_v = live_val.context("t3-settings sync: live T3 settings vanished")?;
            // decide() only reports a conflict against a previous sync record
            // or an unusable one; without a record there is no baseline, so
            // the full sides render instead of changes.
            let (live_base, bundle_base) = match &load {
                StateLoad::Present(state) => (state.live.as_ref(), Some(&state.bundle)),
                _ => {
                    eprintln!(
                        "warning: T3 sync record is missing or unusable; stopping instead of picking a winner"
                    );
                    (None, None)
                }
            };
            let mut combined = vec!["live changes since last sync:".to_owned()];
            combined.extend(diff_summary(live_base.unwrap_or(&Value::Null), &live_v));
            combined.push("bundle changes since last sync:".to_owned());
            combined.extend(diff_summary(
                bundle_base.unwrap_or(&Value::Null),
                &bundle_val,
            ));
            if dry_run {
                println!("t3-settings sync: would stop on conflicting changes (dry run)");
                print_diff(&combined);
                return Ok(());
            }
            eprintln!(
                "t3-settings sync: CONFLICT — live settings and the portable bundle both changed; doing nothing"
            );
            print_diff(&combined);
            let duplicate = last_history_entry(home)
                .is_some_and(|entry| entry.direction == "conflict" && entry.diff == combined);
            if !duplicate {
                record_history(home, "conflict", false, combined, &bundle_val, &live_v)?;
            }
            bail!(
                "resolve with `t3-settings sync --force-capture` (live settings win) or `t3-settings sync --force-apply` (bundle wins); details archived under {}",
                history_dir(home).display()
            );
        }
    }
}

fn capture_destination(output: &Path, base_dir: &Path) -> Result<PathBuf> {
    let output = std::path::absolute(output)?;
    let parent = output
        .parent()
        .context("Capture output needs a parent directory")?;
    fs::create_dir_all(parent)?;
    let output = fs::canonicalize(parent)?.join(
        output
            .file_name()
            .context("Capture output needs a filename")?,
    );
    ensure!(
        !output.starts_with(fs::canonicalize(base_dir.join("userdata"))?),
        "Capture output must be outside live T3 userdata"
    );
    Ok(output)
}

pub fn run(base_dir: Option<PathBuf>, operation: Operation) -> Result<()> {
    let home = PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?);
    let base_dir = resolve_base_dir(&home, base_dir);
    let default_bundle = home.join(".config/t3code/portable-settings.json");
    let policy = Policy::new();
    match operation {
        Operation::Capture {
            output,
            if_configured,
        } => {
            if if_configured && !base_dir.join("userdata").try_exists()? {
                eprintln!(
                    "t3-settings: skipping capture; no T3 userdata at {}",
                    base_dir.display()
                );
                return Ok(());
            }
            let output = output.unwrap_or(default_bundle);
            let output = capture_destination(&output, &base_dir)?;
            let bundle = capture(&policy, &home, &base_dir)?;
            validate_bundle(&policy, &bundle)?;
            let new_val = bundle_value(&bundle)?;
            let before: Value = read_bundle_file(&output)
                .ok()
                .flatten()
                .and_then(|old| bundle_value(&old).ok())
                .unwrap_or(Value::Null);
            if write_bundle_file(&output, &base_dir, &bundle)? {
                record_history(
                    &home,
                    "capture",
                    false,
                    diff_summary(&before, &new_val),
                    &before,
                    &new_val,
                )?;
                println!(
                    "Captured allowed settings and {} projects into {}",
                    bundle.projects.len(),
                    output.display()
                );
            } else {
                println!("Portable bundle already matches live settings; nothing to do");
            }
        }
        Operation::Apply { input, dry_run } => {
            let input = input.unwrap_or(default_bundle);
            let bundle = serde_json::from_slice(
                &fs::read(&input).with_context(|| format!("Reading {}", input.display()))?,
            )
            .context("Parsing portable T3 bundle")?;
            apply(&policy, bundle, &home, &base_dir, dry_run, true)?;
        }
        Operation::Sync {
            input,
            dry_run,
            force_apply,
            force_capture,
        } => {
            sync(
                &policy,
                &home,
                &base_dir,
                input,
                dry_run,
                force_apply,
                force_capture,
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t3_cli_must_match_the_running_server_version() -> Result<()> {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let root = tempfile::tempdir()?;
        let home = root.path().join("home");
        let base_dir = home.join(".t3");
        let binary = base_dir.join("runtime/versions/0.1.3-nightly.1/t3");
        fs::create_dir_all(binary.parent().unwrap())?;
        fs::write(&binary, "")?;
        fs::create_dir_all(home.join(".local/bin"))?;
        let link = home.join(".local/bin/t3");
        symlink(&binary, &link)?;

        ensure_cli_matches_server(
            &json!({"serverVersion": "0.1.3-nightly.1"}),
            &home,
            &base_dir,
        )?;
        let error = ensure_cli_matches_server(
            &json!({"serverVersion": "0.1.4-nightly.2"}),
            &home,
            &base_dir,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("0.1.3-nightly.1"), "{error}");
        assert!(error.contains("0.1.4-nightly.2"), "{error}");
        assert!(error.contains(&link.display().to_string()), "{error}");
        assert!(
            error.contains(
                &base_dir
                    .join("runtime/versions/0.1.4-nightly.2/t3")
                    .display()
                    .to_string()
            ),
            "{error}"
        );

        let linked_base = home.join("linked-t3");
        symlink(&base_dir, &linked_base)?;
        ensure_cli_matches_server(
            &json!({"serverVersion": "0.1.3-nightly.1"}),
            &home,
            &linked_base,
        )?;

        fs::remove_file(&link)?;
        let unmanaged = home.join("unmanaged-t3");
        fs::write(&unmanaged, "#!/bin/sh\necho 't3 v0.1.4-nightly.2'\n")?;
        fs::set_permissions(&unmanaged, fs::Permissions::from_mode(0o755))?;
        symlink(&unmanaged, &link)?;
        ensure_cli_matches_server(
            &json!({"serverVersion": "0.1.4-nightly.2"}),
            &home,
            &base_dir,
        )?;
        let error = ensure_cli_matches_server(
            &json!({"serverVersion": "0.1.3-nightly.1"}),
            &home,
            &base_dir,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("0.1.4-nightly.2"), "{error}");
        assert!(error.contains("0.1.3-nightly.1"), "{error}");

        fs::remove_file(&link)?;
        let broken = home.join("broken-t3");
        fs::write(&broken, "#!/bin/sh\nexit 1\n")?;
        fs::set_permissions(&broken, fs::Permissions::from_mode(0o755))?;
        symlink(&broken, &link)?;
        ensure_cli_matches_server(
            &json!({"serverVersion": "0.1.3-nightly.1"}),
            &home,
            &base_dir,
        )?;
        Ok(())
    }

    #[test]
    fn a_hanging_cli_version_probe_gives_up() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let script = root.path().join("t3");
        fs::write(&script, "#!/bin/sh\nsleep 30\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let started = Instant::now();
        assert_eq!(
            reported_cli_version_within(&script, Duration::from_millis(100)),
            None
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn a_descendant_holding_stdout_does_not_stall_the_probe() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let script = root.path().join("t3");
        fs::write(&script, "#!/bin/sh\nsleep 2 &\necho 't3 v9.9.9'\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            reported_cli_version_within(&script, Duration::from_secs(1)),
            Some("9.9.9".to_string())
        );
    }

    #[test]
    fn a_stale_server_record_is_not_running() {
        assert!(runtime_is_live(&json!({})));
        assert!(runtime_is_live(&json!({"pid": std::process::id()})));
        assert!(!runtime_is_live(&json!({"pid": 0})));
        assert!(!runtime_is_live(&json!({"pid": i64::MAX})));
    }

    #[test]
    fn native_credentials_only_replace_matching_local_enabled_opencode() -> Result<()> {
        let root = tempfile::tempdir()?;
        let config = root.path().join(".config/opencode");
        fs::create_dir_all(&config)?;
        fs::write(
            config.join("service.json"),
            r#"{"password":"generated-private"}"#,
        )?;
        let mut settings = json!({"providerInstances": {
            "native": {"driver":"opencode","enabled":true,"config":{"serverUrl":"http://127.0.0.1:14096","serverPassword":"stale","other":"keep"}},
            "remote": {"driver":"opencode","enabled":true,"config":{"serverUrl":"http://example.test:14096","serverPassword":"remote"}},
            "other-port": {"driver":"opencode","enabled":true,"config":{"serverUrl":"http://localhost:17000","serverPassword":"other"}},
            "disabled": {"driver":"opencode","enabled":false,"config":{"serverUrl":"http://localhost:14096","serverPassword":"disabled"}}
        }});
        configure_native_opencode(&mut settings, root.path())?;
        let providers = &settings["providerInstances"];
        assert_eq!(
            providers["native"]["config"]["serverPassword"],
            "generated-private"
        );
        assert_eq!(providers["native"]["config"]["other"], "keep");
        for (id, password) in [
            ("remote", "remote"),
            ("other-port", "other"),
            ("disabled", "disabled"),
        ] {
            assert_eq!(providers[id]["config"]["serverPassword"], password);
        }
        let policy = Policy::new();
        let portable = policy.capture_entries("provider", &settings, "providerInstances")?;
        assert!(
            portable
                .values()
                .all(|provider| provider["config"].get("serverPassword").is_none())
        );
        settings["providerInstances"]
            .as_object_mut()
            .unwrap()
            .remove("native");
        fs::remove_file(config.join("service.json"))?;
        configure_native_opencode(&mut settings, root.path())?;
        Ok(())
    }

    #[test]
    fn capture_rejects_malformed_server_records_without_printing_values() {
        let policy = Policy::new();
        for (key, name) in [
            ("providerInstances", "provider"),
            ("projectSettingsOverrides", "projectSettings"),
        ] {
            assert!(
                policy
                    .capture_entries(name, &object(), key)
                    .unwrap()
                    .is_empty()
            );
            for value in [Value::Null, json!([]), json!(false), json!("private-value")] {
                let error = policy
                    .capture_entries(name, &json!({key: value}), key)
                    .unwrap_err()
                    .to_string();
                assert_eq!(error, format!("Expected an object at server.{key}"));
            }
        }
    }

    #[test]
    fn allowlist_excludes_nested_credentials_and_future_fields() {
        let policy = Policy::new();
        let source = json!({"driver":"opencode", "enabled":false, "environment":[{"name":"KEY", "value":"secret"}], "config":{"serverUrl":"http://localhost:14096", "serverPassword":"secret", "futureCredential":"secret"}});
        let mut excluded = ExcludedFields::default();
        let selected = policy
            .select(
                &source,
                &policy.0["provider"],
                "providers.opencode",
                &mut excluded,
            )
            .unwrap();
        assert_eq!(
            selected,
            json!({"driver":"opencode", "enabled":false, "config":{"serverUrl":"http://localhost:14096"}})
        );
        assert!(policy.validate("provider", &source).is_err());
        assert!(validate_provider(
            &json!({"driver":"opencode", "config":{"serverUrl":"http://user:secret@localhost"}}),
            None
        )
        .is_err());
        assert_eq!(
            excluded.unknown,
            BTreeSet::from(["providers.opencode.config.futureCredential".to_owned()])
        );
        let error = policy
            .validate("provider", &source)
            .unwrap_err()
            .to_string();
        assert!(error.contains("provider.config.futureCredential"));
        assert!(error.contains("provider.config.serverPassword"));
        assert!(!error.contains("secret"));
    }

    fn assert_portable(policy: &Policy, section: &str, saved: Value) {
        let selected = policy.capture(section, &saved, section).unwrap();
        assert_eq!(selected, saved);
        policy.validate(section, &selected).unwrap();

        let mut local = object();
        policy.merge_section(section, &mut local, &selected);
        assert_eq!(local, saved);

        policy.merge_section(section, &mut local, &object());
        assert_eq!(local, object());
    }

    #[test]
    fn pull_request_link_preference_is_portable() {
        let policy = Policy::new();
        for enabled in [true, false] {
            assert_portable(
                &policy,
                "client",
                json!({"pullRequestLinksOpenInApp": enabled}),
            );
        }
    }

    #[test]
    fn auto_resume_limited_threads_is_portable() {
        let policy = Policy::new();
        for enabled in [true, false] {
            assert_portable(
                &policy,
                "server",
                json!({"autoResumeLimitedThreads": enabled}),
            );
        }
    }

    #[test]
    fn browser_rendering_and_worktree_retention_preferences_are_portable() {
        let policy = Policy::new();
        for (section, saved) in [
            (
                "client",
                json!({
                    "browserSearchEngine": "duckduckgo",
                    "browserCustomSearchUrl": "https://search.example/?q=%s",
                    "mathRenderingEnabled": true
                }),
            ),
            (
                "server",
                json!({
                    "idleCompaction": "onReturn",
                    "storageCleanup": {"worktreeSettledAfterDays": 1}
                }),
            ),
        ] {
            assert_portable(&policy, section, saved);
        }
    }

    #[test]
    fn unknown_fields_report_nested_paths_without_ignored_subtrees_or_values() {
        let policy = Policy::new();
        let source = json!({
            "browserProfiles": [{"newSecretField":"do-not-print"}],
            "favorites": [{"provider":"codex", "model":"model", "futureOption":{"token":"do-not-print"}}],
            "providerModelPreferences": {"codex":{"hiddenModels":[], "futurePreference":"do-not-print"}}
        });
        let mut excluded = ExcludedFields::default();
        let selected = policy
            .select(&source, &policy.0["client"], "client", &mut excluded)
            .unwrap();
        assert_eq!(
            excluded.unknown,
            BTreeSet::from([
                "client.favorites[0].futureOption".to_owned(),
                "client.providerModelPreferences.codex.futurePreference".to_owned()
            ])
        );
        assert!(!format!("{:?}", excluded.unknown).contains("do-not-print"));
        assert_eq!(
            selected,
            json!({
                "favorites":[{"provider":"codex","model":"model"}],
                "providerModelPreferences":{"codex":{"hiddenModels":[]}}
            })
        );
    }

    #[test]
    fn restore_preserves_local_secrets_and_unknown_fields_and_clears_defaults() {
        let policy = Policy::new();
        let mut local = json!({"driver":"opencode", "enabled":true, "config":{"serverPassword":"keep", "futureCredential":"keep", "serverUrl":"http://old", "binaryPath":"old"}, "environment":[{"name":"KEY", "value":"keep"}]});
        let saved = json!({"driver":"opencode", "enabled":false, "config":{"serverUrl":"http://localhost:14096"}});
        policy.merge_section("provider", &mut local, &saved);
        assert_eq!(
            local["config"],
            json!({"serverPassword":"keep", "futureCredential":"keep", "serverUrl":"http://localhost:14096"})
        );
        assert_eq!(local["environment"][0]["value"], "keep");
        policy.merge_section(
            "provider",
            &mut local,
            &json!({"driver":"opencode", "config":null}),
        );
        assert_eq!(
            local["config"],
            json!({"serverPassword":"keep", "futureCredential":"keep"})
        );
        let mut client = json!({"fontSizeCode":20, "onboardingCompletedAt":"keep", "browserDefaultViewport":{"_tag":"fixed", "width":1200, "height":800, "future":true}});
        policy.merge_section(
            "client",
            &mut client,
            &json!({"browserDefaultViewport":{"_tag":"fill"}}),
        );
        assert_eq!(
            client,
            json!({"onboardingCompletedAt":"keep", "browserDefaultViewport":{"_tag":"fill", "future":true}})
        );
    }

    #[test]
    fn restore_preserves_explicit_null_model_overrides() {
        let policy = Policy::new();
        for name in ["server", "projectSettings"] {
            for key in [
                "defaultModelSelection",
                "sourceControlWriterModelSelection",
                "reviewGuideModelSelection",
            ] {
                let mut local =
                    json!({key: {"instanceId":"codex", "model":"model", "future":true}});
                let saved = json!({key: null});
                policy.validate(name, &saved).unwrap();
                policy.merge_section(name, &mut local, &saved);
                assert_eq!(local, saved, "{name}.{key}");
                policy.merge_section(name, &mut local, &object());
                assert!(local.get(key).is_none(), "An absent override must inherit");
            }
        }
    }

    #[test]
    fn restores_project_overrides_under_destination_ids() {
        let policy = Policy::new();
        let bundle: Bundle = serde_json::from_value(json!({"version":1,"client":{},"server":{},"desktop":{},"providers":{},"keybindings":[],"projects":[{"workspace_root":"~/dev/repo","attributes":{"title":"repo"},"settings":{"defaultThreadEnvMode":"worktree"}}]})).unwrap();
        let mut local = json!({"providerInstances":{"local":{"driver":"custom", "config":{"password":"keep"}}},"projectSettingsOverrides":{"new-id":{"defaultThreadEnvMode":"local", "future":true},"other-id":{"defaultThreadEnvMode":"local"}},"projectScriptOverrides":{"new-id":[],"other-id":[]}});
        restore_server(
            &policy,
            &mut local,
            &bundle,
            &[("new-id".into(), &bundle.projects[0])],
        )
        .unwrap();
        assert_eq!(
            local["projectSettingsOverrides"]["new-id"],
            json!({"defaultThreadEnvMode":"worktree", "future":true})
        );
        assert_eq!(
            local["projectSettingsOverrides"]["other-id"]["defaultThreadEnvMode"],
            "local"
        );
        assert_eq!(local["projectScriptOverrides"]["new-id"], json!([]));
        assert_eq!(
            local["providerInstances"]["local"]["config"]["password"],
            "keep"
        );
        assert!(
            validate_provider(
                &json!({"driver":"codex"}),
                Some(&json!({"driver":"opencode"}))
            )
            .is_err()
        );
    }

    #[test]
    fn capture_rejects_live_userdata_aliases() {
        let dir = tempfile::tempdir().unwrap();
        let base_dir = dir.path().join("t3");
        fs::create_dir_all(base_dir.join("userdata")).unwrap();
        let alias = dir.path().join("alias");
        std::os::unix::fs::symlink(base_dir.join("userdata"), &alias).unwrap();
        for output in [
            base_dir.join("userdata/../userdata/settings.json"),
            alias.join("settings.json"),
        ] {
            assert!(capture_destination(&output, &base_dir).is_err());
        }
        assert!(capture_destination(&dir.path().join("source/bundle.json"), &base_dir).is_ok());
    }

    #[test]
    fn portable_paths_and_stale_write_protection() {
        assert_eq!(
            portable_path("/home/source/dev/repo", Path::new("/home/source")),
            "~/dev/repo"
        );
        assert_eq!(
            expand_path("~/dev/repo", Path::new("/home/target")),
            Path::new("/home/target/dev/repo")
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, b"{}").unwrap();
        let mut file = SettingsFile::read(path.clone(), object()).unwrap();
        file.value = json!({"fontSizeCode":14});
        fs::write(&path, b"{\"serverPassword\":\"keep\"}").unwrap();
        assert!(file.write().is_err());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{\"serverPassword\":\"keep\"}"
        );
        let link = dir.path().join("linked-settings.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        let mut file = SettingsFile::read(link.clone(), object()).unwrap();
        file.value["fontSizeCode"] = json!(14);
        assert!(file.write().unwrap());
        assert!(link.is_symlink());
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap()["serverPassword"],
            "keep"
        );
    }

    #[test]
    fn empty_managed_maps_are_stable_across_repeated_restores() {
        let policy = Policy::new();
        let saved =
            json!({"loadBalancingWeights": {}, "providerModelPreferences": {}, "favorites": []});
        let mut current = json!({"loadBalancingWeights": null, "providerModelPreferences": {}, "future-local-field": "keep"});
        policy.merge_section("client", &mut current, &saved);
        let first = current.clone();
        assert!(current.get("loadBalancingWeights").is_none());
        assert!(current.get("providerModelPreferences").is_none());
        assert_eq!(current["future-local-field"], "keep");
        for _ in 0..3 {
            policy.merge_section("client", &mut current, &saved);
            assert_eq!(current, first);
        }
    }

    fn sync_bundle(value: Value) -> Value {
        let mut bundle = value;
        bundle["version"] = json!(1);
        bundle
    }

    #[test]
    fn sync_decision_matrix() {
        let base = sync_bundle(json!({"client": {"a": 1}}));
        let live_changed = sync_bundle(json!({"client": {"a": 2}}));
        let bundle_changed = sync_bundle(json!({"client": {"a": 3}}));

        // No prior record: matching sides initialize, differing sides are
        // adopted (no common ancestor exists to call it a conflict).
        assert_eq!(
            decide(&StateLoad::Absent, Some(&base), &base),
            Decision::InSync
        );
        assert_eq!(
            decide(&StateLoad::Absent, Some(&live_changed), &base),
            Decision::AdoptBundle
        );
        // No live settings means provisioning.
        assert_eq!(decide(&StateLoad::Absent, None, &base), Decision::Provision);
        // An unusable record (corrupt or newer version) cannot prove either
        // side unchanged: agreeing sides are still in sync, but differing
        // sides stop instead of adopting.
        assert_eq!(
            decide(&StateLoad::Unusable, Some(&base), &base),
            Decision::InSync
        );
        assert_eq!(
            decide(&StateLoad::Unusable, Some(&live_changed), &base),
            Decision::Conflict
        );
        assert_eq!(
            decide(&StateLoad::Unusable, None, &base),
            Decision::Provision
        );

        let synced = SyncState {
            version: SYNC_STATE_VERSION,
            bundle: base.clone(),
            live: Some(base.clone()),
        };
        let load = StateLoad::Present(synced);
        assert_eq!(decide(&load, Some(&base), &base), Decision::InSync);
        // Only live changed: capture wins.
        assert_eq!(
            decide(&load, Some(&live_changed), &base),
            Decision::CaptureLive
        );
        // Only the bundle changed: apply wins.
        assert_eq!(
            decide(&load, Some(&base), &bundle_changed),
            Decision::ApplyBundle
        );
        // Both changed: conflict.
        assert_eq!(
            decide(&load, Some(&live_changed), &bundle_changed),
            Decision::Conflict
        );
        // Live reappeared after a provisioning that could not snapshot it: the
        // sides agree, so this is in sync and the record is refreshed.
        let provisioned = StateLoad::Present(SyncState {
            version: SYNC_STATE_VERSION,
            bundle: base.clone(),
            live: None,
        });
        assert_eq!(decide(&provisioned, Some(&base), &base), Decision::InSync);
        // Agreeing sides are in sync even against a stale record: manual
        // capture/apply converge the sides without recording sync state.
        let stale = StateLoad::Present(SyncState {
            version: SYNC_STATE_VERSION,
            bundle: sync_bundle(json!({"client": {"a": 0}})),
            live: Some(sync_bundle(json!({"client": {"a": 0}}))),
        });
        assert_eq!(decide(&stale, Some(&base), &base), Decision::InSync);
    }

    #[test]
    fn settings_file_rejects_concurrent_modification() -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("settings.json");
        fs::write(&path, b"{}")?;
        let file = SettingsFile::read(path.clone(), object())?;
        file.verify_unchanged()?;
        fs::write(&path, b"{\"a\": 1}")?;
        assert!(file.verify_unchanged().is_err());
        Ok(())
    }

    fn native_password_tree(
        root: &Path,
        service: Option<&str>,
        config: Value,
    ) -> Result<(PathBuf, PathBuf)> {
        let home = root.join("home");
        let base = root.join("t3");
        fs::create_dir_all(base.join("userdata"))?;
        if let Some(service) = service {
            fs::create_dir_all(home.join(".config/opencode"))?;
            fs::write(home.join(".config/opencode/service.json"), service)?;
        }
        fs::write(
            base.join("userdata/settings.json"),
            serde_json::to_vec(&json!({"providerInstances": {"local": config}}))?,
        )?;
        Ok((home, base))
    }

    fn native_config(server_password: Option<&str>) -> Value {
        let mut config = json!({
            "driver": "opencode",
            "enabled": true,
            "config": {"serverUrl": "http://localhost:14096/"},
        });
        if let Some(password) = server_password {
            config["config"]["serverPassword"] = json!(password);
        }
        config
    }

    #[test]
    fn ensure_native_password_injects_stale_password_once() -> Result<()> {
        let root = tempfile::tempdir()?;
        let (home, base) = native_password_tree(
            root.path(),
            Some(r#"{"password": "rotated"}"#),
            native_config(Some("old")),
        )?;
        assert!(ensure_native_password(&home, &base)?);
        let after: Value = serde_json::from_slice(&fs::read(base.join("userdata/settings.json"))?)?;
        assert_eq!(
            after["providerInstances"]["local"]["config"]["serverPassword"],
            "rotated"
        );
        assert!(!ensure_native_password(&home, &base)?);
        Ok(())
    }

    #[test]
    fn ensure_native_password_skips_without_service_or_native_provider() -> Result<()> {
        let root = tempfile::tempdir()?;
        // Native provider but no service file: leave live settings alone.
        let (home, base) = native_password_tree(root.path(), None, native_config(Some("old")))?;
        assert!(!ensure_native_password(&home, &base)?);
        let after: Value = serde_json::from_slice(&fs::read(base.join("userdata/settings.json"))?)?;
        assert_eq!(
            after["providerInstances"]["local"]["config"]["serverPassword"],
            "old"
        );
        // Service file present but no native provider: nothing to do.
        let other = root.path().join("other");
        fs::create_dir_all(other.join("userdata"))?;
        fs::write(
            other.join("userdata/settings.json"),
            serde_json::to_vec(&json!({"providerInstances": {
                "remote": {"driver": "opencode", "enabled": true,
                           "config": {"serverUrl": "http://example.com/"}},
            }}))?,
        )?;
        assert!(!ensure_native_password(&home, &other)?);
        Ok(())
    }

    #[test]
    fn sync_diff_renders_nested_changes() {
        let old = json!({
            "client": {"a": 1, "gone": true, "nested": {"x": [1, 2]}},
            "server": {"keep": "same"},
        });
        let new = json!({
            "client": {"a": 2, "nested": {"x": [1, 2, 3]}, "added": "yes"},
            "server": {"keep": "same"},
        });
        let diff = diff_summary(&old, &new);
        assert!(diff.iter().any(|line| line == "client.a: 1 → 2"));
        assert!(
            diff.iter()
                .any(|line| line == "client.added: (absent) → \"yes\"")
        );
        assert!(
            diff.iter()
                .any(|line| line == "client.gone: true → (absent)")
        );
        assert!(
            diff.iter()
                .any(|line| line == "client.nested.x: [2 items → 3 items]")
        );
        assert!(diff_summary(&old, &old).is_empty());
    }

    #[test]
    fn sync_diff_truncates_long_values() {
        let long: String = "v".repeat(DIFF_VALUE_LIMIT + 10);
        let diff = diff_summary(&json!({"k": "short"}), &json!({"k": long}));
        assert_eq!(diff.len(), 1);
        assert!(diff[0].ends_with('…'));
    }

    #[test]
    fn sync_state_roundtrips_and_rejects_garbage() -> Result<()> {
        let home = tempfile::tempdir()?;
        assert!(matches!(load_state(home.path()), StateLoad::Absent));
        let state = SyncState {
            version: SYNC_STATE_VERSION,
            bundle: json!({"version": 1}),
            live: Some(json!({"version": 1})),
        };
        save_state(home.path(), &state)?;
        let loaded = match load_state(home.path()) {
            StateLoad::Present(loaded) => loaded,
            _ => bail!("state should load"),
        };
        assert_eq!(loaded.bundle, state.bundle);
        assert_eq!(loaded.live, state.live);
        fs::write(sync_state_path(home.path()), b"not json")?;
        assert!(matches!(load_state(home.path()), StateLoad::Unusable));
        fs::write(
            sync_state_path(home.path()),
            json!({"version": 999, "bundle": {}, "live": null}).to_string(),
        )?;
        assert!(matches!(load_state(home.path()), StateLoad::Unusable));
        Ok(())
    }

    #[test]
    fn sync_history_prunes_to_retention() -> Result<()> {
        let home = tempfile::tempdir()?;
        for index in 0..HISTORY_KEEP + 5 {
            record_history(
                home.path(),
                "capture",
                false,
                vec![format!("change {index}")],
                &json!({}),
                &json!({"index": index}),
            )?;
        }
        let names: Vec<String> = fs::read_dir(history_dir(home.path()))?
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .collect();
        assert_eq!(names.len(), HISTORY_KEEP);
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names.iter().max(), sorted.last());
        Ok(())
    }

    #[test]
    fn sync_conflict_history_skips_identical_reruns() -> Result<()> {
        let home = tempfile::tempdir()?;
        let diff = vec!["live changes since last sync:".to_owned()];
        assert!(last_history_entry(home.path()).is_none());
        record_history(
            home.path(),
            "conflict",
            false,
            diff.clone(),
            &json!({}),
            &json!({}),
        )?;
        let entry = last_history_entry(home.path()).context("history should load")?;
        assert_eq!(entry.direction, "conflict");
        assert_eq!(entry.diff, diff);
        // A different direction must not compare equal.
        record_history(
            home.path(),
            "capture",
            false,
            diff.clone(),
            &json!({}),
            &json!({}),
        )?;
        let entry = last_history_entry(home.path()).context("history should load")?;
        assert_eq!(entry.direction, "capture");
        Ok(())
    }

    #[test]
    fn capture_prunes_defaults_and_apply_restores_them() {
        let policy = Policy::new();
        let defaults = json!({
            "client": {
                "notificationMode": "off",
                "fontSizeCode": 13,
                "browserDefaultViewport": {"_tag": "fill"},
                "snapShotShortcut": {"kind": "both-shift-keys", "metaKey": false}
            },
            "server": {"idleCompaction": "beforeCacheExpires"}
        });

        let live = json!({
            "notificationMode": "off",
            "fontSizeCode": 15,
            "browserDefaultViewport": {"_tag": "fill"},
            "snapShotShortcut": {"kind": "both-shift-keys", "metaKey": false},
            "wordWrap": true
        });
        let mut client = policy.capture("client", &live, "client").unwrap();
        prune_defaults(&mut client, &defaults["client"]);
        assert_eq!(client, json!({"fontSizeCode": 15, "wordWrap": true}));

        let mut local = json!({"notificationMode": "off", "fontSizeCode": 11, "localOnly": true});
        policy.merge_section("client", &mut local, &client);
        assert_eq!(
            local,
            json!({"fontSizeCode": 15, "localOnly": true, "wordWrap": true})
        );

        let mut server = policy
            .capture(
                "server",
                &json!({"idleCompaction": "beforeCacheExpires", "defaultAutoPull": true}),
                "server",
            )
            .unwrap();
        prune_defaults(&mut server, &defaults["server"]);
        assert_eq!(server, json!({"defaultAutoPull": true}));

        // Sync records written before pruning must compare equal after both
        // sides pass through the same filter. Keybindings stay untouched: T3
        // suppresses every default rule for a command that appears in its file,
        // so an absent binding does not mean "default".
        let expected = crate::t3_defaults::Defaults {
            client: defaults["client"].clone(),
            server: defaults["server"].clone(),
        };
        let mut state = json!({
            "client": {"notificationMode": "off", "fontSizeCode": 15},
            "server": {"idleCompaction": "beforeCacheExpires", "defaultAutoPull": true},
            "keybindings": [{"key": "mod+b", "command": "sidebar.toggle"}]
        });
        normalize_bundle_value(&mut state, &expected);
        assert_eq!(
            state,
            json!({
                "client": {"fontSizeCode": 15},
                "server": {"defaultAutoPull": true},
                "keybindings": [{"key": "mod+b", "command": "sidebar.toggle"}]
            })
        );
    }
}
