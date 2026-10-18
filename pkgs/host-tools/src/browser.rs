//! Private noVNC viewer and on-demand Camofox lifecycle. Human handoff is an
//! agent instruction, not an ownership protocol. Caddy supplies viewer auth.
use anyhow::{Context, Result, bail, ensure};
use axum::{
    Router,
    body::{Body, Bytes, to_bytes},
    extract::{
        DefaultBodyLimit, Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, Method, Request, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    future::Future,
    net::SocketAddr,
    path::{Component, Path as FsPath, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, OwnedMutexGuard, broadcast};

const USER: &str = "home-browser";
const IDLE: u64 = 600;
type Shared = Arc<App>;
type WebResult = std::result::Result<Response, (StatusCode, String)>;

#[derive(Deserialize)]
struct Config {
    listen: SocketAddr,
    backend: reqwest::Url,
    websocket: reqwest::Url,
    origin: String,
    state: PathBuf,
    novnc: PathBuf,
}
impl Config {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.listen.ip().is_loopback(),
            "Browser controller must bind only to loopback"
        );
        for (name, url, scheme) in [
            ("backend", &self.backend, "http"),
            ("websocket", &self.websocket, "ws"),
        ] {
            ensure!(
                private_url(url, scheme),
                "Private {name} must use a literal loopback address without URL credentials"
            );
        }
        https_origin(&self.origin, "viewer origin")?;
        Ok(())
    }
}

/// Private endpoints must use a literal loopback address without URL credentials:
/// a hostname could resolve elsewhere than it appears to.
pub(crate) fn private_url(url: &reqwest::Url, scheme: &str) -> bool {
    let loopback = match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    };
    url.scheme() == scheme && loopback && url.username().is_empty() && url.password().is_none()
}

/// Match pages and the viewer by an exact HTTPS origin, never a path, query or
/// URL credentials that could point somewhere else.
pub(crate) fn https_origin(value: &str, label: &str) -> Result<reqwest::Url> {
    let url: reqwest::Url = value.parse().with_context(|| format!("invalid {label}"))?;
    ensure!(
        url.scheme() == "https" && url.origin().ascii_serialization() == value,
        "{label} must be an exact HTTPS origin without path, query or URL credentials"
    );
    Ok(url)
}
// Only the tab identity survives a controller restart. Activity and readiness
// belong to this process, not a wall-clock record rewritten on every tool call.
#[derive(Deserialize, Serialize)]
struct TabCache {
    tab: Option<String>,
}

#[derive(Clone, Default, PartialEq, Eq)]
enum Browser {
    #[default]
    Stopped,
    Undiscovered(Option<String>),
    Ready(String),
}
impl Browser {
    fn tab(&self) -> Option<&str> {
        match self {
            Self::Stopped => None,
            Self::Undiscovered(tab) => tab.as_deref(),
            Self::Ready(tab) => Some(tab),
        }
    }
    fn is_running(&self) -> bool {
        !matches!(self, Self::Stopped)
    }
}
/// A value typed through the credential-fill bridge. It stays in memory only,
/// long enough for later page responses to redact it again.
struct Secret {
    tab: String,
    value: String,
    created: Instant,
}
/// Credential values are forgotten after this long; a page holds them no longer.
const SECRET_TTL: Duration = Duration::from_secs(900);
const MAX_SECRETS: usize = 8;
/// A listing that only decorates an already live tab must stay well inside the
/// five-second adoption probe of the tools that call it.
const READY_LISTING_TIMEOUT: Duration = Duration::from_secs(2);
struct Runtime {
    browser: Browser,
    last_activity: Instant,
    error: Option<String>,
    viewers: usize,
    secrets: Vec<Secret>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            browser: Browser::Stopped,
            last_activity: Instant::now(),
            error: None,
            viewers: 0,
            secrets: Vec::new(),
        }
    }
}
impl Runtime {
    fn remember_secret(&mut self, tab: &str, value: String) {
        self.secrets
            .retain(|secret| secret.created.elapsed() < SECRET_TTL && secret.value != value);
        self.secrets.push(Secret {
            tab: tab.to_owned(),
            value,
            created: Instant::now(),
        });
        if self.secrets.len() > MAX_SECRETS {
            self.secrets.remove(0);
        }
    }
    fn secrets_for(&self, tab: &str) -> Vec<String> {
        self.secrets
            .iter()
            .filter(|secret| secret.tab == tab && secret.created.elapsed() < SECRET_TTL)
            .map(|secret| secret.value.clone())
            .collect()
    }
}
/// A snapshot renders a value verbatim or as a quoted scalar, which escapes
/// backslashes, quotes and control characters.
fn quoted_form(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
fn redact_text(text: &str, secrets: &[String]) -> String {
    let mut redacted = text.to_owned();
    for secret in secrets {
        redacted = redacted.replace(secret.as_str(), "[redacted]");
        let quoted = quoted_form(secret);
        if quoted != *secret {
            redacted = redacted.replace(&quoted, "[redacted]");
        }
    }
    redacted
}
fn redact_value(value: &mut Value, secrets: &[String], depth: u8) {
    if depth > 64 {
        return;
    }
    match value {
        Value::String(text) => {
            if secrets.iter().any(|secret| text.contains(secret.as_str())) {
                *text = redact_text(text, secrets);
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_value(item, secrets, depth + 1);
            }
        }
        Value::Object(fields) => {
            for field in fields.values_mut() {
                redact_value(field, secrets, depth + 1);
            }
        }
        _ => {}
    }
}
fn redact_response(bytes: &Bytes, secrets: &[String]) -> Option<Bytes> {
    let mut value: Value = serde_json::from_slice(bytes).ok()?;
    redact_value(&mut value, secrets, 0);
    serde_json::to_vec(&value).ok().map(Bytes::from)
}
struct App {
    config: Config,
    client: reqwest::Client,
    access: String,
    admin: String,
    agent: String,
    proxy: String,
    csrf: String,
    page: Bytes,
    csp: header::HeaderValue,
    transport_timeout: Duration,
    runtime: Arc<Mutex<Runtime>>,
    disconnect: broadcast::Sender<()>,
}

fn failure(error: anyhow::Error) -> (StatusCode, String) {
    eprintln!("browser-control: operation failed: {error:#}");
    (
        StatusCode::BAD_GATEWAY,
        "Browser operation failed. Retry; see the private service log.".into(),
    )
}
fn fixed_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}
fn viewer(
    app: &App,
    headers: &HeaderMap,
    mutate: bool,
) -> std::result::Result<(), (StatusCode, String)> {
    let key = headers
        .get("x-hermes-viewer-key")
        .map(|v| v.as_bytes())
        .unwrap_or_default();
    if !fixed_eq(key, app.proxy.as_bytes()) {
        return Err((StatusCode::UNAUTHORIZED, "Authentication required".into()));
    }
    if mutate {
        let origin = headers
            .get(header::ORIGIN)
            .map(|v| v.as_bytes())
            .unwrap_or_default();
        let csrf = headers
            .get("x-hermes-csrf")
            .map(|v| v.as_bytes())
            .unwrap_or_default();
        if !fixed_eq(origin, app.config.origin.as_bytes()) || !fixed_eq(csrf, app.csrf.as_bytes()) {
            return Err((
                StatusCode::FORBIDDEN,
                "Same-origin action required; reload this page".into(),
            ));
        }
    }
    Ok(())
}
fn agent(app: &App, headers: &HeaderMap) -> std::result::Result<(), (StatusCode, String)> {
    let supplied = headers
        .get(header::AUTHORIZATION)
        .map(|v| v.as_bytes())
        .unwrap_or_default();
    if !fixed_eq(supplied, app.agent.as_bytes()) {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Agent authentication required".into(),
        ));
    }
    Ok(())
}

