//! Minimal MCP client for the T3 server: initialize once, then `tools/call`.
//! Only what the warmer needs, no long-lived session bookkeeping beyond the
//! `mcp-session-id` header.

use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{json, Value};

pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// The 401 marker: the stored credential is no longer accepted.
#[derive(Debug)]
pub struct CredentialRejected;

impl std::fmt::Display for CredentialRejected {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "T3 rejected the MCP credential")
    }
}

impl std::error::Error for CredentialRejected {}

pub fn is_credential_rejected(error: &anyhow::Error) -> bool {
    error.downcast_ref::<CredentialRejected>().is_some()
}

#[derive(Debug, PartialEq, Eq)]
enum McpError {
    Credential(String),
    Session(String),
    Other(String),
}

impl McpError {
    fn into_anyhow(self) -> anyhow::Error {
        match self {
            Self::Credential(body) => anyhow::Error::new(CredentialRejected).context(body),
            Self::Session(message) => anyhow::anyhow!("MCP session invalid: {message}"),
            Self::Other(message) => anyhow::anyhow!("{message}"),
        }
    }
}

/// Maps an HTTP failure to the caller's recovery strategy: re-authenticate,
/// re-initialize the session, or give up. Getting this wrong is expensive: a
/// session-caused 401 would burn a pairing code on a full re-login instead of
/// a cheap re-`initialize`.
fn failure_for(status: u16, body: &str) -> McpError {
    if status == 401 {
        return McpError::Credential(body.to_owned());
    }
    if status == 404 || (status == 400 && body.contains("session")) {
        return McpError::Session(format!("HTTP {status}: {body}"));
    }
    McpError::Other(format!("HTTP {status} from /mcp: {body}"))
}

pub struct T3Client {
    agent: ureq::Agent,
    base: String,
    token: String,
    session: Option<String>,
    next_id: u64,
}

impl T3Client {
    pub fn new(base_url: &str, token: String) -> Self {
        let agent = ureq::Agent::new_with_config(
            ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(120)))
                .http_status_as_error(false)
                .build(),
        );
        Self {
            agent,
            base: base_url.trim_end_matches('/').to_owned(),
            token,
            session: None,
            next_id: 1,
        }
    }

    /// Calls an MCP tool and returns its decoded result, re-initializing the
    /// session once when the server forgot it.
    pub fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value> {
        match self.call_tool_once(name, arguments.clone()) {
            Err(McpError::Session(message)) => {
                self.session = None;
                self.call_tool_once(name, arguments)
                    .map_err(McpError::into_anyhow)
                    .with_context(|| format!("retried {name} after: {message}"))
            }
            other => other.map_err(McpError::into_anyhow),
        }
    }

    fn call_tool_once(&mut self, name: &str, arguments: Value) -> Result<Value, McpError> {
        let result = self.send_rpc(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )?;
        decode_tool_result(name, &result)
    }

    fn send_rpc(&mut self, method: &str, params: Value) -> Result<Value, McpError> {
        if method != "initialize" && self.session.is_none() {
            self.initialize()?;
        }
        let id = self.next_id;
        self.next_id += 1;
        let body = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let mut request = self
            .agent
            .post(format!("{}/mcp", self.base))
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .header("authorization", format!("Bearer {}", self.token));
        if let Some(session) = &self.session {
            request = request
                .header("mcp-session-id", session)
                .header("mcp-protocol-version", PROTOCOL_VERSION);
        }
        let mut response = request
            .send(body.to_string())
            .map_err(|error| McpError::Other(format!("POST /mcp: {error}")))?;
        let status = response.status().as_u16();
        let session_header = response
            .headers()
            .get("mcp-session-id")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let text = response.body_mut().read_to_string().unwrap_or_default();

        if !(200..300).contains(&status) {
            return Err(failure_for(status, &text));
        }
        if method == "initialize" {
            self.session = match session_header {
                Some(session) => Some(session),
                None => {
                    return Err(McpError::Session(
                        "server answered initialize without a session id".to_owned(),
                    ));
                }
            };
        }
        let payload = parse_rpc_body(&content_type, &text)?;
        if let Some(error) = payload.get("error") {
            let message = error.to_string();
            if message.contains("session") {
                return Err(McpError::Session(message));
            }
            return Err(McpError::Other(format!(
                "MCP error from {method}: {message}"
            )));
        }
        Ok(payload.get("result").cloned().unwrap_or(Value::Null))
    }

    fn initialize(&mut self) -> Result<(), McpError> {
        self.send_rpc(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "t3-cache-warmer", "version": env!("CARGO_PKG_VERSION") },
            }),
        )
        .map(|_| ())
    }

    pub fn project_list(&mut self) -> Result<Vec<Value>> {
        let result = self.call_tool("t3_project_list", json!({}))?;
        let projects = result
            .get("projects")
            .and_then(Value::as_array)
            .or_else(|| result.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(projects
            .into_iter()
            .filter(|project| {
                project
                    .get("deletedAt")
                    .is_none_or(|deleted| deleted.is_null())
            })
            .collect())
    }

    /// Lists live, non-snoozed threads of one project, following the cursor.
    pub fn thread_list(&mut self, project_id: &str) -> Result<Vec<Value>> {
        let mut threads = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..50 {
            let mut arguments = json!({
                "projectId": project_id,
                "settled": false,
                "snoozed": false,
                "includeSubagents": true,
                "limit": 100,
            });
            if let Some(cursor) = &cursor {
                arguments["cursor"] = json!(cursor);
            }
            let result = self.call_tool("t3_thread_list", arguments)?;
            let page = result
                .get("threads")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            threads.extend(page);
            cursor = result
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        Ok(threads)
    }

    pub fn thread_send(
        &mut self,
        thread_id: &str,
        message: &str,
        client_request_id: &str,
    ) -> Result<Value> {
        self.call_tool(
            "t3_thread_send",
            json!({
                "threadId": thread_id,
                "message": message,
                "mode": "auto",
                "clientRequestId": client_request_id,
            }),
        )
    }
}

/// Decodes an MCP tool result: `content[0].text` is JSON for T3's tools.
fn decode_tool_result(name: &str, result: &Value) -> Result<Value, McpError> {
    let text = result
        .pointer("/content/0/text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return Err(McpError::Other(format!("tool {name} failed: {text}")));
    }
    if text.is_empty() {
        return Ok(result.clone());
    }
    Ok(serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned())))
}

