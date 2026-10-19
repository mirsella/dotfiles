//! Minimal MCP client for the T3 server: initialize once, then `tools/call`.
//! Only what the warmer needs, no long-lived session bookkeeping beyond the
//! `mcp-session-id` header.

use std::time::Duration;

use anyhow::{Context, Result, ensure};
use jiff::Timestamp;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// The fields needed to observe and warm a listed thread.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadView {
    #[serde(rename = "threadId")]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub settled: bool,
    #[serde(default)]
    pub snoozed: bool,
    pub status: String,
    #[serde(rename = "providerInstanceId")]
    pub provider: String,
    pub model: String,
    pub updated_at: Timestamp,
    // Require the field; only an explicit null means the thread has no runs.
    #[serde(deserialize_with = "Option::deserialize")]
    pub latest_run_id: Option<String>,
    #[serde(
        default,
        rename = "relationshipToParent",
        deserialize_with = "is_subagent"
    )]
    pub subagent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletedRun {
    pub run_id: String,
    pub completed_at: Timestamp,
}

fn is_subagent<'de, D: serde::Deserializer<'de>>(value: D) -> Result<bool, D::Error> {
    Ok(Option::<String>::deserialize(value)?.as_deref() == Some("subagent"))
}

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
                // These calls only list local state or enqueue a turn. Waiting
                // two minutes can consume another thread's entire refresh lead.
                .timeout_connect(Some(Duration::from_secs(3)))
                .timeout_global(Some(Duration::from_secs(10)))
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
    fn call_tool(&mut self, name: &str, arguments: &Value) -> Result<Value> {
        match self.call_tool_once(name, arguments) {
            Err(McpError::Session(message)) => {
                self.session = None;
                self.call_tool_once(name, arguments)
                    .map_err(McpError::into_anyhow)
                    .with_context(|| format!("retried {name} after: {message}"))
            }
            other => other.map_err(McpError::into_anyhow),
        }
    }

    fn call_tool_once(&mut self, name: &str, arguments: &Value) -> Result<Value, McpError> {
        let result = self.send_rpc(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )?;
        decode_tool_result(name, result)
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
        let text = response.body_mut().read_to_string().map_err(|error| {
            McpError::Other(format!("read /mcp response (HTTP {status}): {error}"))
        })?;

        if !(200..300).contains(&status) {
            return Err(failure_for(status, &text));
        }
        let mut payload = parse_rpc_body(&content_type, &text)?;
        if payload["jsonrpc"] != "2.0" || payload["id"] != id {
            return Err(McpError::Other(format!(
                "invalid JSON-RPC response to {method}: {}",
                truncate(&text, 400)
            )));
        }
        if let Some(error) = payload.get("error") {
            let message = error.to_string();
            if message.contains("session") {
                return Err(McpError::Session(message));
            }
            return Err(McpError::Other(format!(
                "MCP error from {method}: {message}"
            )));
        }
        let result = payload.get_mut("result").map(Value::take).ok_or_else(|| {
            McpError::Other(format!("missing result in JSON-RPC response to {method}"))
        })?;
        if method == "initialize" {
            self.session = Some(session_header.ok_or_else(|| {
                McpError::Session("server answered initialize without a session id".to_owned())
            })?);
        }
        Ok(result)
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

    pub fn project_ids(&mut self) -> Result<Vec<String>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Project {
            id: String,
            deleted_at: Option<Timestamp>,
        }
        let projects: Vec<Project> = self.list("t3_project_list", "projects", json!({}))?;
        let ids: Vec<_> = projects
            .into_iter()
            .filter(|project| project.deleted_at.is_none())
            .map(|project| project.id)
            .collect();
        ensure!(!ids.is_empty(), "T3 reported no projects");
        Ok(ids)
    }

    /// Lists live, non-snoozed threads of one project, following the cursor.
    pub fn thread_list(&mut self, project_id: &str) -> Result<Vec<ThreadView>> {
        self.list(
            "t3_thread_list",
            "threads",
            json!({
                "projectId": project_id,
                "settled": false,
                "snoozed": false,
                "includeSubagents": true,
                "limit": 100,
            }),
        )
    }

    pub fn completed_run(&mut self, thread_id: &str, run_id: &str) -> Result<CompletedRun> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Snapshot {
            recent_runs: Vec<CompletedRun>,
        }
        let snapshot: Snapshot = serde_json::from_value(self.call_tool(
            "t3_thread_read",
            &json!({
                "threadId": thread_id,
                "runLimit": 1,
                "limit": 1,
                "maxCharsPerItem": 256,
            }),
        )?)
        .context("invalid run history from t3_thread_read")?;
        snapshot
            .recent_runs
            .into_iter()
            .find(|run| run.run_id == run_id)
            .context("latest run changed or missing from t3_thread_read; retry next pass")
    }

    fn list<T: DeserializeOwned>(
        &mut self,
        name: &str,
        field: &str,
        mut arguments: Value,
    ) -> Result<Vec<T>> {
        let mut items = Vec::new();
        for _ in 0..50 {
            let mut result = self.call_tool(name, &arguments)?;
            let page = result
                .get_mut(field)
                .with_context(|| format!("{name} returned no {field} array"))?
                .take();
            // Parse the whole page before returning it: malformed listings must
            // never look like disappeared threads and erase their saved state.
            items.extend(
                serde_json::from_value::<Vec<T>>(page)
                    .with_context(|| format!("invalid {field} from {name}"))?,
            );
            let cursor: Option<String> = serde_json::from_value(
                result
                    .get_mut("nextCursor")
                    .map(Value::take)
                    .unwrap_or(Value::Null),
            )
            .with_context(|| format!("invalid nextCursor from {name}"))?;
            let Some(cursor) = cursor else {
                return Ok(items);
            };
            ensure!(!cursor.is_empty(), "empty nextCursor from {name}");
            arguments["cursor"] = json!(cursor);
        }
        anyhow::bail!("{name} exceeded 50 pages")
    }

    pub fn thread_send(
        &mut self,
        thread_id: &str,
        message: &str,
        client_request_id: &str,
    ) -> Result<Value> {
        self.call_tool(
            "t3_thread_send",
            &json!({
                "threadId": thread_id,
                "message": message,
                "mode": "auto",
                "clientRequestId": client_request_id,
            }),
        )
    }
}

