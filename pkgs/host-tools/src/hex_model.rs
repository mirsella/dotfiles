//! A model-install request only starts a background download. Report completion
//! separately so systemd retries failures without restarting an active download.
use crate::util;
use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

#[derive(Debug, PartialEq)]
enum State {
    Installed,
    Preparing,
    Missing,
}

fn state(status: &Value) -> Result<State> {
    let installed = status
        .pointer("/model/installed")
        .and_then(Value::as_bool)
        .context("Hex status lacks model.installed boolean")?;
    let preparing = status
        .pointer("/model/preparing")
        .context("Hex status lacks model.preparing")?;
    ensure!(
        preparing.is_null() || preparing.is_string(),
        "invalid Hex model.preparing"
    );
    Ok(if installed {
        State::Installed
    } else if preparing.is_null() {
        State::Missing
    } else {
        State::Preparing
    })
}

pub fn run(hex: &str) -> Result<()> {
    match state(&util::json(hex, &["status", "--json"])?)? {
        State::Installed => return Ok(()),
        State::Missing => util::run(hex, &["model", "install"])?,
        State::Preparing => {}
    }
    bail!("Hex model installation is pending; retry after the download finishes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn distinguishes_completion_from_an_active_or_failed_download() -> Result<()> {
        for (installed, preparing, expected) in [
            (true, None, State::Installed),
            (false, Some("parakeet_v3"), State::Preparing),
            (false, None, State::Missing),
        ] {
            assert_eq!(
                state(&json!({"model":{"installed":installed,"preparing":preparing}}))?,
                expected
            );
        }
        assert!(state(&json!({})).is_err());
        assert!(state(&json!({"model":{"installed":true}})).is_err());
        assert!(state(&json!({"model":{"installed":false,"preparing":false}})).is_err());
        Ok(())
    }
}
