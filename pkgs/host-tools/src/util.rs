use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::{fs, path::Path, process::Command, time::Duration};

pub fn path(path: &Path) -> Result<&str> {
    path.to_str().context("path is not UTF-8")
}

pub fn output(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("start {program}"))?;
    if !output.status.success() {
        bail!(
            "{program} exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8(output.stdout).with_context(|| format!("{program} returned non-UTF-8 output"))
}

pub fn run(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("start {program}"))?;
    anyhow::ensure!(status.success(), "{program} exited {status}");
    Ok(())
}

pub fn json(program: &str, args: &[&str]) -> Result<Value> {
    serde_json::from_str(&output(program, args)?).with_context(|| format!("parse {program} JSON"))
}

pub fn read(path: impl AsRef<Path>) -> Result<String> {
    let p = path.as_ref();
    fs::read_to_string(p).with_context(|| format!("read {}", p.display()))
}

pub fn agent(seconds: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(seconds)))
        .http_status_as_error(false)
        .build()
        .into()
}

pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut temporary =
        tempfile::NamedTempFile::new_in(path.parent().context("checkpoint lacks parent")?)?;
    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    Ok(())
}
