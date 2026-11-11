pub mod audit;
pub mod openai;
pub mod policy;
pub mod spending;
pub mod trace;

use anyhow::{Context, Result, ensure};
use axum::body::{Body, Bytes, to_bytes};
use axum::extract::State;
use axum::http::{
    HeaderMap, HeaderValue, Method, Request, Response, StatusCode, Uri, request::Parts,
};
use axum::routing::get;
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use policy::{Mode, Route, Store};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

pub const EXHAUSTED: &str = "Your credit balance is too low to access the Anthropic API. Please go to Plans & Billing to upgrade or purchase credits.";

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub listen: SocketAddr,
    pub upstream: String,
    pub api_key_file: Option<PathBuf>,
    pub organization_id: String,
    pub renewal_anchor: DateTime<Utc>,
    pub billing: Option<Billing>,
    pub openai: Option<openai::Settings>,
    pub notify_send: Option<PathBuf>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Billing {
    pub verified_at: DateTime<Utc>,
    pub no_payment_method: bool,
    pub purchased_balance_usd: u32,
    pub auto_reload: bool,
    pub sales_invoicing: bool,
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.listen.ip().is_loopback(),
            "router must bind a loopback address"
        );
        let upstream = reqwest::Url::parse(&self.upstream)?;
        ensure!(
            upstream.scheme() == "http"
                && upstream.host_str().is_some_and(|host| host == "localhost"
                    || host
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())),
            "upstream must be the local Sleev gateway"
        );
        ensure!(
            upstream.username().is_empty()
                && upstream.password().is_none()
                && upstream.query().is_none()
                && upstream.fragment().is_none(),
            "upstream cannot contain credentials, a query or a fragment"
        );
        ensure!(
            !self.organization_id.is_empty(),
            "expected promotional organization is required"
        );
        HeaderValue::from_str(&self.organization_id).context("invalid organization header")?;
        if let Some(openai) = &self.openai {
            ensure!(
                self.billing.is_none(),
                "OpenAI shared billing cannot be labelled promotional-only"
            );
            openai.validate()?;
        } else {
            let billing = self
                .billing
                .as_ref()
                .context("Claude billing verification is required")?;
            ensure!(
                billing.no_payment_method
                    && billing.purchased_balance_usd == 0
                    && !billing.auto_reload
                    && !billing.sales_invoicing,
                "API routing requires a verified promotional-only organization: no card, purchased credits, auto-reload or invoicing"
            );
            ensure!(
                billing.verified_at <= Utc::now(),
                "billing verification cannot be in the future"
            );
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct Router {
    pub config: Config,
    pub store: Store,
    client: reqwest::Client,
    pub spending: spending::Guard,
    key: HeaderValue,
    // Serializes only promotional attempts until their response headers/error
    // arrive. Streaming output and all subscription traffic remain concurrent.
    promotional: Arc<Mutex<()>>,
    models: Arc<std::sync::RwLock<std::collections::HashSet<String>>>,
    interrupted: Arc<std::sync::Mutex<Vec<Interruption>>>,
    clock: Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>,
}

/// A promotional response the vendor ended mid-stream because the balance ran
/// out. The stream is forwarded as-is; this only lets the client that owns the
/// session decide to send the turn again.
#[derive(Clone, Debug, Serialize)]
pub struct Interruption {
    pub at: DateTime<Utc>,
    pub native_session_id: Option<String>,
}

const INTERRUPTION_RETENTION: chrono::Duration = chrono::Duration::minutes(10);

#[derive(Serialize)]
pub struct Status {
    pub route: Route,
    pub active: Route,
    pub mode: Mode,
    pub organization_id: String,
    pub next_probe_at: Option<DateTime<Utc>>,
    pub history: Vec<policy::Event>,
    pub api_model_count: Option<usize>,
    pub last_promotional_success_at: Option<DateTime<Utc>>,
    pub spending: Option<spending::Snapshot>,
    pub local_spend_limit_usd: Option<u32>,
    pub interrupted: Vec<Interruption>,
}

impl Router {
    pub fn new(config: Config, store: Store, key: &str) -> Result<Self> {
        config.validate()?;
        store.read()?;
        let mut key = HeaderValue::from_str(key).context("invalid API key header")?;
        key.set_sensitive(true);
        Ok(Self {
            spending: spending::Guard::new()?,
            config,
            store,
            key,
            client: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            promotional: Arc::new(Mutex::new(())),
            models: Arc::default(),
            interrupted: Arc::default(),
            clock: Arc::new(Utc::now),
        })
    }

    pub fn with_clock(mut self, clock: impl Fn() -> DateTime<Utc> + Send + Sync + 'static) -> Self {
        self.clock = Arc::new(clock);
        self
    }

    pub fn status(&self, now: DateTime<Utc>) -> Result<Status> {
        if self.config.openai.is_some() && !openai::window_open(self.config.renewal_anchor, now) {
            let changed = self.store.update(|state| {
                Ok(state.defer(
                    now,
                    policy::next_renewal(self.config.renewal_anchor, now)?,
                    "calendar_reset_guard",
                ))
            })?;
            if changed {
                self.notify("Using native Codex until the grant-cycle window opens; the project calendar reset does not renew promotional credits.");
            }
        }
        let state = self.store.read()?;
        Ok(Status {
            route: state.route(now),
            spending: state.spending,
            local_spend_limit_usd: self.config.openai.as_ref().map(|s| s.spending.limit_usd),
            active: state.active,
            mode: state.mode,
            organization_id: self.config.organization_id.clone(),
            next_probe_at: state.next_probe_at,
            history: state.history,
            api_model_count: self.config.openai.as_ref().map(|_| {
                self.models
                    .read()
                    .expect("model inventory lock poisoned")
                    .len()
            }),
            last_promotional_success_at: state.last_promotional_success_at,
            interrupted: self.interruptions(now),
        })
    }

    fn interruptions(&self, now: DateTime<Utc>) -> Vec<Interruption> {
        let mut interrupted = self.interrupted.lock().expect("interruption lock poisoned");
        interrupted.retain(|entry| now - entry.at < INTERRUPTION_RETENTION);
        interrupted.clone()
    }

    fn interrupt(&self, native_session_id: Option<String>) {
        let now = (self.clock)();
        eprintln!(
            "credit-router: promotional credits ran out during a response; the next request is checked and uses your normal subscription"
        );
        let mut interrupted = self.interrupted.lock().expect("interruption lock poisoned");
        interrupted.retain(|entry| now - entry.at < INTERRUPTION_RETENTION);
        // One exhaustion cuts at most the streams in flight; this bound only
        // keeps a misbehaving upstream from growing the list.
        if interrupted.len() < 64 {
            interrupted.push(Interruption {
                at: now,
                native_session_id,
            });
        }
    }

    pub async fn check_spending(&self, fresh: bool) -> Result<bool> {
        if self.config.openai.is_none() {
            return Ok(true);
        }
        let was_promotional = self.store.read()?.active == Route::Promotional;
        let allowed = self
            .spending
            .check(&self.config, &self.store, self.clock.as_ref(), fresh)
            .await?;
        if !allowed && was_promotional {
            self.notify("Local API spend check stopped promotional usage; using your normal subscription. See openai-credit-router status for the reason.");
        }
        Ok(allowed)
    }

    // Free metadata request: validate the key's organization before it is ever
    // allowed to send inference. There is no inference/billing probe at startup.
    pub async fn verify_organization(&self) -> Result<()> {
        if let Some(settings) = &self.config.openai {
            let models = settings
                .verify(&self.client, self.key.clone(), &self.config.organization_id)
                .await?;
            *self.models.write().expect("model inventory lock poisoned") = models;
            return Ok(());
        }
        let response = self
            .client
            .get(format!(
                "{}/v1/models?limit=1",
                self.config.upstream.trim_end_matches('/')
            ))
            .header("x-api-key", self.key.clone())
            .header("anthropic-version", "2023-06-01")
            .send()
            .await?;
        ensure!(
            response.status().is_success(),
            "non-billable organization check returned {}",
            response.status()
        );
        let organization = response
            .headers()
            .get("anthropic-organization-id")
            .and_then(|v| v.to_str().ok());
        ensure!(
            organization == Some(self.config.organization_id.as_str()),
            "API key organization was not verified; refusing promotional inference"
        );
        Ok(())
    }

    pub fn app(self) -> axum::Router {
        axum::Router::new()
            .route("/_credit/status", get(status))
            .fallback(forward)
            .with_state(Arc::new(self))
    }

    fn request(
        &self,
        method: Method,
        uri: &Uri,
        mut headers: HeaderMap,
        body: impl Into<reqwest::Body>,
        route: Route,
    ) -> reqwest::RequestBuilder {
        strip_hop_headers(&mut headers);
        headers.remove("host");
        for name in trace::HEADERS {
            headers.remove(name);
        }
        if route == Route::Promotional {
            headers.remove("authorization");
            headers.remove("cookie");
            if let Some(settings) = &self.config.openai {
                headers.remove("chatgpt-account-id");
                headers.remove("x-api-key");
                let mut bearer = HeaderValue::from_str(&format!(
                    "Bearer {}",
                    self.key.to_str().expect("validated key")
                ))
                .expect("validated key");
                bearer.set_sensitive(true);
                headers.insert("authorization", bearer);
                headers.insert(
                    "openai-organization",
                    HeaderValue::from_str(&self.config.organization_id)
                        .expect("validated organization"),
                );
                headers.insert(
                    "openai-project",
                    HeaderValue::from_str(&settings.project_id).expect("validated project"),
                );
            } else {
                headers.insert("x-api-key", self.key.clone());
            }
            headers.insert("accept-encoding", HeaderValue::from_static("identity"));
            // This beta opts into OAuth authentication, not a model capability.
            if let Some(beta) = headers.get("anthropic-beta").and_then(|v| v.to_str().ok()) {
                let beta = beta
                    .split(',')
                    .map(str::trim)
                    .filter(|v| *v != "oauth-2025-04-20")
                    .collect::<Vec<_>>()
                    .join(",");
                if beta.is_empty() {
                    headers.remove("anthropic-beta");
                } else {
                    headers.insert(
                        "anthropic-beta",
                        HeaderValue::from_str(&beta).expect("filtered a valid header"),
                    );
                }
            }
        }
        let suffix = uri.path_and_query().map_or("/", |value| value.as_str());
        let (upstream, suffix) = match (&self.config.openai, route) {
            (Some(settings), Route::Promotional) => {
                (settings.api_upstream.as_str(), openai::api_path(suffix))
            }
            _ => (self.config.upstream.as_str(), suffix.to_owned()),
        };
        self.client
            .request(
                method,
                format!("{}{suffix}", upstream.trim_end_matches('/')),
            )
            .headers(headers)
            .body(body)
    }

    fn notify(&self, message: &str) {
        eprintln!("credit-router: {message}");
        if let Some(command) = &self.config.notify_send {
            match std::process::Command::new(command)
                .args([
                    if self.config.openai.is_some() {
                        "OpenAI API credits"
                    } else {
                        "Claude promotional credits"
                    },
                    message,
                ])
                .status()
            {
                Ok(status) if status.success() => {}
                Ok(status) => {
                    eprintln!("claude-credit-router: desktop notification failed: {status}")
                }
                Err(error) => {
                    eprintln!("claude-credit-router: desktop notification failed: {error}")
                }
            }
        }
    }

    // Native responses are never inspected, classified or retried here.
    async fn subscription(
        &self,
        parts: Parts,
        body: impl Into<reqwest::Body>,
        context: audit::ContextIds,
        reason: &str,
    ) -> Result<Response<Body>> {
        let trace = trace::Trace::start(self.store.audit(), context, Route::Subscription, reason);
        Ok(trace.stream(
            self.request(
                parts.method,
                &parts.uri,
                parts.headers,
                body,
                Route::Subscription,
            )
            .send()
            .await?,
            false,
        ))
    }

    async fn forward(&self, request: Request<Body>) -> Result<Response<Body>> {
        let (parts, body) = request.into_parts();
        let mut context = trace::context(&parts.headers);
        // Token counting is free and cannot prove a grant renewed. Other CLI
        // endpoints (including OAuth account/usage calls) remain native.
        let eligible = parts.method == Method::POST
            && if self.config.openai.is_some() {
                matches!(
                    parts.uri.path(),
                    "/responses" | "/v1/responses" | "/responses/compact" | "/v1/responses/compact"
                )
            } else {
                parts.uri.path() == "/v1/messages"
            };
        let now = (self.clock)();
        if !eligible || self.status(now)?.route == Route::Subscription {
            return self
                .subscription(
                    parts,
                    reqwest::Body::wrap_stream(body.into_data_stream()),
                    context,
                    if eligible {
                        "subscription_selected"
                    } else {
                        "non_inference_endpoint"
                    },
                )
                .await;
        }
        if parts.headers.contains_key("origin")
            || !parts
                .headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with("Bearer ") && value.len() > 7)
        {
            return Ok(Response::builder().status(StatusCode::FORBIDDEN)
                .body(Body::from("The local credit router accepts only native authenticated requests, not browser requests.")).expect("static response"));
        }
        let guard = self.promotional.lock().await;
        // A concurrent request or manual override may already have switched.
        if self.status((self.clock)())?.route == Route::Subscription {
            drop(guard);
            return self
                .subscription(
                    parts,
                    reqwest::Body::wrap_stream(body.into_data_stream()),
                    context,
                    "subscription_selected",
                )
                .await;
        }
        // Match the official Messages API's 128 MiB request limit. Keep bytes,
        // not parsed/re-serialized history, so signed thinking/tool blocks survive.
        let bytes = to_bytes(body, 128 * 1024 * 1024).await?;
        trace::request_context(&mut context, &bytes);
        // An upload can cross the calendar boundary or a manual route change.
        // Decide again after the last await before sending billable API traffic.
        if self.status((self.clock)())?.route == Route::Subscription {
            drop(guard);
            return self
                .subscription(parts, bytes, context, "route_changed_during_upload")
                .await;
        }
        if self.config.openai.is_some() {
            let model = serde_json::from_slice::<serde_json::Value>(&bytes)
                .ok()
                .and_then(|value| value["model"].as_str().map(str::to_owned));
            if !model.as_ref().is_some_and(|model| {
                self.models
                    .read()
                    .expect("model inventory lock poisoned")
                    .contains(model)
            }) {
                // Codex-only models cannot be silently remapped to API models.
                self.store.update(|state| {
                    state.note(
                        (self.clock)(),
                        &format!(
                            "native_model_not_available_on_api:{}",
                            model.as_deref().unwrap_or("missing")
                        ),
                    );
                    Ok(())
                })?;
                drop(guard);
                return self
                    .subscription(parts, bytes, context, "model_unavailable_on_api")
                    .await;
            }
        }
        // All promotional admissions share the router's 60-second cost snapshot,
        // including automatic operations. A failed refresh never sends API work.
        if !self.check_spending(false).await?
            || self.status((self.clock)())?.route == Route::Subscription
        {
            drop(guard);
            return self
                .subscription(parts, bytes, context, "local_spend_guard")
                .await;
        }
        let mut trace = trace::Trace::start(
            self.store.audit(),
            context.clone(),
            Route::Promotional,
            "api_selected",
        );
        let response = self
            .request(
                parts.method.clone(),
                &parts.uri,
                parts.headers.clone(),
                bytes.clone(),
                Route::Promotional,
            )
            .send()
            .await?;
        trace.response(&response);
        if response.status().is_success() {
            let changed = match self
                .store
                .update(|state| Ok(state.available((self.clock)())))
            {
                Ok(changed) => changed,
                Err(error) => {
                    // The model already accepted this request. Never turn its
                    // response into a retryable local error just because saving
                    // routing metadata failed.
                    eprintln!(
                        "claude-credit-router: forwarding accepted model response despite state persistence failure: {error}"
                    );
                    false
                }
            };
            drop(guard);
            if changed {
                self.notify("Monthly promotional API credits are available; using them again.");
            }
            // Tee only bounded usage metadata; never alter or replay output.
            // The route changes when the next request is rejected before any
            // output, which is the only rejection that is safe to replay.
            let router = self.clone();
            trace.on_credit_cut(move |context| {
                router.interrupt(context.native_session_id.clone());
            });
            return Ok(trace.stream(response, true));
        }
        if !(matches!(
            response.status(),
            StatusCode::BAD_REQUEST | StatusCode::PAYMENT_REQUIRED
        ) || self.config.openai.is_some()
            && response.status() == StatusCode::TOO_MANY_REQUESTS)
        {
            return Ok(trace.stream(response, false));
        }
        let code = response.status();
        let headers = response.headers().clone();
        let mut response = response;
        let mut error = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            // Unknown/large errors are forwarded without changing routes. Keep
            // the rest streaming instead of truncating an upstream response.
            error.extend_from_slice(&chunk);
            if error.len() > 64 * 1024 {
                let prefix = futures_util::stream::once(async move {
                    Ok::<Bytes, reqwest::Error>(Bytes::from(error))
                });
                let body = trace.body(prefix.chain(response.bytes_stream()), false);
                return Ok(reply(code, headers, body));
            }
        }
        let billing_limit = if self.config.openai.is_some() {
            openai::exhausted(code, &error)
        } else {
            exhausted(code, &error)
        };
        if !billing_limit {
            trace.finish("rejected_without_route_change");
            return Ok(reply(code, headers, Body::from(error)));
        }
        trace.finish("billing_rejected_native_fallback");
        let now = (self.clock)();
        let changed = self.store.update(|state| {
            if self.config.openai.is_some() {
                Ok(state.defer(
                    now,
                    policy::next_renewal(self.config.renewal_anchor, now)?,
                    "api_billing_limit",
                ))
            } else {
                state.exhausted(now, self.config.renewal_anchor)
            }
        }).unwrap_or_else(|error| {
            eprintln!("credit-router: cannot persist exhausted API route; forwarding the rejected request natively: {error}");
            false
        });
        drop(guard);
        if changed {
            self.notify("API allowance exhausted; using your normal subscription. Its limits and retries are unchanged.");
        }
        // Only an explicit pre-output rejection reaches here. This is not a
        // retry of a stream or a tool execution, nor a subscription retry loop.
        self.subscription(parts, bytes, context, "api_billing_limit")
            .await
    }
}