impl App {
    async fn save(&self, runtime: &Runtime) -> Result<()> {
        let temporary = self.config.state.with_extension("new");
        let cache = TabCache {
            tab: runtime.browser.tab().map(str::to_owned),
        };
        tokio::fs::write(&temporary, serde_json::to_vec(&cache)?).await?;
        tokio::fs::rename(temporary, &self.config.state).await?;
        Ok(())
    }
    async fn backend(&self, method: Method, path: &str, data: Option<Value>) -> Result<Value> {
        let mut request = self
            .client
            .request(method, self.config.backend.join(path)?)
            .bearer_auth(&self.access);
        if path == "/stop" {
            request = request.header("x-admin-key", &self.admin);
        }
        if let Some(data) = data {
            request = request.json(&data);
        }
        let response = request
            .send()
            .await
            .context("private Camofox request failed")?;
        if !response.status().is_success() {
            bail!("private Camofox returned {} for {path}", response.status());
        }
        response
            .json()
            .await
            .context("invalid private Camofox response")
    }
    async fn backend_running(&self) -> Result<bool> {
        self.backend(Method::GET, "/", None).await?["running"]
            .as_bool()
            .context("Camofox status is missing its running boolean")
    }
    async fn initial_status(&self, timeout: Duration) -> Result<bool> {
        tokio::time::timeout(timeout, async {
            // systemd starts Node before its HTTP listener is ready. Retry only
            // transport startup failures; authentication/schema errors are fatal.
            let mut waiting = false;
            loop {
                match self.backend_running().await {
                    Err(error)
                        if error
                            .downcast_ref::<reqwest::Error>()
                            .is_some_and(|error| error.is_connect() || error.is_timeout()) =>
                    {
                        if !waiting {
                            eprintln!(
                                "browser-control: waiting for Camofox HTTP readiness: {error:#}"
                            );
                            waiting = true;
                        }
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                    result => return result,
                }
            }
        })
        .await
        .context("Camofox HTTP readiness exceeded its startup deadline")?
    }
    async fn start(&self, runtime: &mut Runtime, viewer: bool) -> Result<()> {
        let result =
            tokio::time::timeout(Duration::from_secs(60), self.start_browser(runtime, viewer))
                .await
                .context("browser startup exceeded 60 seconds")
                .and_then(|result| result);
        runtime.error = result
            .as_ref()
            .err()
            .map(|_| "Startup failed. Retry; consult private logs.".into());
        result
    }
    async fn started(self: &Arc<Self>, viewer: bool) -> Result<OwnedMutexGuard<Runtime>> {
        let app = self.clone();
        // A disconnected/short-timeout client must not release the lifecycle
        // lock while Camofox is still creating a browser or tab. Keep the
        // bounded startup alive; later clients reuse its completed discovery.
        tokio::spawn(async move {
            let mut runtime = app.runtime.clone().lock_owned().await;
            app.start(&mut runtime, viewer).await?;
            Ok::<_, anyhow::Error>(runtime)
        })
        .await
        .context("browser startup task failed")?
    }
    async fn start_browser(&self, runtime: &mut Runtime, viewer: bool) -> Result<()> {
        if !self.backend_running().await? {
            runtime.browser = Browser::Undiscovered(None);
            // A timed-out launch may still have created Firefox. Keep it under
            // idle cleanup even if we never receive the launch response.
            runtime.last_activity = Instant::now();
            self.backend(Method::POST, "/start", Some(json!({})))
                .await?;
        }
        if !runtime.browser.is_running() {
            runtime.browser = Browser::Undiscovered(None);
        }
        runtime.last_activity = Instant::now();
        let ready = matches!(runtime.browser, Browser::Ready(_));
        if !ready {
            let tabs = self
                .backend(Method::GET, &format!("/tabs?userId={USER}"), None)
                .await?;
            let tab = match shared_tab(&tabs, runtime.browser.tab())? {
                Some(id) => id,
                None => self
                    .backend(
                        Method::POST,
                        "/tabs",
                        // Omitting URL creates a blank tab; the pinned API
                        // rejects about:blank as an explicit navigation URL.
                        Some(json!({"userId":USER,"listItemId":USER})),
                    )
                    .await?["tabId"]
                    .as_str()
                    .context("Camofox did not return a tab ID")?
                    .into(),
            };
            runtime.browser = Browser::Undiscovered(Some(tab));
            self.save(runtime).await?;
        }
        if viewer || !ready {
            loop {
                if self.backend(Method::GET, "/vnc/status", None).await?["running"]
                    .as_bool()
                    .context("VNC status is missing its running boolean")?
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
        if let Browser::Undiscovered(tab) = &mut runtime.browser {
            runtime.browser = Browser::Ready(
                tab.take()
                    .context("startup completed without a tracked tab")?,
            );
        }
        runtime.last_activity = Instant::now();
        Ok(())
    }
    async fn stop(&self, runtime: &mut Runtime) -> Result<()> {
        if self.backend_running().await? {
            if runtime.browser.tab().is_some() {
                // The pinned upstream export propagates persistence failures.
                self.backend(
                    Method::GET,
                    &format!("/sessions/{USER}/storage_state"),
                    None,
                )
                .await?;
            }
            self.backend(Method::POST, "/stop", Some(json!({}))).await?;
        }
        runtime.browser = Browser::Stopped;
        runtime.error = None;
        self.save(runtime).await
    }
}

fn shared_tab(tabs: &Value, retained: Option<&str>) -> Result<Option<String>> {
    let mut selected = None;
    let mut ambiguous = false;
    for tab in tabs["tabs"]
        .as_array()
        .context("Camofox tabs response is missing its array")?
    {
        if tab
            .get("listItemId")
            .is_some_and(|id| id.as_str() != Some(USER))
        {
            continue;
        }
        let id = tab["tabId"].as_str().context("missing shared tab ID")?;
        if retained == Some(id) {
            return Ok(Some(id.into()));
        }
        ambiguous |= selected.is_some();
        selected = Some(id);
    }
    if ambiguous {
        bail!("Several shared tabs exist; close extra tabs before retrying");
    }
    Ok(selected.map(str::to_owned))
}

/// The live URL of the tracked tab, so the credential bridge can bind a fill to
/// an exact origin instead of trusting the page it is about to type into.
fn tab_url<'a>(tabs: &'a Value, tab: &str) -> Option<&'a str> {
    tabs["tabs"]
        .as_array()?
        .iter()
        .find(|entry| entry["tabId"].as_str() == Some(tab))
        .and_then(|entry| entry["url"].as_str())
}

fn page(csrf: &str, stock: &str) -> Result<(Bytes, header::HeaderValue)> {
    // Keep the packaged UI and its input handlers upstream-owned. Replace only
    // its bootstrap with the authenticated, lazy-start deployment adapter.
    ensure!(
        stock.matches("<head>").count() == 1
            && stock.matches("</head>").count() == 1
            && stock.matches("<script type=\"module\">").count() == 1
            && stock.contains("id=\"noVNC_connect_button\"")
            && stock.contains("id=\"noVNC_keyboardinput\""),
        "Packaged noVNC UI changed; review the startup adapter"
    );
    let (before, module) = stock.split_once("<script type=\"module\">").unwrap();
    let (_, after) = module
        .split_once("</script>")
        .context("Unterminated noVNC bootstrap")?;
    let adapter = include_str!("browser_viewer.js").replace("__CSRF__", csrf);
    let html =
        format!("{before}<script type=\"module\" nonce=\"{csrf}\">{adapter}</script>{after}")
            .replacen("<head>", "<head><base href=\"/browser/novnc/\">", 1)
            .replacen(
                "</head>",
                &format!(
                    "<style nonce=\"{csrf}\">{}</style></head>",
                    include_str!("browser_viewer.css")
                ),
                1,
            );
    let csp = format!("default-src 'none'; script-src 'self' 'nonce-{csrf}'; style-src 'self' 'nonce-{csrf}'; img-src 'self' data:; font-src 'self'; media-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'none'")
        .parse().expect("valid generated CSP");
    Ok((Bytes::from(html), csp))
}

// The native Hermes availability probe has no bearer header. This passive,
// loopback-only answer contains no state and never launches Firefox.
async fn health() -> axum::Json<Value> {
    axum::Json(json!({"ok":true,"engine":"camoufox"}))
}
async fn landing(State(app): State<Shared>, headers: HeaderMap) -> WebResult {
    viewer(&app, &headers, false)?;
    let mut response = (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        app.page.clone(),
    )
        .into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_SECURITY_POLICY, app.csp.clone());
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    Ok(response)
}
async fn status(State(app): State<Shared>, headers: HeaderMap) -> WebResult {
    viewer(&app, &headers, false)?;
    let running = app.backend_running().await.map_err(failure)?;
    let runtime = app.runtime.lock().await;
    Ok(
        axum::Json(json!({"running":running,"viewers":runtime.viewers,"error":runtime.error}))
            .into_response(),
    )
}
async fn open(State(app): State<Shared>, headers: HeaderMap) -> WebResult {
    viewer(&app, &headers, true)?;
    let _runtime = app.started(true).await.map_err(failure)?;
    Ok(axum::Json(json!({"ok":true})).into_response())
}
async fn close_viewer(State(app): State<Shared>, headers: HeaderMap) -> WebResult {
    viewer(&app, &headers, true)?;
    let _ = app.disconnect.send(());
    Ok(axum::Json(json!({"ok":true})).into_response())
}

#[derive(Debug, PartialEq, Eq)]
enum ToolRoute<'a> {
    Tabs,
    Tab(&'a str),
}

