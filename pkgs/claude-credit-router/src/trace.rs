//! A bounded metadata tee. It never changes, retries, buffers the full stream,
//! or publishes prompt/output text. Native subscription bodies are not parsed.
use crate::audit::{Audit, ContextIds, Record, Tokens};
use crate::policy::Route;
use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, Response};
use futures_util::{Stream, StreamExt};
use serde_json::Value;
use std::time::Instant;

pub const HEADERS: [&str; 5] = [
    "x-credit-request-id",
    "x-credit-session-id",
    "x-credit-message-id",
    "x-credit-agent",
    "x-credit-kind",
];

pub fn context(headers: &HeaderMap) -> ContextIds {
    let value = |name| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .and_then(identifier)
    };
    ContextIds {
        request_id: value(HEADERS[0]),
        session_id: value(HEADERS[1]),
        message_id: value(HEADERS[2]),
        agent: value(HEADERS[3]),
        kind: value(HEADERS[4]),
        ..ContextIds::default()
    }
}

fn identifier(value: &str) -> Option<String> {
    (!value.is_empty()
        && value.len() <= 200
        && !value.starts_with("sk-")
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-.:/".contains(c)))
    .then(|| value.into())
}

pub fn request_context(context: &mut ContextIds, bytes: &[u8]) {
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return;
    };
    context.model = value["model"].as_str().and_then(identifier);
    // Claude Code's metadata contains its own UUID, not an OpenCode session ID.
    // Retain only that UUID, never account/user identity from the same field.
    if let Some(user) = value["metadata"]["user_id"].as_str() {
        let decoded = serde_json::from_str::<Value>(user).ok();
        let id = decoded
            .as_ref()
            .and_then(|value| value["session_id"].as_str())
            .or_else(|| user.rsplit_once("_session_").map(|(_, id)| id));
        if let Some(id) =
            id.filter(|id| id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
        {
            context.native_session_id = Some(id.into());
        }
    }
}

pub struct Trace {
    audit: Audit,
    record: Record,
    started: Instant,
    complete: bool,
    metadata: Metadata,
    credit_cut: Option<Box<dyn FnOnce(&ContextIds) + Send + Sync>>,
}

impl Trace {
    pub fn start(audit: Audit, context: ContextIds, route: Route, reason: &str) -> Self {
        let mut record = Record::new("request_started", "router");
        record.context = context;
        record.route = Some(route);
        record.reason = Some(reason.into());
        audit.emit(&record);
        record.id.push_str(":end");
        record.event = "request_finished".into();
        Self {
            audit,
            record,
            started: Instant::now(),
            complete: false,
            metadata: Metadata::default(),
            credit_cut: None,
        }
    }

    pub fn response(&mut self, response: &reqwest::Response) {
        self.record.status = Some(response.status().as_u16());
        self.record.context.upstream_request_id = response
            .headers()
            .get("request-id")
            .or_else(|| response.headers().get("x-request-id"))
            .and_then(|value| value.to_str().ok())
            .and_then(identifier);
        self.metadata.sse = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("text/event-stream"));
    }

    /// Called once if the vendor ends this promotional stream with its balance
    /// error. The bytes still reach the client unchanged.
    pub fn on_credit_cut(&mut self, action: impl FnOnce(&ContextIds) + Send + Sync + 'static) {
        self.credit_cut = Some(Box::new(action));
    }

    pub fn finish(&mut self, outcome: &str) {
        if self.complete {
            return;
        }
        self.complete = true;
        self.record.at = chrono::Utc::now();
        self.record.duration_ms =
            Some(self.started.elapsed().as_millis().min(u64::MAX as u128) as u64);
        self.record.outcome = Some(outcome.into());
        if !self.metadata.final_usage {
            self.record.tokens = None;
        }
        self.audit.emit(&self.record);
    }

    pub fn stream(mut self, response: reqwest::Response, parse_usage: bool) -> Response<Body> {
        self.response(&response);
        let code = response.status();
        let headers = response.headers().clone();
        crate::reply(
            code,
            headers,
            self.body(response.bytes_stream(), parse_usage),
        )
    }

    pub fn body<S>(self, stream: S, parse_usage: bool) -> Body
    where
        S: Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
    {
        let stream = Box::pin(stream);
        Body::from_stream(futures_util::stream::unfold(
            (stream, self),
            move |(mut stream, mut trace)| async move {
                match stream.next().await {
                    Some(Ok(chunk)) => {
                        if parse_usage {
                            trace.metadata.push(&chunk, &mut trace.record);
                        }
                        if trace.metadata.credit_cut {
                            if let Some(action) = trace.credit_cut.take() {
                                action(&trace.record.context);
                            }
                            trace.finish("credit_exhausted_mid_stream");
                        }
                        Some((Ok::<_, reqwest::Error>(chunk), (stream, trace)))
                    }
                    Some(Err(error)) => {
                        trace.finish("stream_error");
                        Some((Err(error), (stream, trace)))
                    }
                    None => {
                        if parse_usage {
                            trace.metadata.end(&mut trace.record);
                        }
                        trace.finish(if trace.metadata.error {
                            "upstream_stream_error"
                        } else {
                            "complete"
                        });
                        None
                    }
                }
            },
        ))
    }
}

