use crate::util;
use anyhow::{Context, Result, bail, ensure};
use flate2::read::GzDecoder;
use regex::Regex;
use serde::Deserialize;
use serde_json::{Value, value::RawValue};
use std::{
    borrow::Cow,
    fs,
    io::{BufRead, BufReader, Read},
    path::Path,
    sync::LazyLock,
    time::UNIX_EPOCH,
};

const QUIET: f64 = 1800.0;
const IMMICH: &[&str] = &[
    "assets",
    "albums",
    "users",
    "sync",
    "people",
    "memories",
    "timeline",
    "notifications",
    "trash",
    "search",
    "download",
    "sessions",
];
const OPENCODE: &[&str] = &[
    "session",
    "project",
    "event",
    "path",
    "config",
    "provider",
    "permission",
    "question",
    "file",
    "find",
    "vcs",
    "command",
    "agent",
    "mcp",
    "pty",
];
const CHAMBER: &[&str] = &[
    "session",
    "project",
    "projects",
    "event",
    "config",
    "settings",
    "provider",
    "permission",
    "question",
    "file",
    "find",
    "vcs",
    "command",
    "agent",
    "mcp",
    "terminal",
    "git",
    "skills",
    "plugins",
];

#[derive(Deserialize)]
pub struct AccessLog<'a> {
    ts: f64,
    #[serde(default)]
    duration: f64,
    #[serde(borrow)]
    request: Option<Request<'a>>,
    status: Option<u16>,
    #[serde(borrow)]
    resp_headers: Option<&'a RawValue>,
}

#[derive(Deserialize)]
struct Request<'a> {
    #[serde(default, borrow)]
    method: Cow<'a, str>,
    #[serde(default, borrow)]
    host: Cow<'a, str>,
    #[serde(default, borrow)]
    uri: Cow<'a, str>,
}

impl AccessLog<'_> {
    pub fn authenticated(&self) -> Result<Option<&'static str>> {
        let request = self.request.as_ref().context("missing request")?;
        if request.method == "OPTIONS" {
            return Ok(None);
        }
        let host = request.host.as_ref();
        let path = request.uri.split(['?', '#']).next().unwrap_or("");
        if (host.eq_ignore_ascii_case("mirsella.mooo.com")
            || host.eq_ignore_ascii_case("mirsella.mooo.com:443"))
            && path.starts_with("/nextcloud/")
        {
            if let Some(raw) = self.resp_headers {
                let headers: Value = serde_json::from_str(raw.get())?;
                if headers.as_object().is_some_and(|headers| {
                    headers.iter().any(|(k, v)| {
                        k.eq_ignore_ascii_case("x-user-id")
                            && v.as_array().is_some_and(|a| {
                                a.iter().any(|v| v.as_str().is_some_and(|s| !s.is_empty()))
                            })
                    })
                }) {
                    return Ok(Some("Nextcloud"));
                }
            }
        }
        let status = self.status.context("missing HTTP status")?;
        if !(200..300).contains(&status) && status != 304 {
            return Ok(None);
        }
        let mut parts = path.trim_matches('/').split('/');
        let first = parts.next().unwrap_or("");
        let second = parts.next().unwrap_or("");
        Ok(
            if (host.eq_ignore_ascii_case("photos.mirsella.mooo.com")
                || host.eq_ignore_ascii_case("photos.mirsella.mooo.com:443"))
                && first == "api"
                && (IMMICH.contains(&second)
                    || matches!(
                        path,
                        "/api/auth/login" | "/api/auth/validateToken" | "/api/auth/status"
                    ))
            {
                Some("Immich")
            } else if (host.eq_ignore_ascii_case("mirsella.mooo.com:4096")
                || host.eq_ignore_ascii_case("mirsella.mooo.com:14096"))
                && (OPENCODE.contains(&first) || path == "/global/event")
            {
                Some("OpenCode")
            } else if (host.eq_ignore_ascii_case("mirsella.mooo.com:4097")
                || host.eq_ignore_ascii_case("mirsella.mooo.com:14097"))
                && ((first == "api" && CHAMBER.contains(&second))
                    || matches!(path, "/auth/session" | "/api/global/event"))
            {
                Some("OpenChamber")
            } else {
                None
            },
        )
    }
}