fn tool_route<'a>(method: &Method, path: &'a str) -> Option<ToolRoute<'a>> {
    if path == "/tabs" {
        return matches!(*method, Method::GET | Method::POST).then_some(ToolRoute::Tabs);
    }
    let (id, suffix) = path.strip_prefix("/tabs/")?.split_once('/')?;
    if id.is_empty()
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
    {
        return None;
    }
    matches!(
        (method, suffix),
        (&Method::GET, "snapshot" | "screenshot" | "downloads")
            | (
                &Method::POST,
                "navigate" | "click" | "type" | "scroll" | "back" | "press"
            )
    )
    .then_some(ToolRoute::Tab(id))
}
async fn api(State(app): State<Shared>, request: Request<Body>) -> WebResult {
    agent(&app, request.headers())?;
    let (parts, body) = request.into_parts();
    let method = &parts.method;
    let path = parts.uri.path();
    let route = tool_route(method, path).ok_or_else(|| {
        (
            StatusCode::FORBIDDEN,
            "Outside the approved Camofox tool surface".into(),
        )
    })?;
    const IDENTITY_KEYS: [&str; 2] = ["userId", "listItemId"];
    let identity_error = || {
        (
            StatusCode::FORBIDDEN,
            "Different browser identity refused".into(),
        )
    };
    let mut url = app
        .config
        .backend
        .join(path)
        .map_err(|error| failure(error.into()))?;
    {
        let mut query = url.query_pairs_mut();
        for (key, value) in
            url::form_urlencoded::parse(parts.uri.query().unwrap_or_default().as_bytes())
        {
            if IDENTITY_KEYS.contains(&key.as_ref()) {
                if value != USER {
                    return Err(identity_error());
                }
            } else {
                query.append_pair(&key, &value);
            }
        }
        query.append_pair("userId", USER);
    }
    let bytes = to_bytes(body, 65536)
        .await
        .map_err(|_| (StatusCode::PAYLOAD_TOO_LARGE, "Request too large".into()))?;
    let mut data: Map<String, Value> = if bytes.is_empty() {
        Map::new()
    } else {
        serde_json::from_slice(&bytes)
            .map_err(|_| (StatusCode::BAD_REQUEST, "Expected JSON object".into()))?
    };
    if IDENTITY_KEYS.into_iter().any(|key| {
        data.get(key)
            .is_some_and(|value| value.as_str() != Some(USER))
    }) {
        return Err(identity_error());
    }
    if route == ToolRoute::Tabs && *method == Method::GET {
        // Hermes's five-second adoption probe must only discover existing
        // tabs. Creating a blank tab here can make its subsequent ensure-tab
        // path skip the requested navigation, or cancel startup mid-creation.
        // The credential bridge reads the same listing, so it also carries the
        // tracked tab's live URL for binding a fill to an exact origin.
        let observed = app.runtime.lock().await.browser.clone();
        let running = app.backend_running().await.map_err(failure)?;
        let ready = matches!(observed, Browser::Ready(_));
        let listing_path = format!("/tabs?userId={USER}");
        let listing = app.backend(Method::GET, &listing_path, None);
        let discovery = if !running {
            None
        } else if ready {
            // Once the tab is live, the listing only supplies the URL that
            // binds a credential fill to an origin. A slow or failing listing
            // must not fail or stall the passive adoption probe.
            match tokio::time::timeout(READY_LISTING_TIMEOUT, listing).await {
                Ok(Ok(tabs)) => Some(tabs),
                outcome => {
                    eprintln!(
                        "browser-control: tab listing unavailable for a live tab: {outcome:?}"
                    );
                    None
                }
            }
        } else {
            Some(listing.await.map_err(failure)?)
        };
        let mut runtime = app.runtime.lock().await;
        // A real operation may have started or stopped the browser while this
        // passive probe was in flight. Its state takes precedence over ours.
        if runtime.browser == observed {
            if !running {
                runtime.browser = Browser::Stopped;
            } else if !matches!(observed, Browser::Ready(_))
                && let Some(tabs) = discovery.as_ref()
            {
                let tab = shared_tab(tabs, runtime.browser.tab()).map_err(failure)?;
                let changed = runtime.browser.tab() != tab.as_deref();
                runtime.browser = Browser::Undiscovered(tab);
                if changed {
                    app.save(&runtime).await.map_err(failure)?;
                }
            }
        }
        if runtime.browser.is_running() {
            runtime.last_activity = Instant::now();
        }
        let tabs = runtime.browser.tab().map_or_else(Vec::new, |tab| {
            let mut entry = json!({"tabId":tab,"userId":USER,"listItemId":USER});
            if let Some(url) = discovery.as_ref().and_then(|tabs| tab_url(tabs, tab)) {
                entry["url"] = json!(url);
            }
            vec![entry]
        });
        return Ok(axum::Json(json!({"tabs":tabs})).into_response());
    }
    // One lifecycle lock coalesces cold launches and keeps idle shutdown out of
    // an actual HTTP operation. It does not reserve ownership or block viewers.
    let mut runtime = app.started(false).await.map_err(failure)?;
    let tab = runtime
        .browser
        .tab()
        .expect("startup selects a tab")
        .to_owned();
    if route == ToolRoute::Tabs {
        if let Some(url) = data
            .get("url")
            .and_then(Value::as_str)
            .filter(|url| *url != "about:blank")
        {
            app.backend(
                Method::POST,
                &format!("/tabs/{tab}/navigate"),
                Some(json!({"userId":USER,"url":url})),
            )
            .await
            .map_err(failure)?;
        }
        return Ok(axum::Json(json!({"tabId":tab})).into_response());
    }
    if route != ToolRoute::Tab(tab.as_str()) {
        return Err((
            StatusCode::NOT_FOUND,
            "Live tab was lost; navigate again to reopen the saved task URL".into(),
        ));
    }
    // A credential fill is the one request whose value must not come back:
    // remember it so later page responses can redact it again.
    let credential = if *method == Method::POST && data.contains_key("expectedOrigin") {
        data.get("text")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .filter(|text| !text.is_empty())
    } else {
        None
    };
    let mut upstream = app
        .client
        .request(method.clone(), url)
        .bearer_auth(&app.access);
    if *method != Method::GET {
        data.insert("userId".into(), json!(USER));
        upstream = upstream.json(&data);
    }
    let response = upstream
        .send()
        .await
        .map_err(|error| failure(error.into()))?;
    let status = response.status();
    let content_type = response.headers().get(header::CONTENT_TYPE).cloned();
    let bytes = response
        .bytes()
        .await
        .map_err(|error| failure(error.into()))?;
    runtime.last_activity = Instant::now();
    if status.is_success()
        && let Some(value) = credential
    {
        runtime.remember_secret(&tab, value);
    }
    if status == StatusCode::NOT_FOUND
        && serde_json::from_slice::<Value>(&bytes)
            .ok()
            .is_some_and(|value| value["error"] == "Tab not found")
    {
        // Preserve a selector/resource 404, but rediscover an actually lost tab
        // on the next call. Never replay a possibly consequential operation.
        runtime.browser = Browser::Undiscovered(None);
        app.save(&runtime).await.map_err(failure)?;
    }
    let secrets = runtime.secrets_for(&tab);
    let bytes = if secrets.is_empty() {
        bytes
    } else {
        redact_response(&bytes, &secrets).unwrap_or(bytes)
    };
    let mut result = (status, bytes).into_response();
    if let Some(value) = content_type {
        result.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    Ok(result)
}

async fn asset(
    State(app): State<Shared>,
    Path(path): Path<String>,
    headers: HeaderMap,
) -> WebResult {
    viewer(&app, &headers, false)?;
    if FsPath::new(&path)
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err((StatusCode::FORBIDDEN, "Invalid asset path".into()));
    }
    let candidate = tokio::fs::canonicalize(app.config.novnc.join(&path))
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, "Asset not found".into()))?;
    if !candidate.starts_with(&app.config.novnc) {
        return Err((StatusCode::FORBIDDEN, "Invalid asset path".into()));
    }
    let mime = match candidate.extension().and_then(|s| s.to_str()) {
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("svg") => "image/svg+xml",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("oga" | "ogg") => "audio/ogg",
        Some("mp3") => "audio/mpeg",
        _ => return Err((StatusCode::NOT_FOUND, "Asset not served".into())),
    };
    let bytes = tokio::fs::read(candidate)
        .await
        .map_err(|error| failure(error.into()))?;
    Ok(([(header::CONTENT_TYPE, mime)], bytes).into_response())
}
async fn websocket(
    State(app): State<Shared>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> WebResult {
    viewer(&app, &headers, false)?;
    if headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) != Some(&app.config.origin) {
        return Err((StatusCode::FORBIDDEN, "WebSocket origin refused".into()));
    }
    let mut runtime = app.runtime.lock().await;
    if !runtime.browser.is_running() {
        return Err((StatusCode::CONFLICT, "Open the browser first".into()));
    }
    let disconnected = app.disconnect.subscribe();
    runtime.viewers += 1;
    drop(runtime);
    let failed = app.clone();
    Ok(ws
        .protocols(["binary"])
        .on_failed_upgrade(move |_| {
            tokio::spawn(async move {
                viewer_closed(&failed).await;
            });
        })
        .on_upgrade(move |socket| forward(app, socket, disconnected))
        .into_response())
}
async fn viewer_closed(app: &App) {
    let mut runtime = app.runtime.lock().await;
    runtime.viewers = runtime
        .viewers
        .checked_sub(1)
        .expect("viewer accounting underflow");
    runtime.last_activity = Instant::now();
}
async fn transport<T, E>(
    operation: &str,
    deadline: Duration,
    future: impl Future<Output = std::result::Result<T, E>>,
) -> Result<T>
where
    E: std::error::Error + Send + Sync + 'static,
{
    tokio::time::timeout(deadline, future)
        .await
        .with_context(|| format!("viewer {operation} timed out"))?
        .with_context(|| format!("viewer {operation} failed"))
}
async fn forward(app: Shared, mut socket: WebSocket, mut disconnected: broadcast::Receiver<()>) {
    let result: Result<()> = async {
        let (mut upstream, _) = tokio::select! {
            biased;
            _ = disconnected.recv() => return Ok(()),
            result = transport("connect", app.transport_timeout, tokio_tungstenite::connect_async(app.config.websocket.as_str())) => result?,
        };
        let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
        let mut last_pong = tokio::time::Instant::now();
        loop {
            tokio::select! {
                _ = disconnected.recv() => break,
                _ = heartbeat.tick() => {
                    if last_pong.elapsed() > Duration::from_secs(45) { break; }
                    transport("heartbeat write", app.transport_timeout, socket.send(Message::Ping(Bytes::new()))).await?;
                }
                incoming = socket.recv() => match incoming {
                    Some(Ok(Message::Binary(bytes))) => transport("upstream write", app.transport_timeout, upstream.send(tokio_tungstenite::tungstenite::Message::Binary(bytes))).await?,
                    Some(Ok(Message::Pong(_))) => last_pong = tokio::time::Instant::now(),
                    Some(Ok(Message::Ping(bytes))) => transport("pong write", app.transport_timeout, socket.send(Message::Pong(bytes))).await?,
                    Some(Ok(Message::Text(_))) => bail!("text frames are not a VNC transport"),
                    Some(Err(error)) => return Err(error.into()),
                    Some(Ok(Message::Close(_))) | None => break,
                },
                incoming = upstream.next() => match incoming {
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Binary(bytes))) => transport("viewer write", app.transport_timeout, socket.send(Message::Binary(bytes))).await?,
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Ping(bytes))) => transport("upstream pong write", app.transport_timeout, upstream.send(tokio_tungstenite::tungstenite::Message::Pong(bytes))).await?,
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Pong(_))) => {},
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) | None => break,
                    Some(Ok(_)) => bail!("unexpected frame from VNC transport"),
                    Some(Err(error)) => return Err(error.into()),
                },
            }
        }
        Ok(())
    }.await;
    if let Err(error) = result {
        eprintln!("browser-control: viewer transport ended: {error}");
    }
    if let Err(error) = transport("close", app.transport_timeout, socket.close()).await {
        eprintln!("browser-control: viewer close failed: {error}");
    }
    viewer_closed(&app).await;
}
fn idle(runtime: &Runtime) -> bool {
    runtime.browser.is_running()
        && runtime.viewers == 0
        && runtime.last_activity.elapsed() >= Duration::from_secs(IDLE)
}
async fn housekeeping(app: Shared) {
    let mut ticker = tokio::time::interval(Duration::from_secs(15));
    loop {
        ticker.tick().await;
        let mut runtime = app.runtime.lock().await;
        if idle(&runtime) {
            if let Err(error) = app.stop(&mut runtime).await {
                eprintln!("browser-control: idle checkpoint/stop failed: {error:#}");
                runtime.error =
                    Some("Storage checkpoint failed; browser retained for recovery.".into());
            }
        }
    }
}
fn notify_ready(socket: &std::ffi::OsStr) -> Result<()> {
    use std::os::{
        linux::net::SocketAddrExt,
        unix::{
            ffi::OsStrExt,
            net::{SocketAddr, UnixDatagram},
        },
    };
    let address = match socket.as_bytes().strip_prefix(b"@") {
        Some(name) => SocketAddr::from_abstract_name(name)?,
        None => SocketAddr::from_pathname(socket)?,
    };
    UnixDatagram::unbound()?
        .send_to_addr(b"READY=1", &address)
        .context("notify systemd of browser-controller readiness")?;
    Ok(())
}

