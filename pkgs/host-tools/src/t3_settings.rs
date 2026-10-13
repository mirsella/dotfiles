use crate::util;
use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Subcommand)]
pub enum Operation {
    /// Capture allowed settings directly into the chezmoi source, without committing.
    Capture {
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Merge a portable bundle into T3, preserving credentials and device-local state.
    Apply {
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
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

    fn write(self) -> Result<bool> {
        if let Some(bytes) = &self.original {
            if serde_json::from_slice::<Value>(bytes)? == self.value {
                return Ok(false);
            }
        }
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
    let result = Command::new("t3")
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

impl Session {
    fn open(base_dir: &Path, write: bool) -> Result<Self> {
        let runtime: Value = serde_json::from_slice(
            &fs::read(base_dir.join("userdata/server-runtime.json"))
                .context("Start the T3 background server before capturing or restoring projects")?,
        )
        .context("Parsing T3 server runtime")?;
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
        if let Some(entries) = server[key].as_object() {
            for (id, value) in entries {
                policy.capture(name, value, &format!("server.{key}.{id}"))?;
            }
        }
    }
    Ok(())
}

fn capture(policy: &Policy, home: &Path, base_dir: &Path) -> Result<Bundle> {
    let dir = base_dir.join("userdata");
    let client = policy.capture(
        "client",
        &SettingsFile::read(dir.join("client-settings.json"), object())?.value,
        "client",
    )?;
    let raw_server = SettingsFile::read(dir.join("settings.json"), object())?.value;
    let server = policy.capture("server", &raw_server, "server")?;
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
    let mut providers = Map::new();
    if let Some(instances) = raw_server["providerInstances"].as_object() {
        for (id, instance) in instances {
            let saved = policy.capture(
                "provider",
                instance,
                &format!("server.providerInstances.{id}"),
            )?;
            validate_provider(&saved, None)?;
            providers.insert(id.clone(), saved);
        }
    }
    let mut overrides = Map::new();
    if let Some(entries) = raw_server["projectSettingsOverrides"].as_object() {
        for (id, value) in entries {
            overrides.insert(
                id.clone(),
                policy.capture(
                    "projectSettings",
                    value,
                    &format!("server.projectSettingsOverrides.{id}"),
                )?,
            );
        }
    }
    let session = Session::open(base_dir, false)?;
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

fn apply(
    policy: &Policy,
    mut bundle: Bundle,
    home: &Path,
    base_dir: &Path,
    dry_run: bool,
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
    let session = Session::open(base_dir, !dry_run)?;
    let ids = restore_projects(&session, &bundle, home, dry_run)?;
    server = SettingsFile::read(dir.join("settings.json"), object())?;
    restore_server(policy, &mut server.value, &bundle, &ids)?;
    if dry_run {
        println!(
            "Validated portable settings and {} existing projects; no settings or projects changed",
            ids.len()
        );
        return Ok(());
    }
    let mut changed = 0;
    for file in [client, desktop, keybindings, server] {
        changed += usize::from(file.write()?);
    }
    println!(
        "Merged portable settings into {changed} files and restored {} projects. Restart the desktop to reload client preferences",
        ids.len()
    );
    Ok(())
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
    let base_dir = base_dir
        .or_else(|| std::env::var_os("T3CODE_HOME").map(PathBuf::from))
        .unwrap_or_else(|| home.join(".t3"));
    let policy = Policy::new();
    match operation {
        Operation::Capture { output } => {
            let output = output.unwrap_or_else(|| {
                    home.join("dev/dotfiles/dotfiles/private_dot_config/t3code/private_portable-settings.json")
            });
            let output = capture_destination(&output, &base_dir)?;
            let bundle = capture(&policy, &home, &base_dir)?;
            validate_bundle(&policy, &bundle)?;
            util::atomic_write(&output, &json_bytes(&bundle)?)?;
            println!(
                "Captured allowed settings and {} projects into {}. Review the source diff before committing",
                bundle.projects.len(),
                output.display()
            );
        }
        Operation::Apply { input, dry_run } => {
            let input = input.unwrap_or_else(|| home.join(".config/t3code/portable-settings.json"));
            let bundle = serde_json::from_slice(
                &fs::read(&input).with_context(|| format!("Reading {}", input.display()))?,
            )
            .context("Parsing portable T3 bundle")?;
            apply(&policy, bundle, &home, &base_dir, dry_run)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(validate_provider(&json!({"driver":"opencode", "config":{"serverUrl":"http://user:secret@localhost"}}), None).is_err());
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
}
