use crate::util;
use anyhow::{Context, Result, ensure};
use chrono::Utc;
use serde::Deserialize;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Deserialize)]
pub struct Task {
    pub program: String,
    pub args: Vec<String>,
    pub stdout: Option<PathBuf>,
}

impl Task {
    pub fn run(&self, staging: &Path) -> Result<()> {
        let mut command = Command::new(&self.program);
        command.args(&self.args).current_dir(staging);
        if let Some(output) = &self.stdout {
            relative(output)?;
            command.stdout(Stdio::from(fs::File::create(staging.join(output))?));
        }
        let status = command
            .status()
            .with_context(|| format!("backup command {}", self.program))?;
        ensure!(
            status.success(),
            "backup command {} exited {status}",
            self.program
        );
        Ok(())
    }
}
#[derive(Deserialize)]
pub struct Config {
    pub dataset: String,
    pub mountpoint: PathBuf,
    pub output: PathBuf,
    #[serde(default)]
    pub directories: Vec<PathBuf>,
    pub commands: Vec<Task>,
}

fn relative(path: &Path) -> Result<()> {
    ensure!(
        !path.as_os_str().is_empty()
            && path.components().all(|c| matches!(c, Component::Normal(_))),
        "invalid backup-relative path: {}",
        path.display()
    );
    Ok(())
}

pub fn publish(
    out: &Path,
    name: &str,
    populate: impl FnOnce(&Path) -> Result<()>,
) -> Result<PathBuf> {
    ensure!(
        name.starts_with("backup-") && Path::new(name).components().count() == 1,
        "invalid bundle name"
    );
    fs::create_dir_all(out)?;
    ensure!(
        !out.symlink_metadata()?.file_type().is_symlink(),
        "backup output must not be a symlink"
    );
    fs::set_permissions(out, fs::Permissions::from_mode(0o700))?;
    let pending = out.join(".latest");
    for path in [&pending, &out.join("latest")] {
        match path.symlink_metadata() {
            Ok(m) => ensure!(
                m.file_type().is_symlink(),
                "{} must be a symlink",
                path.display()
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    let staging = tempfile::Builder::new()
        .prefix(".incomplete.")
        .tempdir_in(out)?;
    populate(staging.path())?;
    let destination = out.join(name);
    match destination.symlink_metadata() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
        Ok(_) => anyhow::bail!("backup destination already exists"),
    }
    // Each systemd oneshot serializes its own output directory.
    fs::rename(staging.path(), &destination)?;
    if pending.symlink_metadata().is_ok() {
        fs::remove_file(&pending)?;
    }
    symlink(name, &pending)?;
    fs::rename(pending, out.join("latest"))?;
    let mut older = Vec::new();
    for entry in fs::read_dir(out)? {
        let entry = entry?;
        if entry.file_type()?.is_dir()
            && entry.file_name().to_string_lossy().starts_with("backup-")
            && entry.path() != destination
        {
            older.push(entry.path());
        }
    }
    older.sort();
    for old in older.iter().take(older.len().saturating_sub(13)) {
        fs::remove_dir_all(old)?;
    }
    println!("Backup completed: {}", destination.display());
    Ok(destination)
}

pub fn run(path: &Path) -> Result<()> {
    ensure!(
        unsafe { libc::geteuid() } == 0,
        "private system backups require root"
    );
    let config: Config = serde_json::from_str(&util::read(path)?)?;
    util::run(
        "findmnt",
        &[
            "--source",
            &config.dataset,
            "--mountpoint",
            util::path(&config.mountpoint)?,
        ],
    )?;
    let name = Utc::now().format("backup-%Y%m%dT%H%M%S.%9fZ").to_string();
    publish(&config.output, &name, |staging| {
        std::os::unix::fs::chown(&config.output, Some(0), Some(0))?;
        for directory in &config.directories {
            relative(directory)?;
            let directory = staging.join(directory);
            fs::create_dir_all(&directory)?;
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
        }
        for task in &config.commands {
            task.run(staging)?;
        }
        Ok(())
    })?;
    Ok(())
}