pub fn exhausted(status: StatusCode, bytes: &[u8]) -> bool {
    if !matches!(
        status,
        StatusCode::BAD_REQUEST | StatusCode::PAYMENT_REQUIRED
    ) {
        return false;
    }
    serde_json::from_slice::<serde_json::Value>(bytes).is_ok_and(|value| exhausted_error(&value))
}

/// The vendor's exact balance error, as a rejection body or a stream event.
pub fn exhausted_error(value: &serde_json::Value) -> bool {
    value["type"] == "error"
        && value["error"]["message"] == EXHAUSTED
        && matches!(
            value["error"]["type"].as_str(),
            Some("invalid_request_error" | "billing_error")
        )
}

fn strip_hop_headers(headers: &mut HeaderMap) {
    if let Some(connection) = headers
        .get("connection")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
    {
        for name in connection.split(',').map(str::trim) {
            headers.remove(name);
        }
    }
    for name in [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    ] {
        headers.remove(name);
    }
}

fn reply(status: StatusCode, mut headers: HeaderMap, body: Body) -> Response<Body> {
    strip_hop_headers(&mut headers);
    let mut response = Response::new(body);
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}

async fn status(State(router): State<Arc<Router>>) -> Response<Body> {
    if router
        .status((router.clock)())
        .is_ok_and(|s| s.route == Route::Promotional)
    {
        if let Err(error) = router.check_spending(false).await {
            return failure(error);
        }
    }
    match router
        .status((router.clock)())
        .and_then(|value| Ok(serde_json::to_vec(&value)?))
    {
        Ok(bytes) => Response::builder()
            .header("content-type", "application/json")
            .header("cache-control", "no-store")
            .body(Body::from(bytes))
            .expect("static response"),
        Err(error) => failure(error),
    }
}

async fn forward(State(router): State<Arc<Router>>, request: Request<Body>) -> Response<Body> {
    match router.forward(request).await {
        Ok(response) => response,
        Err(error) => failure(error),
    }
}

fn failure(error: anyhow::Error) -> Response<Body> {
    // Error chains from HTTP clients can contain URLs; never log bodies/headers.
    if let Some(http) = error.downcast_ref::<reqwest::Error>() {
        eprintln!(
            "claude-credit-router: request failed: status={:?} connect={} timeout={}",
            http.status(),
            http.is_connect(),
            http.is_timeout()
        );
    } else {
        eprintln!("claude-credit-router: request failed: {error}");
    }
    Response::builder().status(StatusCode::BAD_GATEWAY).header("content-type", "application/json")
        .body(Body::from(r#"{"type":"error","error":{"type":"api_error","message":"Local promotional router failed; inspect its service journal."}}"#)).expect("static response")
}