impl Drop for Trace {
    fn drop(&mut self) {
        self.finish("cancelled_or_transport_error");
    }
}

#[derive(Default)]
pub struct Metadata {
    sse: bool,
    pending: Vec<u8>,
    discard: bool,
    pub error: bool,
    pub credit_cut: bool,
    tokens: Tokens,
    input_seen: bool,
    final_usage: bool,
}

impl Metadata {
    pub fn sse() -> Self {
        Self {
            sse: true,
            ..Self::default()
        }
    }

    pub fn push(&mut self, bytes: &[u8], record: &mut Record) {
        for byte in bytes {
            if self.sse && *byte == b'\n' {
                if !self.discard {
                    self.parse(record);
                }
                self.pending.clear();
                self.discard = false;
            } else if !self.discard {
                if self.pending.len() >= 65_536 {
                    self.pending.clear();
                    self.discard = true;
                } else {
                    self.pending.push(*byte);
                }
            }
        }
    }

    pub fn end(&mut self, record: &mut Record) {
        if !self.discard {
            self.parse(record);
        }
        self.pending.clear();
    }

    fn parse(&mut self, record: &mut Record) {
        let bytes = if self.sse {
            self.pending.strip_prefix(b"data:").unwrap_or_default()
        } else {
            &self.pending
        };
        let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
            return;
        };
        if matches!(
            value["type"].as_str(),
            Some("error" | "response.failed" | "response.incomplete")
        ) {
            self.error = true;
            self.credit_cut |= crate::exhausted_error(&value);
        }
        let (object, openai) = if value.get("response").is_some() {
            (&value["response"], true)
        } else if value.get("message").is_some() {
            (&value["message"], false)
        } else {
            (
                &value,
                value.get("object").is_some()
                    || value["type"]
                        .as_str()
                        .is_some_and(|kind| kind.starts_with("response.")),
            )
        };
        if let Some(model) = object["model"].as_str().and_then(identifier) {
            record.context.model = Some(model);
        }
        if let Some(id) = object["id"].as_str().and_then(identifier) {
            record.context.response_id = Some(id);
        }
        if object["service_tier"]
            .as_str()
            .is_some_and(|tier| !matches!(tier, "default" | "auto"))
        {
            record.cost_unavailable_reason =
                Some("nonstandard_service_tier_price_not_captured".into());
        }
        let Some(usage) = object.get("usage").filter(|usage| usage.is_object()) else {
            return;
        };
        if usage["service_tier"]
            .as_str()
            .is_some_and(|tier| !matches!(tier, "standard" | "default" | "auto"))
        {
            record.cost_unavailable_reason =
                Some("nonstandard_service_tier_price_not_captured".into());
        }
        let tokens = &mut self.tokens;
        if openai {
            if let Some(input) = usage["input_tokens"].as_u64() {
                self.input_seen = true;
                tokens.cache_read = usage["input_tokens_details"]["cached_tokens"]
                    .as_u64()
                    .unwrap_or_default();
                tokens.input = input.saturating_sub(tokens.cache_read);
            }
            if let Some(output) = usage["output_tokens"].as_u64() {
                tokens.reasoning = usage["output_tokens_details"]["reasoning_tokens"]
                    .as_u64()
                    .unwrap_or_default();
                tokens.output = output.saturating_sub(tokens.reasoning);
                self.final_usage = self.input_seen;
            }
        } else {
            if usage["cache_creation"]["ephemeral_1h_input_tokens"]
                .as_u64()
                .is_some_and(|tokens| tokens > 0)
            {
                record.cost_unavailable_reason =
                    Some("one_hour_cache_write_price_not_captured".into());
            }
            if let Some(input) = usage["input_tokens"].as_u64() {
                tokens.input = input;
                self.input_seen = true;
            }
            if let Some(output) = usage["output_tokens"].as_u64() {
                tokens.output = output;
                self.final_usage =
                    self.input_seen && (value["type"] == "message_delta" || !self.sse);
            }
            if let Some(read) = usage["cache_read_input_tokens"].as_u64() {
                tokens.cache_read = read;
            }
            if let Some(write) = usage["cache_creation_input_tokens"].as_u64() {
                tokens.cache_write = write;
            }
        }
        if self.final_usage {
            record.tokens = Some(tokens.clone());
        }
    }
}
