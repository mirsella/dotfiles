use axum::{
    Router,
    body::Body,
    response::Response,
    routing::{get, post},
};
use claude_credit_router::policy::{Mode, Store};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};

struct Worker(Child);
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn sigterm_drains_an_accepted_subscription_response() {
    let (finish, pending) = tokio::sync::oneshot::channel::<()>();
    let pending = Arc::new(Mutex::new(Some(pending)));
    let upstream = Router::new()
        .route(
            "/v1/models",
            get(|| async {
                axum::Json(serde_json::json!({"object":"list","data":[{"id":"gpt-test"}]}))
            }),
        )
        .route(
            "/responses",
            post(move || {
                let pending = pending.lock().unwrap().take().unwrap();
                async move {
                    Response::new(Body::from_stream(futures_util::stream::once(async move {
                        pending.await.unwrap();
                        Ok::<_, std::io::Error>("completed response")
                    })))
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("config.json");
    let state = directory.path().join("state.json");
    Store(state.clone())
        .update(|state| {
            state.set_mode(Mode::Subscription, chrono::Utc::now());
            Ok(())
        })
        .unwrap();
    std::fs::write(
        &config,
        serde_json::to_vec(&serde_json::json!({
            "listen":"127.0.0.1:0", "upstream":upstream_url,
            "organization_id":"test-org", "renewal_anchor":"2026-11-09T00:00:00Z",
            "openai":{
                "api_upstream":upstream_url,"project_id":"proj_test",
                "enforced_project_limit_usd":150,"grant_usd":199,
                "spending":{"limit_usd":150,"codexbar_config":directory.path().join("unused.json")}
            }
        }))
        .unwrap(),
    )
    .unwrap();
    let mut child = Worker(
        Command::new(env!("CARGO_BIN_EXE_claude-credit-router"))
            .args(["--openai", "--config"])
            .arg(config)
            .arg("--state")
            .arg(state)
            .arg("serve")
            .env("OPENAI_CREDIT_ROUTER_API_KEY", "sk-proj-test")
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut stderr = BufReader::new(child.0.stderr.take().unwrap());
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let (mut stderr, startup) = tokio::task::spawn_blocking(move || {
            let mut line = String::new();
            stderr.read_line(&mut line).unwrap();
            (stderr, line)
        })
        .await
        .unwrap();
        let address = startup
            .strip_prefix("claude-credit-router: listening on ")
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        let response = reqwest::Client::new()
            .post(format!("http://{address}/responses"))
            .bearer_auth("native-test")
            .body(r#"{"model":"gpt-test"}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(unsafe { libc::kill(child.0.id() as i32, libc::SIGTERM) }, 0);
        let draining = tokio::task::spawn_blocking(move || {
            let mut line = String::new();
            stderr.read_line(&mut line).unwrap();
            line
        })
        .await
        .unwrap();
        assert!(draining.contains("draining active requests"), "{draining}");
        assert!(child.0.try_wait().unwrap().is_none());
        finish.send(()).unwrap();
        assert_eq!(response.text().await.unwrap(), "completed response");
    })
    .await;
    // Kill/reap even on a failed timeout, releasing any blocked stderr reader.
    drop(child);
    server.abort();
    result.unwrap();
}
