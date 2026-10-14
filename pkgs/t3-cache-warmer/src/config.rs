use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

/// Which local hours warming may run in. `start > end` wraps midnight.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct DayWindow {
    /// Local time, `HH:MM`.
    pub start: String,
    /// Local time, `HH:MM`. exclusive.
    pub end: String,
}

impl Default for DayWindow {
    fn default() -> Self {
        Self {
            start: "08:00".to_owned(),
            end: "23:00".to_owned(),
        }
    }
}

/// A provider/model allowlist entry. The first matching rule wins, so order
/// rules from most to least specific.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Rule {
    /// Provider instance id, e.g. `codex` or `opencode`. `None` matches any.
    pub provider: Option<String>,
    /// Substring of the thread model id, e.g. `gpt`, `claude`, `gemini`.
    pub model_contains: String,
    /// How long a written prompt cache is assumed to live.
    pub ttl_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Config {
    /// T3 server base URL.
    pub base_url: String,
    /// How the `login`/renewal flow reaches the `t3` CLI for a pairing code.
    pub t3_cli_path: String,
    /// Local hours warming may run in.
    pub day_window: DayWindow,
    /// Only warm threads with real activity within this window.
    pub active_window_minutes: u64,
    /// Upper bound between full passes of `run`: it wakes earlier when a
    /// refresh comes due, and at least this often to notice new threads.
    pub tick_seconds: u64,
    /// Warm this many seconds before a cache TTL elapses.
    pub lead_seconds: u64,
    /// Keepalives allowed per period of real user activity.
    pub max_refreshes_per_activity: u32,
    /// A keepalive turn is attributed to us when thread activity lands within
    /// this many seconds of our send.
    pub send_grace_seconds: u64,
    /// Renew the MCP credential automatically before it expires.
    pub auto_renew: bool,
    /// Drop thread state not seen for this many days.
    pub prune_days: u64,
    /// OAuth loopback redirect registered for the client. Never opened.
    pub redirect_uri: String,
    /// The keepalive prompt. Reply is expected to be a single word.
    pub keepalive_message: String,
    /// Project ids to consider. Empty means every project.
    pub projects: Vec<String>,
    /// Skip subagent threads.
    pub skip_subagents: bool,
    /// Warming allowlist. Empty disables warming entirely.
    pub rules: Vec<Rule>,
    /// Override the state directory.
    pub state_dir: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:3774".to_owned(),
            t3_cli_path: "t3".to_owned(),
            day_window: DayWindow::default(),
            active_window_minutes: 150,
            tick_seconds: 60,
            lead_seconds: 30,
            max_refreshes_per_activity: 2,
            send_grace_seconds: 300,
            auto_renew: true,
            prune_days: 7,
            redirect_uri: "http://127.0.0.1:47811/callback".to_owned(),
            keepalive_message: "[cache keepalive] reply with just: ok".to_owned(),
            projects: Vec::new(),
            skip_subagents: true,
            rules: Vec::new(),
            state_dir: None,
        }
    }
}

impl Config {
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read config {}", path.display()))?;
        let config: Self = serde_json::from_str(&text)
            .with_context(|| format!("parse config {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if !self.base_url.starts_with("http://") && !self.base_url.starts_with("https://") {
            bail!("baseUrl must start with http:// or https://");
        }
        parse_hhmm(&self.day_window.start, "dayWindow.start")?;
        parse_hhmm(&self.day_window.end, "dayWindow.end")?;
        if self.tick_seconds < 5 || self.tick_seconds > 3600 {
            bail!("tickSeconds must be between 5 and 3600");
        }
        if self.active_window_minutes == 0 {
            bail!("activeWindowMinutes must be positive");
        }
        if self.send_grace_seconds < 60 {
            bail!("sendGraceSeconds must be at least 60");
        }
        if !self.redirect_uri.starts_with("http://127.0.0.1:") {
            bail!("redirectUri must be a loopback address");
        }
        for (index, rule) in self.rules.iter().enumerate() {
            if rule.model_contains.is_empty() && rule.provider.is_none() {
                bail!("rules[{index}] must set modelContains, provider, or both");
            }
            if rule.ttl_seconds < 30 {
                bail!("rules[{index}].ttlSeconds must be at least 30");
            }
        }
        Ok(())
    }

    pub fn state_dir(&self) -> Result<PathBuf> {
        if let Some(dir) = &self.state_dir {
            return Ok(dir.clone());
        }
        let home = std::env::var_os("HOME").context("HOME is not set")?;
        Ok(PathBuf::from(home).join(".local/state/t3-cache-warmer"))
    }
}

/// Parses `HH:MM` into minutes since local midnight.
pub fn parse_hhmm(value: &str, field: &str) -> Result<u16> {
    let (hours, minutes) = value
        .split_once(':')
        .with_context(|| format!("{field} must look like HH:MM"))?;
    let hours: u16 = hours
        .parse()
        .with_context(|| format!("{field} has an invalid hour"))?;
    let minutes: u16 = minutes
        .parse()
        .with_context(|| format!("{field} has an invalid minute"))?;
    if hours > 23 || minutes > 59 {
        bail!("{field} is not a valid time of day");
    }
    Ok(hours * 60 + minutes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_parse_and_validate() {
        Config::default().validate().expect("default config");
    }

    #[test]
    fn wraps_are_parsed() {
        assert_eq!(parse_hhmm("00:00", "t").unwrap(), 0);
        assert_eq!(parse_hhmm("23:59", "t").unwrap(), 1439);
        assert!(parse_hhmm("24:00", "t").is_err());
        assert!(parse_hhmm("8", "t").is_err());
    }

    #[test]
    fn rejects_unusable_rules() {
        let mut config = Config::default();
        config.rules.push(Rule {
            provider: None,
            model_contains: String::new(),
            ttl_seconds: 300,
        });
        assert!(config.validate().is_err());
        let mut config = Config::default();
        config.rules.push(Rule {
            provider: None,
            model_contains: "gpt".to_owned(),
            ttl_seconds: 10,
        });
        assert!(config.validate().is_err());
    }
}
