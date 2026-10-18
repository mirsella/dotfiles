//! The one-time MCP sign-in. T3 approves outside agents through OAuth with a
//! pairing code; the code is created with the `t3` CLI and spends itself on
//! this client alone, yielding a 30-day full-access credential stored with mode
//! 600. `login` deliberately performs no browser step: the pairing code *is*
//! the human approval.

use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jiff::Timestamp;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::config::{Config, valid_redirect_uri};
use crate::state::Credential;

pub fn login(config: &Config) -> Result<Credential> {
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build(),
    );
    let base = config.base_url()?;
    let redirect_uri = config
        .redirect_uri
        .clone()
        .unwrap_or_else(|| loopback_callback(&base));
    if !valid_redirect_uri(&redirect_uri) {
        bail!(
            "redirectUri must be http://127.0.0.1:<port> or http://localhost:<port>; \
             set it explicitly when baseUrl {base} carries no port"
        );
    }
    let verifier = random_token(32);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state_token = random_token(16);
    let client_id = register(&agent, &base, &redirect_uri)?;
    let pairing = pairing_code(&config.t3_cli_path)?;
    let authorization = json!({
        "response_type": "code",
        "client_id": client_id,
        "redirect_uri": redirect_uri,
        "code_challenge": challenge,
        "code_challenge_method": "S256",
        "state": state_token,
    });

    // The GET is what creates the pending request the approval page sees.
    let response = agent
        .get(format!("{base}/oauth/mcp/authorize"))
        .query_pairs([
            ("response_type", "code"),
            ("client_id", client_id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("state", state_token.as_str()),
        ])
        .call()
        .context("create T3 authorization request")?;
    response_text(response).context("T3 authorization request")?;

    let approval = post_json(
        &agent,
        &format!("{base}/oauth/mcp/approval"),
        &authorization,
    )?;
    let redirect = match approval.get("redirectTo").and_then(Value::as_str) {
        Some(redirect) => redirect.to_owned(),
        None => {
            // T3 requires full access before sending to full-access threads.
            let decision = json!({
                "authorization": authorization,
                "decision": { "_tag": "pairing-code", "access": "full-access", "code": pairing },
            });
            let decided = post_json(&agent, &format!("{base}/oauth/mcp/decision"), &decision)?;
            decided
                .get("redirectTo")
                .and_then(Value::as_str)
                .context("T3 did not answer the approval with a redirect")?
                .to_owned()
        }
    };
    let code = redirect
        .split_once('?')
        .and_then(|(_, query)| query_param(query, "code"))
        .with_context(|| {
            let reason = redirect
                .split_once('?')
                .and_then(|(_, query)| query_param(query, "error"))
                .unwrap_or_else(|| "no code".to_owned());
            format!("T3 denied the authorization ({reason})")
        })?;

    let response = agent
        .post(format!("{base}/oauth/mcp/token"))
        .send_form([
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("client_id", client_id.as_str()),
            ("code_verifier", verifier.as_str()),
        ])
        .context("request T3 token exchange")?;
    let token = response_json(response).context("T3 token exchange")?;
    let access_token = token
        .get("access_token")
        .and_then(Value::as_str)
        .context("token response has no access_token")?
        .to_owned();
    let expires_in = token
        .get("expires_in")
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_f64().map(|seconds| seconds as i64))
        })
        .filter(|seconds| *seconds > 0)
        .context("token response has no positive expires_in")?;
    Ok(Credential {
        access_token,
        expires_at: Timestamp::from_second(Timestamp::now().as_second() + expires_in)
            .context("T3 returned an out-of-range credential expiry")?,
        client_id,
        redirect_uri,
        scope: token
            .get("scope")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

/// The OAuth redirect is never opened: the pairing flow reads the code from
/// the redirect URL itself, so a loopback callback keeps the client working
/// whatever host the server binds. The port comes from the server origin.
fn loopback_callback(base: &str) -> String {
    let port = base
        .rsplit_once(':')
        .map(|(_, port)| port)
        .filter(|port| !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit()));
    match port {
        Some(port) => format!("http://127.0.0.1:{port}/callback"),
        None => format!("{base}/callback"),
    }
}

/// Registers this client and returns its id.
fn register(agent: &ureq::Agent, base: &str, redirect_uri: &str) -> Result<String> {
    let body = json!({
        "client_name": "t3-cache-warmer",
        "redirect_uris": [redirect_uri],
        "token_endpoint_auth_method": "none",
    });
    let response = post_json(agent, &format!("{base}/oauth/mcp/register"), &body)?;
    response
        .get("client_id")
        .and_then(Value::as_str)
        .context("registration response has no client_id")
        .map(str::to_owned)
}

/// Creates a pairing code with the `t3` CLI.
fn pairing_code(cli: &str) -> Result<String> {
    let output = Command::new(cli)
        .args([
            "auth",
            "pairing",
            "create",
            "--label",
            "t3-cache-warmer",
            "--json",
        ])
        .output()
        .with_context(|| format!("run `{cli} auth pairing create`"))?;
    if !output.status.success() {
        bail!(
            "`{cli} auth pairing create` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed = serde_json::from_str::<Value>(&stdout)
        .or_else(|_| {
            stdout
                .lines()
                .rev()
                .find_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
                .ok_or_else(|| serde_json::Error::io(std::io::Error::other("no JSON on stdout")))
        })
        .with_context(|| format!("parse `{cli} auth pairing create --json` output"))?;
    parsed
        .get("credential")
        .and_then(Value::as_str)
        .context("pairing output has no credential")
        .map(str::to_owned)
}

fn post_json(agent: &ureq::Agent, url: &str, body: &Value) -> Result<Value> {
    let response = agent
        .post(url)
        .header("content-type", "application/json")
        .send(body.to_string())
        .with_context(|| format!("POST {url}"))?;
    response_json(response).with_context(|| format!("POST {url}"))
}

fn response_text(mut response: ureq::http::Response<ureq::Body>) -> Result<String> {
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .read_to_string()
        .context("read T3 response body")?;
    if status >= 400 {
        bail!("T3 refused the request (HTTP {status}): {text}");
    }
    Ok(text)
}

fn response_json(response: ureq::http::Response<ureq::Body>) -> Result<Value> {
    serde_json::from_str(&response_text(response)?).context("parse T3 JSON response")
}

fn random_token(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).expect("operating system randomness");
    URL_SAFE_NO_PAD.encode(buffer)
}

fn query_param(query: &str, key: &str) -> Option<String> {
    form_urlencoded::parse(query.as_bytes())
        .find_map(|(name, value)| (name == key).then(|| value.into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_query_parameters() {
        let redirect =
            "http://127.0.0.1:47811/callback?code=abc-123&state=x%2Fy&iss=http%3A%2F%2Fx";
        assert_eq!(
            query_param(redirect.split_once('?').unwrap().1, "code").as_deref(),
            Some("abc-123")
        );
        assert_eq!(
            query_param(redirect.split_once('?').unwrap().1, "state").as_deref(),
            Some("x/y")
        );
        assert_eq!(query_param("a=1&b=2", "c"), None);
        assert_eq!(
            query_param("code=a%2Bb%3D", "code").as_deref(),
            Some("a+b=")
        );
        assert_eq!(query_param("code=a+b", "code").as_deref(), Some("a b"));
        assert_eq!(query_param("code=%E2%9C%93", "code").as_deref(), Some("✓"));
    }

    #[test]
    fn derives_a_loopback_callback_from_the_origin_port() {
        assert_eq!(
            loopback_callback("http://192.168.1.19:3773"),
            "http://127.0.0.1:3773/callback"
        );
        assert_eq!(
            loopback_callback("http://[::1]:3773"),
            "http://127.0.0.1:3773/callback"
        );
        assert_eq!(
            loopback_callback("http://127.0.0.1"),
            "http://127.0.0.1/callback",
            "a portless base cannot derive a loopback port and fails validation"
        );
    }

    #[test]
    fn rejects_failed_or_malformed_json_responses() {
        for (status, body, expected) in [
            (401, "credential rejected", "HTTP 401"),
            (500, "upstream unavailable", "upstream unavailable"),
            (200, "not JSON", "parse T3 JSON response"),
        ] {
            let response = ureq::http::Response::builder()
                .status(status)
                .body(ureq::Body::builder().data(body))
                .unwrap();
            let error = response_json(response).unwrap_err();
            assert!(format!("{error:#}").contains(expected), "{error:#}");
        }
    }

    #[test]
    fn preserves_body_read_errors() {
        struct Unreadable;
        impl std::io::Read for Unreadable {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("body read failed"))
            }
        }
        let response = ureq::http::Response::new(ureq::Body::builder().reader(Unreadable));
        let error = response_json(response).unwrap_err();
        assert!(
            format!("{error:#}").contains("body read failed"),
            "{error:#}"
        );
    }
}
