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
    /// T3 server base URL. `None` discovers the origin of the running T3
    /// server from its runtime file, so a moved port keeps working.
    pub base_url: Option<String>,
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
    /// OAuth loopback redirect registered for the client. `None` derives
    /// `<baseUrl>/callback`. Never opened.
    pub redirect_uri: Option<String>,
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
            base_url: None,
            t3_cli_path: "t3".to_owned(),
            day_window: DayWindow::default(),
            active_window_minutes: 150,
            tick_seconds: 60,
            lead_seconds: 30,
            max_refreshes_per_activity: 3,
            send_grace_seconds: 300,
            auto_renew: true,
            prune_days: 7,
            redirect_uri: None,
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
        // This build has no TLS stack, so https:// could never work.
        if let Some(base_url) = &self.base_url
            && !base_url.starts_with("http://")
        {
            bail!("baseUrl must use plain http://");
        }
        let start = parse_hhmm(&self.day_window.start, "dayWindow.start")?;
        let end = parse_hhmm(&self.day_window.end, "dayWindow.end")?;
        if start == end {
            bail!("dayWindow.start and dayWindow.end must differ");
        }
        if self.tick_seconds < 5 || self.tick_seconds > 3600 {
            bail!("tickSeconds must be between 5 and 3600");
        }
        if self.active_window_minutes == 0 {
            bail!("activeWindowMinutes must be positive");
        }
        if self.prune_days == 0 {
            // Zero would read as "disable pruning" but prune it as "keep
            // nothing": every pass would wipe the state it just loaded.
            bail!("pruneDays must be at least 1");
        }
        if self.send_grace_seconds < 60 {
            bail!("sendGraceSeconds must be at least 60");
        }
        if let Some(redirect_uri) = &self.redirect_uri
            && !valid_redirect_uri(redirect_uri)
        {
            bail!("redirectUri must be http://127.0.0.1:<port> or http://localhost:<port>");
        }
        if self.keepalive_message.trim().is_empty() {
            bail!("keepaliveMessage must not be empty");
        }
        for (index, rule) in self.rules.iter().enumerate() {
            if rule.model_contains.is_empty() && rule.provider.is_none() {
                bail!("rules[{index}] must set modelContains, provider, or both");
            }
            if rule.ttl_seconds < 30 {
                bail!("rules[{index}].ttlSeconds must be at least 30");
            }
            if self.lead_seconds >= rule.ttl_seconds {
                bail!(
                    "leadSeconds ({}) must be below rules[{index}].ttlSeconds ({})",
                    self.lead_seconds,
                    rule.ttl_seconds
                );
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

    /// The T3 server base URL: the configured override, or the origin of the
    /// running T3 server from its runtime file. Resolved per call so a server
    /// restarted on another port is picked up.
    pub fn base_url(&self) -> Result<String> {
        let url = match &self.base_url {
            Some(url) => url.clone(),
            None => runtime_origin()
                .context("no baseUrl configured and the running T3 server could not be located")?,
        };
        if !url.starts_with("http://") {
            bail!("baseUrl must use plain http://");
        }
        Ok(url)
    }
}

/// The origin the running T3 server published in its runtime file, when it
/// exists and parses. Best-effort: a missing or malformed file means no
/// discovery source, and callers report that through their own error path.
pub fn runtime_origin() -> Option<String> {
    let home = std::env::var_os("HOME")?;
    runtime_origin_from(&PathBuf::from(home).join(".t3/userdata/server-runtime.json"))
}

fn runtime_origin_from(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let runtime: serde_json::Value = serde_json::from_str(&text).ok()?;
    if let Some(origin) = runtime.get("origin").and_then(serde_json::Value::as_str) {
        return Some(origin.to_owned());
    }
    let port = runtime.get("port").and_then(serde_json::Value::as_u64)?;
    Some(format!("http://127.0.0.1:{port}"))
}

/// True for `http://127.0.0.1:<port>[...]` or `http://localhost:<port>[...]`.
/// A string prefix is not enough: `http://127.0.0.1:1@evil.com/` would pass
/// one while pointing at another host entirely.
pub fn valid_redirect_uri(uri: &str) -> bool {
    let Some(authority) = uri
        .strip_prefix("http://")
        .and_then(|rest| rest.split('/').next())
    else {
        return false;
    };
    if authority.contains('@') {
        return false;
    }
    let Some((host, port)) = authority.rsplit_once(':') else {
        return false;
    };
    (host == "127.0.0.1" || host == "localhost")
        && !port.is_empty()
        && port.len() <= 5
        && port.bytes().all(|byte| byte.is_ascii_digit())
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
        let config = Config {
            lead_seconds: 300,
            rules: vec![Rule {
                provider: None,
                model_contains: "gpt".to_owned(),
                ttl_seconds: 300,
            }],
            ..Config::default()
        };
        assert!(config.validate().is_err(), "lead must stay below the ttl");
    }

    #[test]
    fn rejects_unusable_scalars() {
        let config = Config {
            base_url: Some("https://t3.example.com".to_owned()),
            ..Config::default()
        };
        assert!(config.validate().is_err(), "no TLS stack for https");
        let config = Config {
            prune_days: 0,
            ..Config::default()
        };
        assert!(config.validate().is_err(), "prune 0 would wipe state");
        let config = Config {
            keepalive_message: "   ".to_owned(),
            ..Config::default()
        };
        assert!(config.validate().is_err(), "blank keepalive");
        let config = Config {
            day_window: DayWindow {
                start: "09:00".to_owned(),
                end: "09:00".to_owned(),
            },
            ..Config::default()
        };
        assert!(config.validate().is_err(), "empty window warms nothing");
    }

    #[test]
    fn redirect_uri_must_be_loopback() {
        assert!(valid_redirect_uri("http://127.0.0.1:39001/callback"));
        assert!(valid_redirect_uri("http://localhost:8080/"));
        assert!(!valid_redirect_uri("http://127.0.0.1:1@evil.com/"));
        assert!(!valid_redirect_uri("http://127.0.0.1:/callback"));
        assert!(!valid_redirect_uri("http://127.0.0.1/callback"));
        assert!(!valid_redirect_uri("https://127.0.0.1:39001/callback"));
        assert!(!valid_redirect_uri("http://example.com:39001/callback"));
    }

    #[test]
    fn reads_the_runtime_origin() {
        let dir = std::env::temp_dir().join(format!("warmer-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create test dir");
        let path = dir.join("server-runtime.json");
        std::fs::write(&path, r#"{"origin":"http://127.0.0.1:3999","port":3999}"#).expect("write");
        assert_eq!(
            runtime_origin_from(&path).as_deref(),
            Some("http://127.0.0.1:3999")
        );
        std::fs::write(&path, r#"{"port":4001}"#).expect("write");
        assert_eq!(
            runtime_origin_from(&path).as_deref(),
            Some("http://127.0.0.1:4001"),
            "origin falls back to the port"
        );
        std::fs::write(&path, "not json").expect("write");
        assert_eq!(runtime_origin_from(&path), None);
        assert_eq!(runtime_origin_from(&dir.join("missing.json")), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn explicit_base_url_wins_and_must_be_plain_http() {
        let config = Config {
            base_url: Some("http://127.0.0.1:9".to_owned()),
            ..Config::default()
        };
        assert_eq!(config.base_url().expect("resolve"), "http://127.0.0.1:9");
        let config = Config {
            base_url: Some("https://127.0.0.1:9".to_owned()),
            ..Config::default()
        };
        assert!(config.base_url().is_err(), "no TLS stack for https");
    }
}
