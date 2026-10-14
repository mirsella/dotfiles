//! The one-time MCP sign-in. T3 approves outside agents through OAuth with a
//! pairing code; the code is created with the `t3` CLI and spends itself on
//! this client alone, so the result is a 30-day credential stored with mode
//! 600. `login` deliberately performs no browser step: the pairing code *is*
//! the human approval.

use std::process::Command;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use jiff::Timestamp;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::config::Config;
use crate::state::Credential;

const ACCESS: &str = "approval-required";

pub fn login(config: &Config) -> Result<Credential> {
    let agent = http_agent();
    let base = config.base_url.trim_end_matches('/');
    let verifier = random_token(32);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state_token = random_token(16);
    let client_id = register(&agent, base, &config.redirect_uri)?;
    let pairing = pairing_code(&config.t3_cli_path)?;
    let authorization = json!({
        "response_type": "code",
        "client_id": client_id,
        "redirect_uri": config.redirect_uri,
        "code_challenge": challenge,
        "code_challenge_method": "S256",
        "state": state_token,
    });

    // The GET is what creates the pending request the approval page sees.
    let query = authorization
        .as_object()
        .expect("object")
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                encode_component(key),
                encode_component(value.as_str().expect("string"))
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    let (status, body) = get(&agent, &format!("{base}/oauth/mcp/authorize?{query}"))?;
    if status >= 400 {
        bail!("T3 refused the authorization request (HTTP {status}): {body}");
    }

    let (status, approval) = post_json(
        &agent,
        &format!("{base}/oauth/mcp/approval"),
        &authorization,
    )?;
    if status >= 400 {
        bail!("T3 refused the approval request (HTTP {status}): {approval}");
    }
    let redirect = match approval.get("redirectTo").and_then(Value::as_str) {
        Some(redirect) => redirect.to_owned(),
        None => {
            let decision = json!({
                "authorization": authorization,
                "decision": { "_tag": "pairing-code", "access": ACCESS, "code": pairing },
            });
            let (status, decided) =
                post_json(&agent, &format!("{base}/oauth/mcp/decision"), &decision)?;
            if status >= 400 {
                bail!("T3 rejected the pairing code (HTTP {status}): {decided}");
            }
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

    let fields = [
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", config.redirect_uri.as_str()),
        ("client_id", client_id.as_str()),
        ("code_verifier", verifier.as_str()),
    ];
    let (status, token) = post_form(&agent, &format!("{base}/oauth/mcp/token"), &fields)?;
    if status >= 400 {
        bail!("T3 refused the token exchange (HTTP {status}): {token}");
    }
    let access_token = token
        .get("access_token")
        .and_then(Value::as_str)
        .context("token response has no access_token")?
        .to_owned();
    let expires_in = token
        .get("expires_in")
        .and_then(Value::as_i64)
        .context("token response has no expires_in")?;
    Ok(Credential {
        access_token,
        expires_at: Timestamp::from_second(Timestamp::now().as_second() + expires_in)
            .context("T3 returned an out-of-range credential expiry")?,
        client_id,
        redirect_uri: config.redirect_uri.clone(),
        scope: token
            .get("scope")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

/// Registers this client and returns its id.
fn register(agent: &ureq::Agent, base: &str, redirect_uri: &str) -> Result<String> {
    let body = json!({
        "client_name": "t3-cache-warmer",
        "redirect_uris": [redirect_uri],
        "token_endpoint_auth_method": "none",
    });
    let (status, response) = post_json(agent, &format!("{base}/oauth/mcp/register"), &body)?;
    if status >= 400 {
        bail!("T3 refused the client registration (HTTP {status}): {response}");
    }
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

pub fn http_agent() -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build(),
    )
}

fn get(agent: &ureq::Agent, url: &str) -> Result<(u16, String)> {
    let mut response = agent
        .get(url)
        .call()
        .with_context(|| format!("GET {url}"))?;
    let status = response.status().as_u16();
    let body = response.body_mut().read_to_string().unwrap_or_default();
    Ok((status, body))
}

fn post_json(agent: &ureq::Agent, url: &str, body: &Value) -> Result<(u16, Value)> {
    let mut response = agent
        .post(url)
        .header("content-type", "application/json")
        .send(body.to_string())
        .with_context(|| format!("POST {url}"))?;
    let status = response.status().as_u16();
    let text = response.body_mut().read_to_string().unwrap_or_default();
    Ok((status, parse_json(&text)))
}

fn post_form(agent: &ureq::Agent, url: &str, fields: &[(&str, &str)]) -> Result<(u16, Value)> {
    let body = fields
        .iter()
        .map(|(key, value)| format!("{}={}", encode_component(key), encode_component(value)))
        .collect::<Vec<_>>()
        .join("&");
    let mut response = agent
        .post(url)
        .header("content-type", "application/x-www-form-urlencoded")
        .send(body)
        .with_context(|| format!("POST {url}"))?;
    let status = response.status().as_u16();
    let text = response.body_mut().read_to_string().unwrap_or_default();
    Ok((status, parse_json(&text)))
}

fn parse_json(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned()))
}

fn random_token(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).expect("operating system randomness");
    URL_SAFE_NO_PAD.encode(buffer)
}

fn encode_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (pair_key, pair_value) = pair.split_once('=')?;
        (decode_component(pair_key) == key).then(|| decode_component(pair_value))
    })
}

fn decode_component(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        decoded.push(byte);
                        index += 3;
                    }
                    Err(_) => {
                        decoded.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_roundtrips() {
        let value = "http://127.0.0.1:47811/callback?a=b c";
        let encoded = encode_component(value);
        assert!(!encoded.contains(':'));
        assert_eq!(decode_component(&encoded), value);
    }

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
    }
}
