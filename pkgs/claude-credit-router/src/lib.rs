pub mod policy;

use anyhow::{Context, Result, ensure};
use axum::body::{Body, Bytes, to_bytes};
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, Method, Request, Response, StatusCode, Uri};
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
    pub billing: Billing,
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
        ensure!(
            self.billing.no_payment_method
                && self.billing.purchased_balance_usd == 0
                && !self.billing.auto_reload
                && !self.billing.sales_invoicing,
            "API routing requires a verified promotional-only organization: no card, purchased credits, auto-reload or invoicing"
        );
        ensure!(
            self.billing.verified_at <= Utc::now(),
            "billing verification cannot be in the future"
        );
        Ok(())
    }
}

#[derive(Clone)]
pub struct Router {
    pub config: Config,
    pub store: Store,
    client: reqwest::Client,
    key: HeaderValue,
    // Serializes only promotional attempts until their response headers/error
    // arrive. Streaming output and all subscription traffic remain concurrent.
    promotional: Arc<Mutex<()>>,
}

#[derive(Serialize)]
pub struct Status {
    pub route: Route,
    pub active: Route,
    pub mode: Mode,
    pub organization_id: String,
    pub next_probe_at: Option<DateTime<Utc>>,
    pub history: Vec<policy::Event>,
}

impl Router {
    pub fn new(config: Config, store: Store, key: &str) -> Result<Self> {
        config.validate()?;
        store.read()?;
        let mut key = HeaderValue::from_str(key).context("invalid API key header")?;
        key.set_sensitive(true);
        Ok(Self {
            config,
            store,
            key,
            client: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            promotional: Arc::new(Mutex::new(())),
        })
    }

    pub fn status(&self, now: DateTime<Utc>) -> Result<Status> {
        let state = self.store.read()?;
        Ok(Status {
            route: state.route(now),
            active: state.active,
            mode: state.mode,
            organization_id: self.config.organization_id.clone(),
            next_probe_at: state.next_probe_at,
            history: state.history,
        })
    }

    // Free metadata request: validate the key's organization before it is ever
    // allowed to send inference. There is no inference/billing probe at startup.
    pub async fn verify_organization(&self) -> Result<()> {
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
        if route == Route::Promotional {
            headers.remove("authorization");
            headers.remove("cookie");
            headers.insert("x-api-key", self.key.clone());
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
        self.client
            .request(
                method,
                format!("{}{suffix}", self.config.upstream.trim_end_matches('/')),
            )
            .headers(headers)
            .body(body)
    }

    fn notify(&self, message: &str) {
        eprintln!("claude-credit-router: {message}");
        if let Some(command) = &self.config.notify_send {
            match std::process::Command::new(command)
                .args(["Claude promotional credits", message])
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

    async fn forward(&self, request: Request<Body>) -> Result<Response<Body>> {
        let (parts, body) = request.into_parts();
        // Token counting is free and cannot prove a grant renewed. Other CLI
        // endpoints (including OAuth account/usage calls) remain native.
        let eligible = parts.method == Method::POST && parts.uri.path() == "/v1/messages";
        let now = Utc::now();
        if !eligible || self.store.read()?.route(now) == Route::Subscription {
            // No subscription response is inspected, retried or classified.
            let response = self
                .request(
                    parts.method,
                    &parts.uri,
                    parts.headers,
                    reqwest::Body::wrap_stream(body.into_data_stream()),
                    Route::Subscription,
                )
                .send()
                .await?;
            return Ok(stream(response));
        }
        if parts.headers.contains_key("origin")
            || !parts
                .headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with("Bearer ") && value.len() > 7)
        {
            return Ok(Response::builder().status(StatusCode::FORBIDDEN)
                .body(Body::from("The local credit router accepts only native authenticated Claude requests, not browser requests.")).expect("static response"));
        }
        let guard = self.promotional.lock().await;
        // A concurrent request or manual override may already have switched.
        if self.store.read()?.route(Utc::now()) == Route::Subscription {
            drop(guard);
            let response = self
                .request(
                    parts.method,
                    &parts.uri,
                    parts.headers,
                    reqwest::Body::wrap_stream(body.into_data_stream()),
                    Route::Subscription,
                )
                .send()
                .await?;
            return Ok(stream(response));
        }
        // Match the official Messages API's 128 MiB request limit. Keep bytes,
        // not parsed/re-serialized history, so signed thinking/tool blocks survive.
        let bytes = to_bytes(body, 128 * 1024 * 1024).await?;
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
        if response.status().is_success() {
            let changed = match self.store.update(|state| Ok(state.available(Utc::now()))) {
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
            // Never examine or replay a stream, including a 200 SSE error.
            return Ok(stream(response));
        }
        if !matches!(
            response.status(),
            StatusCode::BAD_REQUEST | StatusCode::PAYMENT_REQUIRED
        ) {
            return Ok(stream(response));
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
                let body = Body::from_stream(prefix.chain(response.bytes_stream()));
                return Ok(reply(code, headers, body));
            }
        }
        if !exhausted(code, &error) {
            return Ok(reply(code, headers, Body::from(error)));
        }
        let changed = self
            .store
            .update(|state| state.exhausted(Utc::now(), self.config.renewal_anchor))?;
        drop(guard);
        if changed {
            self.notify("Promotional API credits exhausted; using your normal Claude subscription. Its limits and retries are unchanged.");
        }
        // Only an explicit pre-output rejection reaches here. This is not a
        // retry of a stream or a tool execution, nor a subscription retry loop.
        let response = self
            .request(
                parts.method,
                &parts.uri,
                parts.headers,
                bytes,
                Route::Subscription,
            )
            .send()
            .await?;
        Ok(stream(response))
    }
}

pub fn exhausted(status: StatusCode, bytes: &[u8]) -> bool {
    if !matches!(
        status,
        StatusCode::BAD_REQUEST | StatusCode::PAYMENT_REQUIRED
    ) {
        return false;
    }
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return false;
    };
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

fn stream(response: reqwest::Response) -> Response<Body> {
    reply(
        response.status(),
        response.headers().clone(),
        Body::from_stream(response.bytes_stream()),
    )
}

async fn status(State(router): State<Arc<Router>>) -> Response<Body> {
    match router
        .status(Utc::now())
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
