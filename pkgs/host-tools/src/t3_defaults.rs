use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

/// Settings T3 itself considers default for the running app version.
///
/// Capture omits values equal to these so the portable bundle only carries
/// deliberate changes. Defaults only apply when the desktop app matches the
/// running server; otherwise an update that has not restarted yet could prune
/// values the server still needs. Extraction failure is not fatal: capture then
/// keeps every allowlisted value, which is noisier but always correct.
#[derive(Deserialize)]
pub struct Defaults {
    pub client: Value,
    pub server: Value,
}

#[derive(Deserialize)]
struct Extracted {
    version: String,
    #[serde(flatten)]
    defaults: Defaults,
}

pub fn load(home: &Path, base_dir: &Path) -> Option<Defaults> {
    read(home, base_dir).unwrap_or_else(|error| {
        eprintln!("t3-settings: keeping all settings; cannot read T3 defaults: {error:#}");
        None
    })
}

fn read(home: &Path, base_dir: &Path) -> Result<Option<Defaults>> {
    let Some(image) = app_image(home)? else {
        return Ok(None);
    };
    let Some(server_version) = running_server_version(base_dir) else {
        eprintln!("t3-settings: keeping all settings; cannot read the running T3 version");
        return Ok(None);
    };
    // The helper keys its cache on the image and on its own contents, so an app
    // update or a helper change recomputes defaults.
    let cache = crate::agent_tools::state_dir(home).join("t3-settings-defaults");
    fs::create_dir_all(&cache)?;
    let script = cache.join("defaults.mjs");
    if fs::read(&script).ok().as_deref() != Some(SCRIPT.as_bytes()) {
        fs::write(&script, SCRIPT)?;
    }
    let node = std::env::var_os("T3_SETTINGS_NODE").unwrap_or_else(|| "node".into());
    let output = Command::new(&node)
        .arg(&script)
        .arg(&image)
        .arg(&cache)
        .output()
        .with_context(|| format!("running {}", Path::new(&node).display()))?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let extracted: Extracted =
        serde_json::from_slice(&output.stdout).context("parsing extracted defaults")?;
    ensure!(
        extracted.defaults.client.is_object(),
        "client defaults are not an object"
    );
    ensure!(
        extracted.defaults.server.is_object(),
        "server defaults are not an object"
    );
    let app_version = extracted.version.clone();
    let defaults = defaults_for(extracted, &server_version);
    if defaults.is_none() {
        eprintln!(
            "t3-settings: keeping all settings; defaults come from T3 {app_version}, but the server runs {server_version}"
        );
    }
    Ok(defaults)
}

fn defaults_for(extracted: Extracted, server_version: &str) -> Option<Defaults> {
    (extracted.version == server_version).then_some(extracted.defaults)
}

fn running_server_version(base_dir: &Path) -> Option<String> {
    let runtime = fs::read(base_dir.join("userdata/server-runtime.json")).ok()?;
    let runtime: Value = serde_json::from_slice(&runtime).ok()?;
    Some(runtime.get("serverVersion")?.as_str()?.to_owned())
}

fn app_image(home: &Path) -> Result<Option<PathBuf>> {
    let dir = home.join(".local/share/t3code/app");
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut images = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "AppImage")
        {
            let modified = fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            images.push((modified, path));
        }
    }
    Ok(images.into_iter().max().map(|(_, path)| path))
}

const SCRIPT: &str = include_str!("t3-defaults.mjs");

#[cfg(test)]
mod tests {
    use super::*;

    fn base_dir(runtime: Option<&str>) -> tempfile::TempDir {
        let base = tempfile::tempdir().unwrap();
        if let Some(runtime) = runtime {
            fs::create_dir_all(base.path().join("userdata")).unwrap();
            fs::write(base.path().join("userdata/server-runtime.json"), runtime).unwrap();
        }
        base
    }

    #[test]
    fn reads_the_running_server_version() {
        let base = base_dir(Some(r#"{"serverVersion":"0.1.3-nightly.1"}"#));
        assert_eq!(
            running_server_version(base.path()).as_deref(),
            Some("0.1.3-nightly.1")
        );
    }

    #[test]
    fn only_accepts_defaults_from_the_running_version() {
        let extracted = |version: &str| Extracted {
            version: version.to_owned(),
            defaults: Defaults {
                client: serde_json::json!({"a": 1}),
                server: serde_json::json!({"b": 2}),
            },
        };
        assert!(defaults_for(extracted("0.1.3"), "0.1.3").is_some());
        assert!(defaults_for(extracted("0.1.4"), "0.1.3").is_none());
    }

    #[test]
    fn parses_the_extracted_defaults() {
        let extracted: Extracted =
            serde_json::from_str(r#"{"version":"0.1.3","client":{"a":1},"server":{"b":2}}"#)
                .unwrap();
        assert_eq!(extracted.version, "0.1.3");
        assert_eq!(extracted.defaults.client, serde_json::json!({"a": 1}));
        assert_eq!(extracted.defaults.server, serde_json::json!({"b": 2}));
    }

    #[test]
    fn ignores_a_runtime_without_a_version() {
        assert_eq!(running_server_version(base_dir(None).path()), None);
        assert_eq!(running_server_version(base_dir(Some("{}")).path()), None);
        assert_eq!(
            running_server_version(base_dir(Some(r#"{"serverVersion":1}"#)).path()),
            None
        );
    }
}