/// Handles both plain JSON and `text/event-stream` responses.
fn parse_rpc_body(content_type: &str, text: &str) -> Result<Value, McpError> {
    if content_type.contains("text/event-stream") {
        for line in text.lines().rev() {
            if let Some(data) = line.strip_prefix("data:")
                && let Ok(value) = serde_json::from_str::<Value>(data.trim())
            {
                return Ok(value);
            }
        }
        return Err(McpError::Other(format!(
            "no JSON in event stream: {}",
            truncate(text, 400)
        )));
    }
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(text).map_err(|error| {
        McpError::Other(format!(
            "invalid JSON from /mcp ({error}): {}",
            truncate(text, 400)
        ))
    })
}

fn truncate(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_json_tool_content() {
        let result = json!({
            "content": [{ "type": "text", "text": "{\"threads\": [], \"nextCursor\": null}" }],
        });
        let decoded = decode_tool_result("t3_thread_list", &result).unwrap();
        assert_eq!(decoded["threads"], json!([]));
    }

    #[test]
    fn surfaces_tool_failures() {
        let result = json!({
            "isError": true,
            "content": [{ "type": "text", "text": "OrchestratorMcpFailure { code: \"target_required\" }" }],
        });
        let error = decode_tool_result("t3_thread_list", &result).unwrap_err();
        assert!(error.into_anyhow().to_string().contains("target_required"));
    }

    #[test]
    fn parses_event_stream_responses() {
        let body =
            "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":true}}\n\n";
        let parsed = parse_rpc_body("text/event-stream", body).unwrap();
        assert_eq!(parsed["result"]["ok"], json!(true));
    }

    #[test]
    fn parses_plain_responses() {
        let parsed = parse_rpc_body("application/json", "{\"result\":1}").unwrap();
        assert_eq!(parsed["result"], json!(1));
        assert!(parse_rpc_body("application/json", "not json").is_err());
    }

    #[test]
    fn http_failures_map_to_recovery_strategy() {
        assert_eq!(
            failure_for(401, "invalid_mcp_credential"),
            McpError::Credential("invalid_mcp_credential".to_owned())
        );
        assert!(matches!(failure_for(404, "gone"), McpError::Session(_)));
        assert!(matches!(
            failure_for(400, "unknown session id"),
            McpError::Session(_)
        ));
        assert!(matches!(
            failure_for(400, "bad request"),
            McpError::Other(_)
        ));
        assert!(matches!(failure_for(500, "boom"), McpError::Other(_)));
    }
}