/// Decodes an MCP tool result: `content[0].text` is JSON for T3's tools.
fn decode_tool_result(name: &str, mut result: Value) -> Result<Value, McpError> {
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return Err(McpError::Other(format!(
            "tool {name} failed: {}",
            truncate(&result.to_string(), 400)
        )));
    }
    let value = if let Some(value) = result.get_mut("structuredContent") {
        value.take()
    } else {
        let text = result
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                McpError::Other(format!(
                    "tool {name} returned no structured content or JSON text"
                ))
            })?;
        serde_json::from_str::<Value>(text)
            .map_err(|error| McpError::Other(format!("invalid JSON from tool {name}: {error}")))?
    };
    if !value.is_object() {
        return Err(McpError::Other(format!(
            "tool {name} returned a non-object result"
        )));
    }
    Ok(value)
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
pub(crate) mod tests {
    use super::*;

    /// A local MCP server that checks request order and can lose a reply.
    pub(crate) fn mock_server(
        expected: &'static [&'static str],
        mut reply: impl FnMut(&str, &Value) -> Option<Value> + Send + 'static,
    ) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for &name in expected {
                let (socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(socket);
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    assert_ne!(
                        reader.read_line(&mut line).unwrap(),
                        0,
                        "incomplete request headers"
                    );
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                }
                let mut bytes = vec![0; length];
                reader.read_exact(&mut bytes).unwrap();
                let request: Value = serde_json::from_slice(&bytes).unwrap();
                let result = if name == "initialize" {
                    assert_eq!(request["method"], name);
                    json!({})
                } else {
                    assert_eq!(request["params"]["name"], name);
                    let Some(value) = reply(name, &request["params"]["arguments"]) else {
                        continue;
                    };
                    json!({ "content": [{ "type": "text", "text": value.to_string() }] })
                };
                let body =
                    json!({ "jsonrpc": "2.0", "id": request["id"], "result": result }).to_string();
                write!(reader.get_mut(), "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nmcp-session-id: test\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        (base, server)
    }

    fn listed_thread() -> Value {
        json!({
            "threadId": "thread-1", "title": "Test", "status": "completed",
            "providerInstanceId": "opencode", "model": "openai/gpt-6-astra",
            "latestRunId": null,
            "updatedAt": "2026-10-14T09:00:00Z"
        })
    }

    #[test]
    fn parses_threads_and_subagent_relationships() {
        let mut raw = listed_thread();
        let view: ThreadView = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(view.id, "thread-1");
        assert_eq!(view.provider, "opencode");
        assert!(!view.subagent);
        for (relationship, subagent) in [
            (Value::Null, false),
            (json!("subagent"), true),
            (json!("fork"), false),
        ] {
            raw["relationshipToParent"] = relationship;
            assert_eq!(
                serde_json::from_value::<ThreadView>(raw.clone())
                    .unwrap()
                    .subagent,
                subagent
            );
        }
    }

    #[test]
    fn reads_actual_run_completion_and_rejects_races_or_missing_history() {
        let completed: Timestamp = "2026-10-19T14:37:59.204Z".parse().unwrap();
        let (base, server) = mock_server(
            &[
                "initialize",
                "t3_thread_read",
                "t3_thread_read",
                "t3_thread_read",
                "t3_thread_read",
            ],
            move |_, arguments| {
                assert_eq!(arguments["runLimit"], 1);
                assert_eq!(arguments["limit"], 1);
                Some(match arguments["threadId"].as_str().unwrap() {
                    "completed" => json!({ "recentRuns": [{
                        "runId": "run-1", "completedAt": completed.to_string()
                    }] }),
                    "running" => json!({ "recentRuns": [{
                        "runId": "run-1", "completedAt": null
                    }] }),
                    "changed" => json!({ "recentRuns": [{
                        "runId": "run-2", "completedAt": completed.to_string()
                    }] }),
                    "missing" => json!({ "recentRuns": [] }),
                    other => panic!("unexpected thread {other}"),
                })
            },
        );
        let mut client = T3Client::new(&base, "test".to_owned());
        assert_eq!(
            client
                .completed_run("completed", "run-1")
                .unwrap()
                .completed_at,
            completed
        );
        for id in ["running", "changed", "missing"] {
            assert!(client.completed_run(id, "run-1").is_err(), "{id}");
        }
        server.join().unwrap();
    }

    #[test]
    fn rejects_threads_with_missing_or_malformed_warming_metadata() {
        for field in [
            "threadId",
            "status",
            "providerInstanceId",
            "model",
            "updatedAt",
            "latestRunId",
        ] {
            let mut raw = listed_thread();
            raw.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<ThreadView>(raw).is_err(),
                "missing {field}"
            );
        }
        for (field, value) in [
            ("updatedAt", json!("invalid")),
            ("latestRunId", json!(42)),
            ("settled", json!("false")),
            ("relationshipToParent", json!(true)),
        ] {
            let mut raw = listed_thread();
            raw[field] = value;
            assert!(
                serde_json::from_value::<ThreadView>(raw).is_err(),
                "malformed {field}"
            );
        }
    }

    #[test]
    fn paginates_projects_and_excludes_deleted_ones() {
        let (base, server) = mock_server(
            &["initialize", "t3_project_list", "t3_project_list"],
            |_, arguments| {
                if arguments.get("cursor").is_none() {
                    Some(
                        json!({ "projects": [{"id": "first"}, {"id": "deleted", "deletedAt": "2026-10-14T09:00:00Z"}], "nextCursor": "second-page" }),
                    )
                } else {
                    assert_eq!(arguments["cursor"], "second-page");
                    Some(json!({ "projects": [{"id": "second"}], "nextCursor": null }))
                }
            },
        );
        let mut client = T3Client::new(&base, "test".to_owned());
        assert_eq!(client.project_ids().unwrap(), ["first", "second"]);
        server.join().unwrap();
    }

    #[test]
    fn rejects_invalid_cursors_instead_of_returning_partial_results() {
        let (base, server) = mock_server(&["initialize", "t3_project_list"], |_, _| {
            Some(json!({ "projects": [{"id": "first"}], "nextCursor": 42 }))
        });
        let error = T3Client::new(&base, "test".to_owned())
            .project_ids()
            .unwrap_err();
        assert!(
            error.to_string().contains("invalid nextCursor"),
            "{error:#}"
        );
        server.join().unwrap();
    }

    #[test]
    fn rejects_missing_or_invalid_tool_acknowledgements() {
        for result in [
            Value::Null,
            json!({}),
            json!({ "structuredContent": null }),
            json!({ "content": [] }),
            json!({ "content": [{ "text": "" }] }),
            json!({ "content": [{ "text": "ok" }] }),
            json!({ "content": [{ "text": "null" }] }),
        ] {
            assert!(
                decode_tool_result("t3_thread_send", result.clone()).is_err(),
                "{result}"
            );
        }
        let structured = json!({ "structuredContent": { "threads": [] }, "content": [{ "text": "unparsed text" }] });
        assert_eq!(
            decode_tool_result("t3_thread_list", structured).unwrap(),
            json!({ "threads": [] })
        );
    }

    #[test]
    fn decodes_json_tool_content() {
        let result = json!({
            "content": [{ "type": "text", "text": "{\"threads\": [], \"nextCursor\": null}" }],
        });
        let decoded = decode_tool_result("t3_thread_list", result).unwrap();
        assert_eq!(decoded["threads"], json!([]));
    }

    #[test]
    fn surfaces_tool_failures() {
        let result = json!({
            "isError": true,
            "content": [{ "type": "text", "text": "OrchestratorMcpFailure { code: \"target_required\" }" }],
        });
        let error = decode_tool_result("t3_thread_list", result).unwrap_err();
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
        assert!(parse_rpc_body("application/json", "").is_err());
    }

    #[test]
    fn reports_response_body_timeouts_instead_of_accepting_empty_results() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (finished, finish) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            socket.read(&mut [0; 4096]).unwrap();
            // Announce a body, send only one byte, and hold the connection
            // until the client has timed out reading the rest.
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n{").unwrap();
            let _ = finish.recv_timeout(Duration::from_secs(2));
        });
        let mut client = T3Client::new(&base, "test".to_owned());
        client.session = Some("test".to_owned());
        client.agent = ureq::Agent::new_with_config(
            ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_millis(100)))
                .build(),
        );
        let error = client.call_tool("t3_project_list", &json!({})).unwrap_err();
        finished.send(()).unwrap();
        server.join().unwrap();
        assert!(
            error.to_string().contains("read /mcp response (HTTP 200)"),
            "{error:#}"
        );
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
