//! Pass the native service's private credential to clients without a second secret.
use anyhow::{Context, Result, bail};
use std::{
    ffi::OsString, fs, io, os::unix::process::CommandExt, path::Path, process::Command,
    time::Duration,
};

pub(crate) fn password(path: &Path) -> Result<Option<String>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("read native OpenCode service configuration"),
    };
    let mut config: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&bytes).context("parse native OpenCode service configuration")?;
    match config.remove("password") {
        None => Ok(None),
        Some(serde_json::Value::String(value)) => Ok((!value.is_empty()).then_some(value)),
        _ => bail!("native OpenCode service password must be a string"),
    }
}

pub(crate) fn wait_password(path: &Path) -> Result<String> {
    crate::util::wait_for_file(path, Duration::from_secs(30), || password(path))
}

pub fn client_env(path: &Path, command: &[OsString]) -> Result<()> {
    let (program, args) = command
        .split_first()
        .context("client command is required")?;
    let password = wait_password(path)?;
    let error = Command::new(program)
        .args(args)
        .env("OPENCODE_PASSWORD", &password)
        .env("OPENCODE_SERVER_PASSWORD", &password)
        .exec();
    Err(error).context("execute client with native OpenCode service credential")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_credentials_wait_for_initialization_and_reject_invalid_config() -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("service.json");
        assert_eq!(password(&path)?, None);
        fs::write(&path, r#"{"port":14096}"#)?;
        assert_eq!(password(&path)?, None);
        fs::write(&path, r#"{"password":"test-generated"}"#)?;
        assert_eq!(wait_password(&path)?, "test-generated");
        fs::write(&path, r#"{"password":false}"#)?;
        assert!(password(&path).is_err());
        fs::write(&path, "{")?;
        assert!(password(&path).is_err());
        Ok(())
    }
}
