use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

/// Settings T3 itself considers default for the installed app version.
///
/// Capture omits values equal to these so the portable bundle only carries
/// deliberate changes. Extraction failure is not fatal: capture then keeps
/// every allowlisted value, which is noisier but always correct.
#[derive(Deserialize)]
pub struct Defaults {
    pub client: Value,
    pub server: Value,
}

pub fn load(home: &Path) -> Option<Defaults> {
    read(home).unwrap_or_else(|error| {
        eprintln!("t3-settings: keeping all settings; cannot read T3 defaults: {error:#}");
        None
    })
}

fn read(home: &Path) -> Result<Option<Defaults>> {
    let Some(image) = app_image(home)? else {
        return Ok(None);
    };
    // The helper keys its cache on the image and on its own contents, so an app
    // update or a helper change recomputes defaults without version pins here.
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
    let defaults: Defaults =
        serde_json::from_slice(&output.stdout).context("parsing extracted defaults")?;
    ensure!(
        defaults.client.is_object(),
        "client defaults are not an object"
    );
    ensure!(
        defaults.server.is_object(),
        "server defaults are not an object"
    );
    Ok(Some(defaults))
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