pub fn run(path: &FsPath) -> Result<()> {
    let mut config: Config = serde_json::from_slice(&std::fs::read(path)?)?;
    config.validate()?;
    config.novnc = std::fs::canonicalize(&config.novnc).context("resolve noVNC asset directory")?;
    let cache: TabCache = match std::fs::read(&config.state) {
        Ok(bytes) => serde_json::from_slice(&bytes).context("invalid browser lifecycle record")?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => TabCache { tab: None },
        Err(error) => return Err(error.into()),
    };
    fn secret(name: &str) -> Result<String> {
        let value =
            std::env::var(name).with_context(|| format!("missing runtime secret {name}"))?;
        if value.len() < 32 {
            bail!("{name} is unexpectedly short");
        }
        Ok(value)
    }
    let (disconnect, _) = broadcast::channel(8);
    let csrf = std::fs::read_to_string("/proc/sys/kernel/random/uuid")?
        .trim()
        .to_owned();
    let stock =
        std::fs::read_to_string(config.novnc.join("vnc.html")).context("Read packaged noVNC UI")?;
    let (page, csp) = page(&csrf, &stock)?;
    let app = Arc::new(App {
        config,
        client: reqwest::Client::builder()
            .timeout(Duration::from_secs(50))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()?,
        access: secret("CAMOFOX_ACCESS_KEY")?,
        admin: secret("CAMOFOX_ADMIN_KEY")?,
        agent: format!("Bearer {}", secret("HERMES_BROWSER_KEY")?),
        proxy: secret("HERMES_VIEWER_KEY")?,
        csrf,
        page,
        csp,
        transport_timeout: Duration::from_secs(10),
        runtime: Arc::new(Mutex::new(Runtime::default())),
        disconnect,
    });
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?
        .block_on(async {
            // Passive recovery preserves an existing live browser; no task replay.
            let running = app.initial_status(Duration::from_secs(30)).await?;
            {
                let mut runtime = app.runtime.lock().await;
                runtime.browser = if running {
                    Browser::Undiscovered(cache.tab)
                } else {
                    Browser::Stopped
                };
            }
            let housekeeping = tokio::spawn(housekeeping(app.clone()));
            let router = Router::new()
                .route("/health", get(health))
                .route("/browser/", get(landing))
                .route("/browser/status", get(status))
                .route("/browser/start", post(open))
                .route("/browser/disconnect", post(close_viewer))
                .route("/browser/websockify", get(websocket))
                .route("/browser/novnc/{*path}", get(asset))
                .fallback(api)
                .layer(DefaultBodyLimit::max(65536))
                .with_state(app.clone());
            let listener = tokio::net::TcpListener::bind(app.config.listen).await?;
            if let Some(socket) = std::env::var_os("NOTIFY_SOCKET") {
                notify_ready(&socket)?;
            }
            let disconnect = app.disconnect.clone();
            let shutdown = async move {
                let mut terminate =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("SIGTERM handler");
                tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
                let _ = disconnect.send(());
            };
            let served = axum::serve(listener, router)
                .with_graceful_shutdown(shutdown)
                .await;
            housekeeping.abort();
            // Drain accepted HTTP operations before the final checkpoint. A
            // late upgrade also receives the disconnect after draining.
            let _ = app.disconnect.send(());
            let mut runtime = app.runtime.lock().await;
            if let Err(error) = app.stop(&mut runtime).await {
                eprintln!(
                    "browser-control: shutdown checkpoint failed, browser retained: {error:#}"
                );
            }
            served?;
            Ok(())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    const STOCK_PAGE: &str = "<html><head><script type=\"module\">stock bootstrap</script></head><body><button id=\"noVNC_connect_button\">Connect</button><textarea id=\"noVNC_keyboardinput\"></textarea></body></html>";
    struct Fixture {
        app: Shared,
        _directory: tempfile::TempDir,
        task: tokio::task::JoinHandle<()>,
        counts: Arc<Mutex<Counts>>,
    }
    #[derive(Default)]
    struct Counts {
        running: bool,
        starts: u32,
        checkpoints: u32,
        fail_checkpoint: bool,
        status_reads: u32,
        tab_reads: u32,
        vnc_reads: u32,
        snapshots: u32,
        missing_tab: bool,
        tab_exists: bool,
        fail_tabs: bool,
        typed: Vec<String>,
        snapshot_text: Option<String>,
        created_tabs: u32,
        navigations: Vec<String>,
        delayed_creation: Option<Arc<StatusWait>>,
        delayed_status: Option<Arc<StatusWait>>,
        delayed_tabs: Option<Arc<StatusWait>>,
    }
    #[derive(Default)]
    struct StatusWait {
        started: tokio::sync::Notify,
        release: tokio::sync::Notify,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            self.task.abort();
        }
    }
    async fn fixture() -> Fixture {
        let counts = Arc::new(Mutex::new(Counts {
            tab_exists: true,
            ..Counts::default()
        }));
        let backend = Router::new()
            .route("/", get(|State(counts): State<Arc<Mutex<Counts>>>| async move {
                let (delay, running) = {
                    let mut counts = counts.lock().await;
                    counts.status_reads += 1;
                    (counts.delayed_status.take(), counts.running)
                };
                if let Some(delay) = delay {
                    delay.started.notify_one();
                    delay.release.notified().await;
                }
                axum::Json(json!({"running":running}))
            }))
            .route("/start", post(|State(counts): State<Arc<Mutex<Counts>>>| async move { let mut c = counts.lock().await; c.running = true; c.starts += 1; axum::Json(json!({"ok":true})) }))
            .route("/stop", post(|State(counts): State<Arc<Mutex<Counts>>>| async move { let mut c = counts.lock().await; assert!(c.checkpoints > 0); c.running = false; axum::Json(json!({"ok":true})) }))
            .route("/tabs", get(|State(counts): State<Arc<Mutex<Counts>>>| async move {
                let (delay, tabs) = {
                    let mut counts = counts.lock().await;
                    counts.tab_reads += 1;
                    if counts.fail_tabs {
                        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                    }
                    let tabs = if counts.tab_exists {
                        json!({"tabs":[{"tabId":"newer-other-tab","listItemId":"other","url":"https://example.com/other"},{"tabId":"shared-tab","listItemId":USER,"url":"https://example.com/shared"}]})
                    } else { json!({"tabs":[]}) };
                    (counts.delayed_tabs.take(), tabs)
                };
                if let Some(delay) = delay {
                    delay.started.notify_one();
                    delay.release.notified().await;
                }
                axum::Json(tabs).into_response()
            }).post(|State(counts): State<Arc<Mutex<Counts>>>, axum::Json(data): axum::Json<Value>| async move {
                if data.get("url").is_some_and(|url| !url.as_str().is_some_and(|url| url.starts_with("https://") || url.starts_with("http://"))) {
                    return (StatusCode::BAD_REQUEST, axum::Json(json!({"error":"Only HTTP(S) URLs allowed"}))).into_response();
                }
                assert_eq!(data["userId"], USER);
                assert_eq!(data["listItemId"], USER);
                let delay = counts.lock().await.delayed_creation.take();
                if let Some(delay) = delay {
                    delay.started.notify_one();
                    delay.release.notified().await;
                }
                let mut counts = counts.lock().await;
                counts.created_tabs += 1;
                counts.tab_exists = true;
                axum::Json(json!({"tabId":"shared-tab"})).into_response()
            }))
            .route("/tabs/shared-tab/navigate", post(|State(counts): State<Arc<Mutex<Counts>>>, axum::Json(data): axum::Json<Value>| async move {
                assert_eq!(data["userId"], USER);
                let url = data["url"].as_str().unwrap().to_string();
                counts.lock().await.navigations.push(url.clone());
                axum::Json(json!({"ok":true,"url":url}))
            }))
            .route("/tabs/shared-tab/snapshot", get(|State(counts): State<Arc<Mutex<Counts>>>, axum::extract::OriginalUri(uri): axum::extract::OriginalUri| async move {
                let mut counts = counts.lock().await;
                counts.snapshots += 1;
                if counts.missing_tab {
                    return (StatusCode::NOT_FOUND, axum::Json(json!({"error":"Tab not found"}))).into_response();
                }
                let snapshot = counts.snapshot_text.clone().unwrap_or_else(|| "current page".into());
                axum::Json(json!({"snapshot":snapshot, "query":uri.query()})).into_response()
            }))
            .route("/tabs/shared-tab/type", post(|State(counts): State<Arc<Mutex<Counts>>>, axum::Json(data): axum::Json<Value>| async move {
                assert_eq!(data["userId"], USER);
                counts.lock().await.typed.push(data["text"].as_str().unwrap().to_owned());
                axum::Json(json!({"ok":true}))
            }))
            .route("/vnc/status", get(|State(counts): State<Arc<Mutex<Counts>>>| async move {
                counts.lock().await.vnc_reads += 1;
                axum::Json(json!({"running":true}))
            }))
            .route("/sessions/home-browser/storage_state", get(|State(counts): State<Arc<Mutex<Counts>>>| async move {
                let mut c = counts.lock().await;
                if c.fail_checkpoint { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); }
                c.checkpoints += 1; axum::Json(json!({"cookies":[],"origins":[]})).into_response()
            })).with_state(counts.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, backend).await.unwrap();
        });
        let directory = tempfile::tempdir().unwrap();
        let (disconnect, _) = broadcast::channel(8);
        let (page, csp) = page("test-csrf", STOCK_PAGE).unwrap();
        let app = Arc::new(App {
            config: Config {
                listen: "127.0.0.1:0".parse().unwrap(),
                backend: format!("http://{address}").parse().unwrap(),
                websocket: "ws://127.0.0.1:1".parse().unwrap(),
                origin: "https://example.test".into(),
                state: directory.path().join("lifecycle.json"),
                novnc: directory.path().into(),
            },
            client: reqwest::Client::new(),
            access: "test-access".into(),
            admin: "test-admin".into(),
            agent: "Bearer test-agent".into(),
            proxy: "test-proxy".into(),
            csrf: "test-csrf".into(),
            page,
            csp,
            transport_timeout: Duration::from_millis(50),
            runtime: Arc::new(Mutex::new(Runtime::default())),
            disconnect,
        });
        Fixture {
            app,
            _directory: directory,
            task,
            counts,
        }
    }
    fn viewer_headers() -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("x-hermes-viewer-key", "test-proxy".parse().unwrap());
        h.insert(header::ORIGIN, "https://example.test".parse().unwrap());
        h.insert("x-hermes-csrf", "test-csrf".parse().unwrap());
        h
    }
    #[test]
    fn readiness_notifications_support_filesystem_and_abstract_systemd_sockets() {
        use std::os::{
            linux::net::SocketAddrExt,
            unix::net::{SocketAddr, UnixDatagram},
        };
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notify.sock");
        let name = format!(
            "hermes-{}-{}",
            std::process::id(),
            directory.path().file_name().unwrap().to_str().unwrap()
        );
        let abstract_socket = format!("@{name}");
        let abstract_address = SocketAddr::from_abstract_name(name.as_bytes()).unwrap();
        for (receiver, target) in [
            (UnixDatagram::bind(&path).unwrap(), path.as_os_str()),
            (
                UnixDatagram::bind_addr(&abstract_address).unwrap(),
                std::ffi::OsStr::new(&abstract_socket),
            ),
        ] {
            receiver
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            notify_ready(target).unwrap();
            let mut message = [0; 64];
            let length = receiver.recv(&mut message).unwrap();
            assert_eq!(&message[..length], b"READY=1");
        }
    }
    #[tokio::test]
    async fn controller_waits_for_backend_listener_but_rejects_invalid_status() {
        let mut f = fixture().await;
        let socket = tokio::net::TcpSocket::new_v4().unwrap();
        socket.bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = socket.local_addr().unwrap();
        Arc::get_mut(&mut f.app).unwrap().config.backend =
            format!("http://{address}").parse().unwrap();
        let error = f
            .app
            .initial_status(Duration::from_millis(20))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("startup deadline"));
        {
            let readiness = f.app.initial_status(Duration::from_secs(2));
            tokio::pin!(readiness);
            assert!(
                tokio::time::timeout(Duration::from_millis(30), &mut readiness)
                    .await
                    .is_err()
            );
            let task = tokio::spawn(async move {
                let router = Router::new()
                    .route("/", get(|| async { axum::Json(json!({"running":false})) }));
                axum::serve(socket.listen(8).unwrap(), router)
                    .await
                    .unwrap();
            });
            assert!(!readiness.await.unwrap());
            task.abort();
        }

        for (status, body, diagnostic) in [
            (StatusCode::UNAUTHORIZED, json!({}), "401"),
            (StatusCode::INTERNAL_SERVER_ERROR, json!({}), "500"),
            (StatusCode::OK, json!({}), "running boolean"),
            (
                StatusCode::OK,
                json!({"running":"false"}),
                "running boolean",
            ),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            Arc::get_mut(&mut f.app).unwrap().config.backend =
                format!("http://{address}").parse().unwrap();
            let task = tokio::spawn(async move {
                let router = Router::new()
                    .route("/", get(move || async move { (status, axum::Json(body)) }));
                axum::serve(listener, router).await.unwrap();
            });
            let error = tokio::time::timeout(
                Duration::from_secs(1),
                f.app.initial_status(Duration::from_secs(30)),
            )
            .await
            .expect("invalid status was retried")
            .unwrap_err();
            assert!(format!("{error:#}").contains(diagnostic));
            task.abort();
        }
    }
    fn request(method: Method, path: &str, body: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(path)
            .header(header::AUTHORIZATION, "Bearer test-agent")
            .body(Body::from(body.to_owned()))
            .unwrap()
    }
    async fn json_body(response: Response) -> Value {
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap()
    }
    #[test]
    fn stock_ui_keeps_native_controls_and_rejects_changed_bootstrap_contract() {
        let (html, csp) = page("nonce-test", STOCK_PAGE).unwrap();
        let html = std::str::from_utf8(&html).unwrap();
        assert!(html.contains("id=\"noVNC_connect_button\""));
        assert!(html.contains("id=\"noVNC_keyboardinput\""));
        assert!(html.contains("<base href=\"/browser/novnc/\">"));
        assert!(html.contains("nonce=\"nonce-test\""));
        assert!(html.contains("'X-Hermes-CSRF': 'nonce-test'"));
        assert!(!html.contains("stock bootstrap"));
        assert!(!html.contains("__CSRF__"));
        assert!(csp.to_str().unwrap().contains("base-uri 'self'"));
        for changed in [
            STOCK_PAGE.replace("type=\"module\"", "type=\"application/javascript\""),
            STOCK_PAGE.replace("noVNC_connect_button", "changed-connect"),
            STOCK_PAGE.replace("noVNC_keyboardinput", "changed-keyboard"),
        ] {
            assert!(page("nonce-test", &changed).is_err());
        }
    }
    #[tokio::test]
    async fn stock_ui_assets_are_authenticated_and_confined_to_the_package() {
        use std::os::unix::fs::symlink;
        let f = fixture().await;
        for (path, mime) in [
            ("app/ui.js", "text/javascript"),
            ("app/styles/base.css", "text/css"),
            ("app/images/keyboard.svg", "image/svg+xml"),
            ("app/images/icons/novnc-192x192.png", "image/png"),
            ("app/locale/fr.json", "application/json"),
            ("package.json", "application/json"),
            ("app/sounds/bell.oga", "audio/ogg"),
            ("app/sounds/bell.mp3", "audio/mpeg"),
        ] {
            let file = f.app.config.novnc.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, b"packaged asset").unwrap();
            let response = asset(State(f.app.clone()), Path(path.into()), viewer_headers())
                .await
                .unwrap();
            assert_eq!(response.headers()[header::CONTENT_TYPE], mime);
        }
        assert_eq!(
            asset(
                State(f.app.clone()),
                Path("package.json".into()),
                HeaderMap::new()
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            asset(
                State(f.app.clone()),
                Path("../private.json".into()),
                viewer_headers()
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::FORBIDDEN
        );
        let outside = tempfile::NamedTempFile::new().unwrap();
        symlink(outside.path(), f.app.config.novnc.join("escape.json")).unwrap();
        assert_eq!(
            asset(
                State(f.app.clone()),
                Path("escape.json".into()),
                viewer_headers()
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(f.counts.lock().await.starts, 0);
    }
    #[test]
    fn surface_and_idle_policy() {
        assert_eq!(
            tool_route(&Method::GET, "/tabs/shared-tab/snapshot"),
            Some(ToolRoute::Tab("shared-tab"))
        );
        assert_eq!(tool_route(&Method::POST, "/tabs"), Some(ToolRoute::Tabs));
        for (method, path) in [
            (Method::DELETE, "/sessions/home-browser"),
            (Method::POST, "/stop"),
            (Method::POST, "/agent/claim"),
            (Method::POST, "/tabs/tab/evaluate"),
            (Method::GET, "/tabs/%2e%2e/stop"),
            (Method::GET, "/tabs/shared-tab/snapshot/extra"),
            (Method::GET, "/tabs//snapshot"),
        ] {
            assert_eq!(tool_route(&method, path), None);
        }
        let mut runtime = Runtime {
            browser: Browser::Ready("shared-tab".into()),
            ..Runtime::default()
        };
        assert!(!idle(&runtime));
        runtime.last_activity = Instant::now() - Duration::from_secs(599);
        assert!(
            !idle(&runtime),
            "Browser must remain open throughout the ten-minute grace period"
        );
        runtime.last_activity = Instant::now() - Duration::from_secs(IDLE + 1);
        assert!(idle(&runtime));
        runtime.viewers = 1;
        assert!(!idle(&runtime));
    }
    #[tokio::test]
    async fn public_listener_or_remote_backend_configuration_fails_closed() {
        let mut f = fixture().await;
        let config = &mut Arc::get_mut(&mut f.app).unwrap().config;
        config.origin = "https://example.test".into();
        assert!(config.validate().is_ok());
        config.listen = "0.0.0.0:9378".parse().unwrap();
        assert!(config.validate().is_err());
        config.listen = "127.0.0.1:9378".parse().unwrap();
        config.backend = "http://example.test:9377".parse().unwrap();
        assert!(config.validate().is_err());
        config.backend = "http://127.0.0.1:9377".parse().unwrap();
        config.websocket = "ws://example.test:6080/websockify".parse().unwrap();
        assert!(config.validate().is_err());
        config.websocket = "ws://127.0.0.1:6080/websockify".parse().unwrap();
        config.backend = "http://[::1]:9377".parse().unwrap();
        config.websocket = "ws://[::1]:6080/websockify".parse().unwrap();
        assert!(config.validate().is_ok());
        config.origin = "http://example.test".into();
        assert!(config.validate().is_err());
        config.origin = "https://example.test/browser/".into();
        assert!(config.validate().is_err());
    }
    #[test]
    fn persisted_state_is_only_a_tab_cache_and_selection_is_unambiguous() {
        let cache: TabCache = serde_json::from_value(json!({"tab":"remembered"})).unwrap();
        let runtime = Runtime {
            browser: Browser::Undiscovered(cache.tab.clone()),
            ..Runtime::default()
        };
        assert!(!idle(&runtime));
        assert_eq!(
            serde_json::to_value(&cache).unwrap(),
            json!({"tab":"remembered"})
        );
        let tabs = json!({"tabs":[
            {"tabId":"first","listItemId":USER},
            {"tabId":"second","listItemId":USER},
        ]});
        assert_eq!(
            shared_tab(&tabs, Some("first")).unwrap().as_deref(),
            Some("first")
        );
        assert!(shared_tab(&tabs, None).is_err());
        assert!(shared_tab(&json!({}), None).is_err());
    }
    #[tokio::test]
    async fn fresh_browser_creates_blank_tab_without_rejected_navigation_url() {
        let f = fixture().await;
        f.counts.lock().await.tab_exists = false;
        open(State(f.app.clone()), viewer_headers()).await.unwrap();
        assert_eq!(f.counts.lock().await.created_tabs, 1);
        api(State(f.app.clone()), request(Method::GET, "/tabs", ""))
            .await
            .unwrap();
        assert_eq!(f.counts.lock().await.created_tabs, 1);
        assert_eq!(f.app.runtime.lock().await.browser.tab(), Some("shared-tab"));
    }
    #[tokio::test]
    async fn native_adoption_probes_stay_cold_then_create_navigates_requested_url() {
        let f = fixture().await;
        f.counts.lock().await.tab_exists = false;
        for _ in 0..2 {
            let response = api(
                State(f.app.clone()),
                request(Method::GET, "/tabs?userId=home-browser", ""),
            )
            .await
            .unwrap();
            assert_eq!(json_body(response).await, json!({"tabs":[]}));
        }
        assert_eq!(f.counts.lock().await.starts, 0);
        assert_eq!(f.counts.lock().await.created_tabs, 0);
        api(State(f.app.clone()), request(Method::POST, "/tabs", r#"{"userId":"home-browser","listItemId":"home-browser","url":"https://en.wikipedia.org/wiki/Hermes"}"#)).await.unwrap();
        let counts = f.counts.lock().await;
        assert_eq!(counts.starts, 1);
        assert_eq!(counts.created_tabs, 1);
        assert_eq!(counts.navigations, ["https://en.wikipedia.org/wiki/Hermes"]);
    }
    #[tokio::test]
    async fn tab_listing_reports_the_shared_tab_url_for_credential_fills() {
        let f = fixture().await;
        api(State(f.app.clone()), request(Method::POST, "/tabs", "{}"))
            .await
            .unwrap();
        let response = api(State(f.app.clone()), request(Method::GET, "/tabs", ""))
            .await
            .unwrap();
        assert_eq!(
            json_body(response).await,
            json!({"tabs":[{"tabId":"shared-tab","userId":USER,"listItemId":USER,"url":"https://example.com/shared"}]})
        );
        // A tab the native browser no longer lists carries no URL, so the
        // credential bridge never matches it.
        f.counts.lock().await.tab_exists = false;
        let response = api(State(f.app.clone()), request(Method::GET, "/tabs", ""))
            .await
            .unwrap();
        assert_eq!(
            json_body(response).await,
            json!({"tabs":[{"tabId":"shared-tab","userId":USER,"listItemId":USER}]})
        );
        assert_eq!(f.app.runtime.lock().await.browser.tab(), Some("shared-tab"));
    }
    #[tokio::test]
    async fn ready_tab_listing_failures_do_not_fail_the_passive_probe() {
        let f = fixture().await;
        {
            let mut counts = f.counts.lock().await;
            counts.running = true;
            counts.fail_tabs = true;
        }
        f.app.runtime.lock().await.browser = Browser::Ready("shared-tab".into());
        // The entry survives without the URL the listing could not supply, so
        // the adoption probe cannot mistake a live tab for a cold browser.
        let response = api(
            State(f.app.clone()),
            request(Method::GET, "/tabs?userId=home-browser", ""),
        )
        .await
        .unwrap();
        assert_eq!(
            json_body(response).await,
            json!({"tabs":[{"tabId":"shared-tab","userId":USER,"listItemId":USER}]})
        );
        // Without a live tab the listing is load-bearing, so its failure stays
        // fatal for the caller.
        f.app.runtime.lock().await.browser = Browser::Undiscovered(None);
        let error = api(
            State(f.app.clone()),
            request(Method::GET, "/tabs?userId=home-browser", ""),
        )
        .await
        .unwrap_err();
        assert_eq!(error.0, StatusCode::BAD_GATEWAY);
    }
    #[tokio::test]
    async fn credential_fill_values_are_redacted_from_later_page_responses() {
        let f = fixture().await;
        api(State(f.app.clone()), request(Method::POST, "/tabs", "{}"))
            .await
            .unwrap();
        f.counts.lock().await.snapshot_text = Some("hunter2 and visible text".into());
        api(
            State(f.app.clone()),
            request(
                Method::POST,
                "/tabs/shared-tab/type",
                r#"{"userId":"home-browser","ref":"e1","text":"hunter2","mode":"fill","expectedOrigin":"https://example.com"}"#,
            ),
        )
        .await
        .unwrap();
        // The value reached the browser, and only the later response hides it.
        assert_eq!(f.counts.lock().await.typed, ["hunter2"]);
        let response = api(
            State(f.app.clone()),
            request(Method::GET, "/tabs/shared-tab/snapshot", ""),
        )
        .await
        .unwrap();
        assert_eq!(
            json_body(response).await["snapshot"],
            "[redacted] and visible text"
        );
    }
    #[test]
    fn credential_redaction_covers_quoted_values_and_leaves_other_bodies_alone() {
        let secrets = vec!["hunter2".to_owned(), "quo\"te".to_owned()];
        let body = json!({
            "url": "https://example.com/login",
            "snapshot": "- textbox \"Password\": hunter2\n- textbox \"Code\": \"quo\\\"te\"\n- textbox \"User\": hunter3",
            "structure": {"forms": [{"fields": [{"value": "hunter2"}, {"value": "unrelated"}]}]},
        });
        let bytes = Bytes::from(serde_json::to_vec(&body).unwrap());
        let redacted: Value =
            serde_json::from_slice(&redact_response(&bytes, &secrets).unwrap()).unwrap();
        assert_eq!(
            redacted["snapshot"],
            "- textbox \"Password\": [redacted]\n- textbox \"Code\": \"[redacted]\"\n- textbox \"User\": hunter3"
        );
        assert_eq!(
            redacted["structure"]["forms"][0]["fields"][0]["value"],
            "[redacted]"
        );
        assert_eq!(
            redacted["structure"]["forms"][0]["fields"][1]["value"],
            "unrelated"
        );
        assert_eq!(redacted["url"], "https://example.com/login");
        // Only JSON responses are rewritten; an image body passes through.
        assert!(redact_response(&Bytes::from_static(b"\x89PNG hunter2"), &secrets).is_none());
    }
    #[test]
    fn remembered_secrets_stay_bounded_and_scoped_to_their_tab() {
        let mut runtime = Runtime::default();
        runtime.remember_secret("tab-a", "first".into());
        runtime.remember_secret("tab-a", "first".into());
        runtime.remember_secret("tab-b", "second".into());
        assert_eq!(runtime.secrets_for("tab-a"), vec!["first".to_owned()]);
        assert_eq!(runtime.secrets_for("tab-b"), vec!["second".to_owned()]);
        assert!(runtime.secrets_for("tab-c").is_empty());
        for index in 0..MAX_SECRETS + 4 {
            runtime.remember_secret("tab-a", format!("value-{index}"));
        }
        let remembered = runtime.secrets_for("tab-a");
        let newest = format!("value-{}", MAX_SECRETS + 3);
        assert_eq!(remembered.len(), MAX_SECRETS);
        assert_eq!(remembered.last().map(String::as_str), Some(newest.as_str()));
    }
    #[tokio::test]
    async fn canceled_startup_does_not_release_inflight_tab_creation() {
        let f = fixture().await;
        let delay = Arc::new(StatusWait::default());
        {
            let mut counts = f.counts.lock().await;
            counts.tab_exists = false;
            counts.delayed_creation = Some(delay.clone());
        }
        let app = f.app.clone();
        let first = tokio::spawn(async move { open(State(app), viewer_headers()).await });
        delay.started.notified().await;
        first.abort();
        assert!(first.await.unwrap_err().is_cancelled());
        let app = f.app.clone();
        let second = tokio::spawn(async move { open(State(app), viewer_headers()).await });
        delay.release.notify_one();
        tokio::time::timeout(Duration::from_secs(2), second)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let counts = f.counts.lock().await;
        assert_eq!(counts.starts, 1);
        assert_eq!(counts.created_tabs, 1);
        assert_eq!(f.app.runtime.lock().await.browser.tab(), Some("shared-tab"));
    }
    #[tokio::test]
    async fn warm_tools_do_not_rediscover_tabs_poll_vnc_or_write_state() {
        use std::os::unix::fs::MetadataExt;
        let f = fixture().await;
        api(State(f.app.clone()), request(Method::POST, "/tabs", "{}"))
            .await
            .unwrap();
        // Keep the original inode alive so deletion/replacement cannot reuse
        // its number and accidentally make a write regression pass.
        let persisted = std::fs::File::open(&f.app.config.state).unwrap();
        let inode = persisted.metadata().unwrap().ino();
        let before = {
            let counts = f.counts.lock().await;
            (
                counts.status_reads,
                counts.tab_reads,
                counts.vnc_reads,
                counts.snapshots,
            )
        };
        for _ in 0..25 {
            api(
                State(f.app.clone()),
                request(Method::GET, "/tabs/shared-tab/snapshot", ""),
            )
            .await
            .unwrap();
        }
        let counts = f.counts.lock().await;
        assert_eq!(counts.status_reads - before.0, 25);
        assert_eq!(counts.tab_reads - before.1, 0);
        assert_eq!(counts.vnc_reads - before.2, 0);
        assert_eq!(counts.snapshots - before.3, 25);
        assert_eq!(std::fs::metadata(&f.app.config.state).unwrap().ino(), inode);
        println!(
            "25 warm snapshots: 50 backend requests, zero tab/VNC probes and zero state replacements (previous path: 100 requests/50 replacements)"
        );
    }
    #[tokio::test]
    async fn slow_passive_status_does_not_hold_the_lifecycle_lock() {
        let f = fixture().await;
        let delay = Arc::new(StatusWait::default());
        f.counts.lock().await.delayed_status = Some(delay.clone());
        let app = f.app.clone();
        let pending = tokio::spawn(async move { status(State(app), viewer_headers()).await });
        delay.started.notified().await;
        let operation = tokio::time::timeout(
            Duration::from_secs(2),
            api(State(f.app.clone()), request(Method::POST, "/tabs", "{}")),
        )
        .await;
        delay.release.notify_one();
        pending.await.unwrap().unwrap();
        operation
            .expect("passive status blocked a real operation")
            .unwrap();
    }
    #[tokio::test]
    async fn slow_adoption_probes_do_not_block_or_overwrite_real_browser_operations() {
        for probe in ["status", "tabs"] {
            let f = fixture().await;
            if probe == "tabs" {
                api(State(f.app.clone()), request(Method::POST, "/tabs", "{}"))
                    .await
                    .unwrap();
                f.app.runtime.lock().await.browser = Browser::Undiscovered(None);
            } else {
                f.counts.lock().await.tab_exists = false;
            }
            let delay = Arc::new(StatusWait::default());
            {
                let mut counts = f.counts.lock().await;
                if probe == "tabs" {
                    counts.delayed_tabs = Some(delay.clone());
                } else {
                    counts.delayed_status = Some(delay.clone());
                }
            }
            let app = f.app.clone();
            let pending =
                tokio::spawn(
                    async move { api(State(app), request(Method::GET, "/tabs", "")).await },
                );
            delay.started.notified().await;
            let operation = tokio::time::timeout(
                Duration::from_secs(2),
                api(State(f.app.clone()), request(Method::POST, "/tabs", "{}")),
            )
            .await;
            delay.release.notify_one();
            pending.await.unwrap().unwrap();
            operation
                .expect("adoption blocked a real operation")
                .unwrap();
            assert_eq!(f.counts.lock().await.starts, 1);
            assert!(
                matches!(&f.app.runtime.lock().await.browser, Browser::Ready(tab) if tab == "shared-tab")
            );
        }
    }
    #[tokio::test]
    async fn missing_native_tab_is_rediscovered_without_replaying_the_request() {
        let f = fixture().await;
        api(State(f.app.clone()), request(Method::POST, "/tabs", "{}"))
            .await
            .unwrap();
        f.counts.lock().await.missing_tab = true;
        let response = api(
            State(f.app.clone()),
            request(Method::GET, "/tabs/shared-tab/snapshot", ""),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(f.counts.lock().await.snapshots, 1);
        assert!(matches!(
            f.app.runtime.lock().await.browser,
            Browser::Undiscovered(None)
        ));
        f.counts.lock().await.missing_tab = false;
        api(State(f.app.clone()), request(Method::POST, "/tabs", "{}"))
            .await
            .unwrap();
        assert_eq!(f.counts.lock().await.tab_reads, 2);
        assert_eq!(f.counts.lock().await.starts, 1);
        assert_eq!(f.app.runtime.lock().await.browser.tab(), Some("shared-tab"));
    }
    #[tokio::test]
    async fn passive_and_unauthorized_requests_do_not_wake() {
        let f = fixture().await;
        let last_activity = f.app.runtime.lock().await.last_activity;
        assert_eq!(
            landing(State(f.app.clone()), HeaderMap::new())
                .await
                .unwrap_err()
                .0,
            StatusCode::UNAUTHORIZED
        );
        assert!(
            landing(State(f.app.clone()), viewer_headers())
                .await
                .is_ok()
        );
        assert!(status(State(f.app.clone()), viewer_headers()).await.is_ok());
        let mut headers = viewer_headers();
        headers.insert(header::ORIGIN, "https://wrong.test".parse().unwrap());
        assert_eq!(
            open(State(f.app.clone()), headers).await.unwrap_err().0,
            StatusCode::FORBIDDEN
        );
        let mut headers = viewer_headers();
        headers.remove("x-hermes-csrf");
        assert_eq!(
            open(State(f.app.clone()), headers).await.unwrap_err().0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(f.counts.lock().await.starts, 0);
        assert_eq!(f.app.runtime.lock().await.last_activity, last_activity);
    }
    #[tokio::test]
    async fn native_browser_calls_need_no_task_hook_and_remain_allowed_with_viewer() {
        let f = fixture().await;
        assert!(
            api(State(f.app.clone()), request(Method::POST, "/tabs", "{}"))
                .await
                .is_ok()
        );
        assert_eq!(f.app.runtime.lock().await.browser.tab(), Some("shared-tab"));
        assert!(open(State(f.app.clone()), viewer_headers()).await.is_ok());
        f.app.runtime.lock().await.viewers = 1;
        assert!(
            api(
                State(f.app.clone()),
                request(Method::GET, "/tabs/shared-tab/snapshot", "")
            )
            .await
            .is_ok()
        );
        assert!(
            api(
                State(f.app.clone()),
                request(
                    Method::POST,
                    "/tabs",
                    r#"{"userId":"home-browser","listItemId":"home-browser"}"#
                )
            )
            .await
            .is_ok()
        );
        assert_eq!(f.counts.lock().await.starts, 1);
        assert_eq!(
            api(
                State(f.app.clone()),
                request(Method::GET, "/tabs/old-tab/snapshot", "")
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::NOT_FOUND
        );
    }
    #[tokio::test]
    async fn identity_and_admin_routes_are_denied_before_startup() {
        let f = fixture().await;
        for req in [
            request(Method::POST, "/stop", "{}"),
            request(Method::POST, "/agent/resume", "{}"),
            request(Method::GET, "/tabs?userId=another", ""),
            request(
                Method::GET,
                "/tabs?userId=home-browser&user%49d=another",
                "",
            ),
            request(Method::POST, "/tabs", r#"{"userId":"another"}"#),
            request(Method::GET, "/tabs?listItemId=another", ""),
            request(
                Method::GET,
                "/tabs?listItemId=home-browser&list%49temId=another",
                "",
            ),
            request(Method::POST, "/tabs", r#"{"listItemId":null}"#),
        ] {
            assert_eq!(
                api(State(f.app.clone()), req).await.unwrap_err().0,
                StatusCode::FORBIDDEN
            );
        }
        assert_eq!(f.counts.lock().await.starts, 0);
    }
    #[tokio::test]
    async fn non_object_browser_requests_are_rejected_before_startup() {
        let f = fixture().await;
        for body in ["[]", "null", "true", "1", "\"text\"", "{invalid"] {
            assert_eq!(
                api(State(f.app.clone()), request(Method::POST, "/tabs", body))
                    .await
                    .unwrap_err()
                    .0,
                StatusCode::BAD_REQUEST
            );
        }
        assert_eq!(f.counts.lock().await.starts, 0);
    }
    #[tokio::test]
    async fn identity_query_fields_are_rebound_without_losing_tool_parameters() {
        use std::borrow::Cow;
        let f = fixture().await;
        let response = api(
            State(f.app.clone()),
            request(
                Method::GET,
                "/tabs/shared-tab/snapshot?userId=home-browser&user%49d=home-browser&listItemId=home-browser&compact=true&label=a%2Bb%20%26x",
                "",
            ),
        )
        .await
        .unwrap();
        let data = json_body(response).await;
        let query = url::form_urlencoded::parse(data["query"].as_str().unwrap().as_bytes())
            .collect::<Vec<_>>();
        assert_eq!(
            query,
            [("compact", "true"), ("label", "a+b &x"), ("userId", USER)]
                .map(|(key, value)| (Cow::Borrowed(key), Cow::Borrowed(value)))
        );
    }
    #[tokio::test]
    async fn stalled_or_cancelled_websocket_upgrades_release_viewer_and_idle_cleanup() {
        use futures_util::FutureExt;
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        for cancel_before_upgrade in [false, true] {
            let mut f = fixture().await;
            // A listening socket that never answers the WebSocket handshake.
            let stalled = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = stalled.local_addr().unwrap();
            Arc::get_mut(&mut f.app).unwrap().config.websocket =
                format!("ws://{address}").parse().unwrap();
            f.app.runtime.lock().await.browser = Browser::Ready("shared-tab".into());
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let router = Router::new()
                .route(
                    "/browser/websockify",
                    get(
                        move |State(app): State<Shared>,
                              headers: HeaderMap,
                              ws: WebSocketUpgrade| async move {
                            let response = websocket(State(app.clone()), headers, ws).await?;
                            if cancel_before_upgrade {
                                let _ = app.disconnect.send(());
                            }
                            WebResult::Ok(response)
                        },
                    ),
                )
                .with_state(f.app.clone());
            let server = tokio::spawn(async move {
                axum::serve(listener, router).await.unwrap();
            });
            let mut request = format!("ws://{address}/browser/websockify")
                .into_client_request()
                .unwrap();
            request
                .headers_mut()
                .insert("x-hermes-viewer-key", "test-proxy".parse().unwrap());
            request
                .headers_mut()
                .insert(header::ORIGIN, "https://example.test".parse().unwrap());
            let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
            tokio::time::timeout(Duration::from_secs(2), async {
                while socket.next().await.is_some_and(|message| {
                    !matches!(
                        message,
                        Ok(tokio_tungstenite::tungstenite::Message::Close(_))
                    )
                }) {}
                while f.app.runtime.lock().await.viewers != 0 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("stalled or cancelled upgrade leaked a viewer");
            if cancel_before_upgrade {
                assert!(
                    stalled.accept().now_or_never().is_none(),
                    "cancelled viewer opened an upstream connection"
                );
            }
            let mut runtime = f.app.runtime.lock().await;
            runtime.last_activity = Instant::now() - Duration::from_secs(IDLE + 1);
            assert!(idle(&runtime));
            server.abort();
        }
    }
    #[tokio::test]
    async fn websocket_backpressure_has_a_write_deadline() {
        let sink = futures_util::sink::unfold((), |(), _: Message| async {
            std::future::pending::<std::result::Result<(), std::io::Error>>().await
        });
        futures_util::pin_mut!(sink);
        let error = transport(
            "write",
            Duration::from_millis(20),
            sink.send(Message::Ping(Bytes::new())),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("write timed out"));
    }
    #[tokio::test]
    async fn failed_storage_checkpoint_retains_browser_then_success_stops_it() {
        let f = fixture().await;
        assert!(
            api(State(f.app.clone()), request(Method::POST, "/tabs", "{}"))
                .await
                .is_ok()
        );
        f.counts.lock().await.fail_checkpoint = true;
        let mut runtime = f.app.runtime.lock().await;
        assert!(f.app.stop(&mut runtime).await.is_err());
        assert!(f.counts.lock().await.running);
        f.counts.lock().await.fail_checkpoint = false;
        f.app.stop(&mut runtime).await.unwrap();
        assert!(matches!(runtime.browser, Browser::Stopped));
        assert_eq!(f.counts.lock().await.checkpoints, 1);
    }
}