pub fn recent_logs(directory: &Path, now: f64) -> Result<Option<String>> {
    let cutoff = now - QUIET;
    let mut latest: Option<(f64, &str)> = None;
    for file in fs::read_dir(directory).context("read Caddy access log directory")? {
        let file = file?;
        let name = file.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("access-") || !name.contains(".log") {
            continue;
        }
        if file
            .metadata()?
            .modified()?
            .duration_since(UNIX_EPOCH)?
            .as_secs_f64()
            < cutoff
        {
            continue;
        }
        let path = file.path();
        let input = fs::File::open(&path)?;
        let input: Box<dyn Read> = if name.ends_with(".gz") {
            Box::new(GzDecoder::new(input))
        } else {
            Box::new(input)
        };
        let mut input = BufReader::new(input);
        let mut line = String::new();
        loop {
            line.clear();
            if input.read_line(&mut line)? == 0 {
                break;
            }
            let entry: AccessLog<'_> =
                serde_json::from_str(&line).with_context(|| format!("parse {}", path.display()))?;
            let finished = entry.ts + entry.duration;
            ensure!(finished.is_finite(), "non-finite request completion time");
            if finished < cutoff {
                continue;
            }
            if let Some(app) = entry.authenticated()? {
                if latest.is_none_or(|(t, _)| finished > t) {
                    latest = Some((finished, app));
                }
            }
        }
    }
    Ok(latest.map(|(t, app)| {
        format!(
            "authenticated {app} activity {}s ago (quiet period 1800s)",
            (now - t).max(0.0) as u64
        )
    }))
}

pub fn authenticated_ssh(titles: &str) -> bool {
    static SSH: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^sshd(?:-session)?: (?:\S+ \[postauth\]|[^\s@]+@(?:notty|pts/\d+|tty\S+))$")
            .unwrap()
    });
    titles.lines().any(|line| SSH.is_match(line.trim()))
}

pub fn upstream(connections: &str) -> Result<Option<String>> {
    for line in connections.lines() {
        let mut fields = line.split_whitespace();
        let peer = fields
            .nth(3)
            .with_context(|| format!("unrecognized ss row: {line}"))?;
        let app = match peer {
            "127.0.0.1:8080" => "Nextcloud",
            "127.0.0.1:2283" => "Immich",
            "192.168.1.131:4096" => "OpenCode",
            "192.168.1.131:4097" => "OpenChamber",
            _ => continue,
        };
        if fields.next().is_none() {
            bail!("missing socket owner; activity check requires root");
        }
        if line.contains("\"caddy\",pid=") {
            return Ok(Some(format!(
                "open {app} proxy connection (transfer/stream guard)"
            )));
        }
    }
    Ok(None)
}

pub fn blocker(now: f64) -> Result<Option<String>> {
    let sessions = util::json("loginctl", &["list-sessions", "--json=short"])?;
    for session in sessions.as_array().context("invalid loginctl sessions")? {
        if session["class"]
            .as_str()
            .context("missing session class")?
            .starts_with("user")
        {
            return Ok(Some("SSH or local login session".into()));
        }
    }
    if authenticated_ssh(&util::output(
        "ps",
        &["-C", "sshd,sshd-session", "-o", "args="],
    )?) {
        return Ok(Some("authenticated SSH connection".into()));
    }
    if let Some(reason) = upstream(&util::output("ss", &["-Hntp", "state", "established"])?)? {
        return Ok(Some(reason));
    }
    recent_logs(Path::new("/var/log/caddy"), now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_access_fields_borrow_the_input_buffer() -> Result<()> {
        let line = r#"{"ts":100,"request":{"method":"GET","host":"photos.mirsella.mooo.com","uri":"/api/assets"},"status":200}"#;
        let log: AccessLog<'_> = serde_json::from_str(line)?;
        let request = log.request.unwrap();
        assert!(matches!(request.method, Cow::Borrowed("GET")));
        assert!(matches!(
            request.host,
            Cow::Borrowed("photos.mirsella.mooo.com")
        ));
        assert!(matches!(request.uri, Cow::Borrowed("/api/assets")));
        Ok(())
    }
}
