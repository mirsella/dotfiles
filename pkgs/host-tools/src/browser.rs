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
use serde_json::{Value, json};
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
            let loopback = match url.host() {
                Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
                Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
                _ => false,
            };
            ensure!(
                url.scheme() == scheme
                    && loopback
                    && url.username().is_empty()
                    && url.password().is_none(),
                "Private {name} must use a literal loopback address without URL credentials"
            );
        }
        let origin: reqwest::Url = self.origin.parse().context("invalid viewer origin")?;
        ensure!(
            origin.scheme() == "https" && origin.origin().ascii_serialization() == self.origin,
            "Viewer origin must be an exact HTTPS origin"
        );
        Ok(())
    }
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
    fn running(&self) -> bool {
        !matches!(self, Self::Stopped)
    }
}
struct Runtime {
    browser: Browser,
    last_activity: Instant,
    error: Option<String>,
    viewers: usize,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            browser: Browser::Stopped,
            last_activity: Instant::now(),
            error: None,
            viewers: 0,
        }
    }
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
    async fn running(&self) -> Result<bool> {
        self.backend(Method::GET, "/", None).await?["running"]
            .as_bool()
            .context("Camofox status is missing its running boolean")
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
        if !self.running().await? {
            runtime.browser = Browser::Undiscovered(None);
            // A timed-out launch may still have created Firefox. Keep it under
            // idle cleanup even if we never receive the launch response.
            runtime.last_activity = Instant::now();
            self.backend(Method::POST, "/start", Some(json!({})))
                .await?;
        }
        if !runtime.browser.running() {
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
        if self.running().await? {
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
    let running = app.running().await.map_err(failure)?;
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

fn tab_path(path: &str) -> Option<(&str, &str)> {
    let (id, suffix) = path.strip_prefix("/tabs/")?.split_once('/')?;
    (!id.is_empty()
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_')))
    .then_some((id, suffix))
}
fn permitted(method: &Method, path: &str) -> bool {
    if path == "/tabs" {
        return matches!(*method, Method::GET | Method::POST);
    }
    let Some((_, suffix)) = tab_path(path) else {
        return false;
    };
    match method.as_str() {
        "GET" => matches!(suffix, "snapshot" | "screenshot" | "downloads"),
        "POST" => matches!(
            suffix,
            "navigate" | "click" | "type" | "scroll" | "back" | "press"
        ),
        _ => false,
    }
}
async fn api(State(app): State<Shared>, request: Request<Body>) -> WebResult {
    agent(&app, request.headers())?;
    let (parts, body) = request.into_parts();
    let method = &parts.method;
    let path = parts.uri.path();
    if !permitted(method, path) {
        return Err((
            StatusCode::FORBIDDEN,
            "Outside the approved Camofox tool surface".into(),
        ));
    }
    let query = url::form_urlencoded::parse(parts.uri.query().unwrap_or_default().as_bytes());
    if query
        .clone()
        .any(|(key, value)| key == "userId" && value != USER)
    {
        return Err((
            StatusCode::FORBIDDEN,
            "Different browser identity refused".into(),
        ));
    }
    let bytes = to_bytes(body, 65536)
        .await
        .map_err(|_| (StatusCode::PAYLOAD_TOO_LARGE, "Request too large".into()))?;
    let mut data: Value = if bytes.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&bytes)
            .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid JSON".into()))?
    };
    if !data.is_object() {
        return Err((StatusCode::BAD_REQUEST, "Expected JSON object".into()));
    }
    if ["userId", "listItemId"].into_iter().any(|key| {
        data.get(key)
            .is_some_and(|value| value.as_str() != Some(USER))
    }) {
        return Err((
            StatusCode::FORBIDDEN,
            "Different browser identity refused".into(),
        ));
    }
    if path == "/tabs" && *method == Method::GET {
        // Hermes's five-second adoption probe must only discover existing
        // tabs. Creating a blank tab here can make its subsequent ensure-tab
        // path skip the requested navigation, or cancel startup mid-creation.
        let observed = app.runtime.lock().await.browser.clone();
        let running = app.running().await.map_err(failure)?;
        let discovery = if running && !matches!(observed, Browser::Ready(_)) {
            Some(
                app.backend(Method::GET, &format!("/tabs?userId={USER}"), None)
                    .await
                    .map_err(failure)?,
            )
        } else {
            None
        };
        let mut runtime = app.runtime.lock().await;
        // A real operation may have started or stopped the browser while this
        // passive probe was in flight. Its state takes precedence over ours.
        if runtime.browser == observed {
            if !running {
                runtime.browser = Browser::Stopped;
            } else if let Some(tabs) = discovery {
                let tab = shared_tab(&tabs, runtime.browser.tab()).map_err(failure)?;
                let changed = runtime.browser.tab() != tab.as_deref();
                runtime.browser = Browser::Undiscovered(tab);
                if changed {
                    app.save(&runtime).await.map_err(failure)?;
                }
            }
        }
        if runtime.browser.running() {
            runtime.last_activity = Instant::now();
        }
        let tabs = runtime.browser.tab().map_or_else(Vec::new, |tab| {
            vec![json!({"tabId":tab,"userId":USER,"listItemId":USER})]
        });
        return Ok(axum::Json(json!({"tabs":tabs})).into_response());
    }
    // One lifecycle lock coalesces cold launches and keeps idle shutdown out of
    // an actual HTTP operation. It does not reserve ownership or block viewers.
    let mut runtime = app.started(false).await.map_err(failure)?;
    let tab = runtime.browser.tab().expect("startup selects a tab");
    if path == "/tabs" {
        if let Some(url) = data["url"].as_str().filter(|url| *url != "about:blank") {
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
    if tab_path(&path).is_none_or(|(id, _)| id != tab) {
        return Err((
            StatusCode::NOT_FOUND,
            "Live tab was lost; navigate again to reopen the saved task URL".into(),
        ));
    }
    let mut url = app
        .config
        .backend
        .join(path)
        .map_err(|error| failure(error.into()))?;
    url.query_pairs_mut()
        .extend_pairs(query.filter(|(key, _)| key != "userId"))
        .append_pair("userId", USER);
    let mut upstream = app
        .client
        .request(method.clone(), url)
        .bearer_auth(&app.access);
    if *method != Method::GET {
        data["userId"] = json!(USER);
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
    if !runtime.browser.running() {
        return Err((StatusCode::CONFLICT, "Open the browser first".into()));
    }
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
        .on_upgrade(move |socket| forward(app, socket))
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
async fn forward(app: Shared, mut socket: WebSocket) {
    let mut disconnected = app.disconnect.subscribe();
    let result: Result<()> = async {
        let (mut upstream, _) = tokio::select! {
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
    runtime.browser.running()
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
            let running = app.running().await?;
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
                    let tabs = if counts.tab_exists {
                        json!({"tabs":[{"tabId":"newer-other-tab","listItemId":"other"},{"tabId":"shared-tab","listItemId":USER}]})
                    } else { json!({"tabs":[]}) };
                    (counts.delayed_tabs.take(), tabs)
                };
                if let Some(delay) = delay {
                    delay.started.notify_one();
                    delay.release.notified().await;
                }
                axum::Json(tabs)
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
            .route("/tabs/shared-tab/snapshot", get(|State(counts): State<Arc<Mutex<Counts>>>| async move {
                let mut counts = counts.lock().await;
                counts.snapshots += 1;
                if counts.missing_tab {
                    return (StatusCode::NOT_FOUND, axum::Json(json!({"error":"Tab not found"}))).into_response();
                }
                axum::Json(json!({"snapshot":"current page"})).into_response()
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
    fn request(method: Method, path: &str, body: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(path)
            .header(header::AUTHORIZATION, "Bearer test-agent")
            .body(Body::from(body.to_owned()))
            .unwrap()
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
        assert!(permitted(&Method::GET, "/tabs/shared-tab/snapshot"));
        assert!(permitted(&Method::POST, "/tabs"));
        for (method, path) in [
            (Method::DELETE, "/sessions/home-browser"),
            (Method::POST, "/stop"),
            (Method::POST, "/agent/claim"),
            (Method::POST, "/tabs/tab/evaluate"),
            (Method::GET, "/tabs/%2e%2e/stop"),
        ] {
            assert!(!permitted(&method, path));
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
            let bytes = to_bytes(response.into_body(), 65536).await.unwrap();
            assert_eq!(
                serde_json::from_slice::<Value>(&bytes).unwrap(),
                json!({"tabs":[]})
            );
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
        ] {
            assert_eq!(
                api(State(f.app.clone()), req).await.unwrap_err().0,
                StatusCode::FORBIDDEN
            );
        }
        assert_eq!(f.counts.lock().await.starts, 0);
    }
    #[tokio::test]
    async fn stalled_websocket_handshake_releases_viewer_and_idle_cleanup() {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let mut f = fixture().await;
        let stalled = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = stalled.local_addr().unwrap();
        Arc::get_mut(&mut f.app).unwrap().config.websocket =
            format!("ws://{address}").parse().unwrap();
        let stalled_peer = tokio::spawn(async move {
            let (_socket, _) = stalled.accept().await.unwrap();
            std::future::pending::<()>().await;
        });
        {
            let mut runtime = f.app.runtime.lock().await;
            runtime.browser = Browser::Ready("shared-tab".into());
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = Router::new()
            .route("/browser/websockify", get(websocket))
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
        .expect("stalled handshake leaked a viewer");
        let mut runtime = f.app.runtime.lock().await;
        runtime.last_activity = Instant::now() - Duration::from_secs(IDLE + 1);
        assert!(idle(&runtime));
        server.abort();
        stalled_peer.abort();
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
