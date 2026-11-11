use axum::body::{Body, Bytes, to_bytes};
use axum::extract::State;
use axum::http::{HeaderMap, Method, Request, Response, StatusCode};
use chrono::{DateTime, Utc};
use claude_credit_router::policy::{Mode, Route, Store, next_renewal, private_key};
use claude_credit_router::{Billing, Config, EXHAUSTED, Router, exhausted};
use serde_json::json;
use std::collections::VecDeque;
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

fn date(value: &str) -> DateTime<Utc> {
    value.parse().unwrap()
}

fn config(upstream: String) -> Config {
    Config {
        listen: "127.0.0.1:0".parse().unwrap(),
        upstream,
        api_key_file: Some("/unused-in-mock-tests".into()),
        organization_id: "personal-org".into(),
        renewal_anchor: date("2026-12-03T00:00:00Z"),
        notify_send: None,
        openai: None,
        billing: Some(Billing {
            verified_at: date("2026-01-01T00:00:00Z"),
            no_payment_method: true,
            purchased_balance_usd: 0,
            auto_reload: false,
            sales_invoicing: false,
        }),
    }
}

fn depleted() -> Reply {
    Reply(
        StatusCode::PAYMENT_REQUIRED,
        json!({"type":"error", "error":{"type":"billing_error", "message":EXHAUSTED}}).to_string(),
    )
}

struct Reply(StatusCode, String);

#[derive(Clone)]
struct Seen {
    method: Method,
    uri: String,
    headers: HeaderMap,
    body: Bytes,
}

#[derive(Clone)]
struct Upstream {
    responses: Arc<Mutex<VecDeque<Reply>>>,
    cost: Arc<Mutex<Option<f64>>>,
    cost_checks: Arc<Mutex<usize>>,
    seen: Arc<Mutex<Vec<Seen>>>,
    before_reply: Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>,
}

async fn upstream(State(state): State<Upstream>, request: Request<Body>) -> Response<Body> {
    if request.uri().path() == "/costs" {
        *state.cost_checks.lock().unwrap() += 1;
        assert_eq!(
            request.headers()["authorization"],
            "Bearer reporting-only-test"
        );
        assert_eq!(request.headers()["openai-organization"], "personal-org");
        let url = reqwest::Url::parse(&format!("http://127.0.0.1{}", request.uri())).unwrap();
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q["project_ids"], "proj_personal");
        let Some(cost) = *state.cost.lock().unwrap() else {
            return Response::builder()
                .status(503)
                .body(Body::from("unavailable"))
                .unwrap();
        };
        return Response::new(Body::from(
            json!({"object":"page","has_more":false,"data":[{
                "start_time":q["start_time"].parse::<i64>().unwrap(),
                "end_time":q["end_time"].parse::<i64>().unwrap(),
                "results":[{"object":"organization.costs.result","project_id":"proj_personal",
                    "amount":{"currency":"usd","value":cost}}]
            }]})
            .to_string(),
        ));
    }
    let (parts, body) = request.into_parts();
    let body = to_bytes(body, 129 * 1024 * 1024).await.unwrap();
    state.seen.lock().unwrap().push(Seen {
        method: parts.method,
        uri: parts.uri.to_string(),
        headers: parts.headers,
        body,
    });
    if let Some(action) = state.before_reply.lock().unwrap().take() {
        action();
    }
    let Reply(status, text) = state
        .responses
        .lock()
        .unwrap()
        .pop_front()
        .expect("unexpected extra upstream request");
    Response::builder()
        .status(status)
        .header("request-id", "unchanged-id")
        .header("anthropic-organization-id", "personal-org")
        .header("content-type", "text/event-stream")
        .body(Body::from(text))
        .unwrap()
}

struct Fixture {
    url: String,
    directory: tempfile::TempDir,
    store: Store,
    state: Upstream,
    config: Config,
    router: Router,
    clock: Arc<Mutex<DateTime<Utc>>>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

impl Fixture {
    async fn new(responses: Vec<Reply>) -> Self {
        Self::create(responses, false).await
    }

    async fn openai(responses: Vec<Reply>) -> Self {
        Self::create(responses, true).await
    }

    async fn create(mut responses: Vec<Reply>, openai: bool) -> Self {
        if openai {
            responses.insert(
                0,
                Reply(
                    StatusCode::OK,
                    json!({"object":"list", "data":[{"id":"gpt-test"}]}).to_string(),
                ),
            );
        }
        let state = Upstream {
            responses: Arc::new(Mutex::new(responses.into())),
            cost: Arc::new(Mutex::new(Some(0.0))),
            cost_checks: Arc::default(),
            seen: Arc::default(),
            before_reply: Arc::default(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut config = config(format!(
            "http://{}/sleev/opencode/anthropic",
            listener.local_addr().unwrap()
        ));
        let directory = tempfile::tempdir().unwrap();
        let reporting = directory.path().join("codexbar.json");
        std::fs::write(
            &reporting,
            r#"{"providers":[{"id":"openai","apiKey":"reporting-only-test"}]}"#,
        )
        .unwrap();
        std::fs::set_permissions(&reporting, std::fs::Permissions::from_mode(0o600)).unwrap();
        if openai {
            config.billing = None;
            config.renewal_anchor = date("2026-11-09T00:00:00Z");
            config.openai = Some(claude_credit_router::openai::Settings {
                api_upstream: config.upstream.replace("anthropic", "openai"),
                project_id: "proj_personal".into(),
                enforced_project_limit_usd: 190,
                grant_usd: 199,
                spending: claude_credit_router::spending::Settings {
                    limit_usd: 150,
                    codexbar_config: reporting,
                },
            });
            config.upstream = config.upstream.replace("anthropic", "codex");
        }
        let app = axum::Router::new()
            .fallback(upstream)
            .with_state(state.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let store = Store(directory.path().join("state.json"));
        let clock = Arc::new(Mutex::new(date("2026-11-10T12:00:00Z")));
        let mut router = Router::new(
            config.clone(),
            store.clone(),
            if openai {
                "sk-proj-mock"
            } else {
                "sk-ant-api-mock"
            },
        )
        .unwrap()
        .with_clock({
            let clock = clock.clone();
            move || *clock.lock().unwrap()
        });
        router.spending = router.spending.with_test_endpoint(&format!(
            "{}/costs",
            config.upstream.split("/sleev").next().unwrap()
        ));
        if openai {
            router.verify_organization().await.unwrap();
            let metadata = state.seen.lock().unwrap().pop().unwrap();
            assert_eq!(metadata.uri, "/sleev/opencode/openai/v1/models");
            assert_eq!(metadata.headers["openai-project"], "proj_personal");
            assert_eq!(metadata.headers["openai-organization"], "personal-org");
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = router.clone().app();
        let task2 = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            url,
            directory,
            store,
            state,
            config,
            router,
            clock,
            tasks: vec![task, task2],
        }
    }

    async fn send(&self, body: &str) -> reqwest::Response {
        reqwest::Client::new()
            .post(format!("{}/v1/messages?beta=true", self.url))
            .header("authorization", "Bearer native-subscription")
            .header(
                "anthropic-beta",
                "oauth-2025-04-20,interleaved-thinking-2025-05-14",
            )
            .header("x-native-header", "preserved")
            .body(body.to_owned())
            .send()
            .await
            .unwrap()
    }

    fn seen(&self) -> Vec<Seen> {
        self.state.seen.lock().unwrap().clone()
    }

    fn now(&self) -> DateTime<Utc> {
        *self.clock.lock().unwrap()
    }
}

#[tokio::test]
async fn available_promotional_credit_preserves_model_settings_and_history_bytes() {
    let fixture = Fixture::new(vec![Reply(StatusCode::OK, "data: model output\n\n".into())]).await;
    let body = r#"{ "model":"claude-opus-4-6", "thinking":{"type":"adaptive"}, "messages":[{"role":"assistant","content":[{"type":"thinking","signature":"signed-exactly"}]}] }"#;
    let response = fixture.send(body).await;
    assert_eq!(response.headers()["request-id"], "unchanged-id");
    assert_eq!(response.text().await.unwrap(), "data: model output\n\n");
    let seen = fixture.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].body, body);
    assert_eq!(seen[0].method, Method::POST);
    assert_eq!(
        seen[0].uri,
        "/sleev/opencode/anthropic/v1/messages?beta=true"
    );
    assert_eq!(seen[0].headers["x-api-key"], "sk-ant-api-mock");
    assert!(!seen[0].headers.contains_key("authorization"));
    assert_eq!(
        seen[0].headers["anthropic-beta"],
        "interleaved-thinking-2025-05-14"
    );
    assert_eq!(seen[0].headers["x-native-header"], "preserved");
}

#[tokio::test]
async fn transport_audit_preserves_bytes_strips_private_correlation_and_never_parses_native_usage()
{
    let payload = "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_api\",\"model\":\"claude-test\",\"usage\":{\"input_tokens\":3,\"output_tokens\":0}}}\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"PRIVATE_OUTPUT\"}}\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":9}}\n";
    let fixture = Fixture::new(vec![
        Reply(StatusCode::OK, payload.into()),
        Reply(StatusCode::OK, payload.into()),
    ])
    .await;
    let body = r#"{"model":"claude-test","messages":[{"content":"PRIVATE_PROMPT"}]}"#;
    let response = reqwest::Client::new()
        .post(format!("{}/v1/messages", fixture.url))
        .header("authorization", "Bearer native-subscription")
        .header("x-credit-session-id", "ses_exact")
        .header("x-credit-request-id", "request_exact")
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.text().await.unwrap(), payload);
    assert!(
        !fixture.seen()[0]
            .headers
            .contains_key("x-credit-session-id")
    );
    assert_eq!(fixture.seen()[0].body, body);
    fixture
        .store
        .update(|state| {
            state.set_mode(Mode::Subscription, fixture.now());
            Ok(())
        })
        .unwrap();
    assert_eq!(fixture.send(body).await.text().await.unwrap(), payload);
    let records = fixture.store.audit().records(None).unwrap();
    let api = records
        .iter()
        .find(|record| {
            record.event == "request_finished" && record.route == Some(Route::Promotional)
        })
        .unwrap();
    assert_eq!(api.context.session_id.as_deref(), Some("ses_exact"));
    assert_eq!(
        api.context.upstream_request_id.as_deref(),
        Some("unchanged-id")
    );
    assert_eq!(api.tokens.as_ref().unwrap().output, 9);
    let native = records
        .iter()
        .find(|record| {
            record.event == "request_finished" && record.route == Some(Route::Subscription)
        })
        .unwrap();
    assert!(native.tokens.is_none());
    let json = serde_json::to_string(&records).unwrap();
    for private in ["PRIVATE_OUTPUT", "PRIVATE_PROMPT", "native-subscription"] {
        assert!(!json.contains(private));
    }
}

#[tokio::test]
async fn explicit_exhaustion_falls_back_once_and_passes_subscription_errors_unchanged() {
    let native_error =
        r#"{"error":{"type":"rate_limit_error","message":"subscription limit reached"}}"#;
    let fixture = Fixture::new(vec![
        depleted(),
        Reply(StatusCode::TOO_MANY_REQUESTS, native_error.into()),
        Reply(
            StatusCode::SERVICE_UNAVAILABLE,
            "native retry policy owns this".into(),
        ),
    ])
    .await;
    let response = fixture.send("same-history").await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.text().await.unwrap(), native_error);
    assert_eq!(fixture.store.read().unwrap().active, Route::Subscription);
    let seen = fixture.seen();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].body, seen[1].body);
    assert_eq!(
        seen[1].headers["authorization"],
        "Bearer native-subscription"
    );
    assert_eq!(
        seen[1].headers["anthropic-beta"],
        "oauth-2025-04-20,interleaved-thinking-2025-05-14"
    );
    assert!(!seen[1].headers.contains_key("x-api-key"));
    let response = fixture.send("next request").await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        response.text().await.unwrap(),
        "native retry policy owns this"
    );
    assert_eq!(fixture.seen().len(), 3);
}

#[tokio::test]
async fn temporary_api_limits_authentication_and_spend_caps_do_not_switch() {
    let cases = [
        (StatusCode::TOO_MANY_REQUESTS, json!({"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}).to_string()),
        (StatusCode::TOO_MANY_REQUESTS, json!({"type":"error","error":{"type":"rate_limit_error","details":{"error_code":"enforced_spend_limit_reached"},"message":EXHAUSTED}}).to_string()),
        (StatusCode::BAD_REQUEST, json!({"type":"error","error":{"type":"invalid_request_error","message":"You have reached your specified API usage limits"}}).to_string()),
        (StatusCode::UNAUTHORIZED, "invalid key".into()),
        (StatusCode::PAYMENT_REQUIRED, "Credit balance too low · Add funds: https://platform.claude.com/settings/billing".into()),
    ];
    for (status, body) in cases {
        let fixture = Fixture::new(vec![Reply(status, body.clone())]).await;
        let response = fixture.send("request").await;
        assert_eq!(response.status(), status);
        assert_eq!(response.text().await.unwrap(), body);
        assert_eq!(fixture.seen().len(), 1);
        assert_eq!(fixture.store.read().unwrap().active, Route::Promotional);
    }
}

#[tokio::test]
async fn partial_sse_output_is_never_parsed_replayed_or_switched() {
    let body = format!(
        "event: content_block_start\ndata: {{\"type\":\"tool_use\",\"name\":\"bash\"}}\n\nevent: error\ndata: {{\"message\":\"{EXHAUSTED}\"}}\n\n"
    );
    let fixture = Fixture::new(vec![Reply(StatusCode::OK, body.clone())]).await;
    assert_eq!(fixture.send("request").await.text().await.unwrap(), body);
    assert_eq!(fixture.seen().len(), 1);
    assert_eq!(fixture.store.read().unwrap().active, Route::Promotional);
}

#[tokio::test]
async fn concurrent_exhaustion_does_not_repeat_api_rejections() {
    let fixture = Fixture::new(vec![
        depleted(),
        Reply(StatusCode::OK, "native one".into()),
        Reply(StatusCode::OK, "native two".into()),
    ])
    .await;
    let (first, second) = tokio::join!(fixture.send("one"), fixture.send("two"));
    assert!(first.status().is_success() && second.status().is_success());
    let seen = fixture.seen();
    assert_eq!(seen.len(), 3);
    assert_eq!(
        seen.iter()
            .filter(|item| item.headers.contains_key("x-api-key"))
            .count(),
        1
    );
}

#[tokio::test]
async fn renewal_probe_restores_promotional_route_without_an_extra_inference_probe() {
    let fixture = Fixture::new(vec![Reply(StatusCode::OK, "renewed output".into())]).await;
    fixture
        .store
        .update(|state| {
            state.exhausted(fixture.now(), fixture.config.renewal_anchor)?;
            state.next_probe_at = Some(fixture.now() - chrono::Duration::seconds(1));
            Ok(())
        })
        .unwrap();
    assert_eq!(
        fixture
            .send("real next request")
            .await
            .text()
            .await
            .unwrap(),
        "renewed output"
    );
    assert_eq!(fixture.seen().len(), 1);
    let state = fixture.store.read().unwrap();
    assert_eq!(state.active, Route::Promotional);
    assert!(state.next_probe_at.is_none());
    assert_eq!(
        state.history.last().unwrap().reason,
        "promotional_renewal_probe_succeeded"
    );
}

#[tokio::test]
async fn late_renewal_stays_on_subscription_between_bounded_probes() {
    let fixture = Fixture::new(vec![depleted(), Reply(StatusCode::OK, "native".into())]).await;
    fixture
        .store
        .update(|state| {
            state.exhausted(fixture.now(), fixture.config.renewal_anchor)?;
            state.next_probe_at = Some(fixture.now());
            Ok(())
        })
        .unwrap();
    fixture.send("request").await;
    let state = fixture.store.read().unwrap();
    assert_eq!(state.active, Route::Subscription);
    assert_eq!(state.route(fixture.now()), Route::Subscription);
    assert!(state.next_probe_at.unwrap() > fixture.now() + chrono::Duration::hours(5));
    assert_eq!(
        state.history.last().unwrap().reason,
        "renewal_grant_not_available_yet"
    );
}

#[tokio::test]
async fn token_counting_and_other_automatic_endpoints_remain_native() {
    let fixture = Fixture::new(vec![Reply(StatusCode::OK, "free token count".into())]).await;
    reqwest::Client::new()
        .post(format!("{}/v1/messages/count_tokens", fixture.url))
        .header("authorization", "Bearer native")
        .body("count this")
        .send()
        .await
        .unwrap();
    assert_eq!(fixture.seen()[0].headers["authorization"], "Bearer native");
    assert!(!fixture.seen()[0].headers.contains_key("x-api-key"));
}

#[tokio::test]
async fn large_unknown_billing_errors_are_forwarded_without_truncation_or_switch() {
    let body = "x".repeat(200_000);
    let fixture = Fixture::new(vec![Reply(StatusCode::PAYMENT_REQUIRED, body.clone())]).await;
    assert_eq!(fixture.send("request").await.text().await.unwrap(), body);
    assert_eq!(fixture.store.read().unwrap().active, Route::Promotional);
}

#[tokio::test]
async fn malformed_state_never_silently_reactivates_api() {
    let fixture = Fixture::new(vec![]).await;
    std::fs::write(fixture.directory.path().join("state.json"), "broken").unwrap();
    assert_eq!(
        fixture.send("request").await.status(),
        StatusCode::BAD_GATEWAY
    );
    assert!(fixture.seen().is_empty());
}

#[tokio::test]
async fn non_billable_metadata_verifies_expected_organization() {
    let fixture = Fixture::new(vec![
        Reply(StatusCode::OK, "{}".into()),
        Reply(StatusCode::OK, "{}".into()),
    ])
    .await;
    let router = Router::new(
        fixture.config.clone(),
        fixture.store.clone(),
        "sk-ant-api-test",
    )
    .unwrap();
    router.verify_organization().await.unwrap();
    let mut wrong = fixture.config.clone();
    wrong.organization_id = "company-org".into();
    let router = Router::new(wrong, fixture.store.clone(), "sk-ant-api-test").unwrap();
    assert!(router.verify_organization().await.is_err());
    assert!(
        fixture
            .seen()
            .iter()
            .all(|item| item.method == Method::GET && item.uri.ends_with("/v1/models?limit=1"))
    );
}

#[test]
fn exact_credit_error_only() {
    let bytes =
        json!({"type":"error", "error":{"type":"invalid_request_error", "message":EXHAUSTED}})
            .to_string();
    assert!(exhausted(StatusCode::BAD_REQUEST, bytes.as_bytes()));
    assert!(!exhausted(StatusCode::TOO_MANY_REQUESTS, bytes.as_bytes()));
    assert!(!exhausted(StatusCode::OK, bytes.as_bytes()));
    assert!(!exhausted(StatusCode::PAYMENT_REQUIRED, b"not json"));
}

#[test]
fn persists_restart_override_and_reason_history() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.json");
    let store = Store(path.clone());
    let now = date("2026-11-11T12:00:00Z");
    store
        .update(|state| {
            state.exhausted(now, date("2026-12-03T00:00:00Z"))?;
            Ok(())
        })
        .unwrap();
    let restarted = Store(path);
    assert_eq!(restarted.read().unwrap().route(now), Route::Subscription);
    restarted
        .update(|state| {
            state.set_mode(Mode::Subscription, now);
            state.recheck(now);
            Ok(())
        })
        .unwrap();
    assert_eq!(store.read().unwrap().route(now), Route::Promotional);
    store
        .update(|state| {
            state.set_mode(Mode::Subscription, now);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        store.read().unwrap().route(date("2027-01-01T00:00:00Z")),
        Route::Subscription
    );
    assert_eq!(store.read().unwrap().history.len(), 4);
    assert_eq!(
        std::fs::metadata(store.0).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn renewal_uses_subscription_anchor_and_preserves_month_end() {
    let anchor = date("2026-12-03T00:00:00Z");
    assert_eq!(
        next_renewal(anchor, date("2026-11-11T00:00:00Z")).unwrap(),
        anchor
    );
    assert_eq!(
        next_renewal(anchor, anchor).unwrap(),
        date("2027-01-03T00:00:00Z")
    );
    assert_eq!(
        next_renewal(date("2026-01-31T12:00:00Z"), date("2026-02-28T13:00:00Z")).unwrap(),
        date("2026-03-31T12:00:00Z")
    );
}

#[test]
fn depletion_at_the_renewal_boundary_does_not_skip_a_late_grant_for_a_month() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store(directory.path().join("state.json"));
    let renewal = date("2026-12-03T00:00:00Z");
    let now = date("2026-12-03T00:01:00Z");
    store
        .update(|state| {
            state.available(date("2026-12-02T23:59:00Z"));
            state.exhausted(now, renewal)?;
            assert_eq!(
                state.next_probe_at.unwrap(),
                now + chrono::Duration::hours(6)
            );
            Ok(())
        })
        .unwrap();
}

#[tokio::test]
async fn browser_and_unauthenticated_requests_cannot_spend_promotional_credits() {
    let fixture = Fixture::new(vec![]).await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/v1/messages", fixture.url))
        .body("request")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = client
        .post(format!("{}/v1/messages", fixture.url))
        .header("origin", "https://untrusted.example")
        .header("authorization", "Bearer example")
        .body("request")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(fixture.seen().is_empty());
}

#[test]
fn rejects_all_paid_billing_modes_and_non_loopback_endpoints() {
    for index in 0..6 {
        let mut value = config("http://127.0.0.1:17321/sleev/opencode/anthropic".into());
        match index {
            0 => value.billing.as_mut().unwrap().no_payment_method = false,
            1 => value.billing.as_mut().unwrap().purchased_balance_usd = 1,
            2 => value.billing.as_mut().unwrap().auto_reload = true,
            3 => value.billing.as_mut().unwrap().sales_invoicing = true,
            4 => value.listen = "0.0.0.0:17322".parse().unwrap(),
            _ => value.upstream = "https://api.anthropic.com".into(),
        }
        assert!(value.validate().is_err());
    }
}

#[test]
fn key_must_be_private_and_never_subscription_oauth() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("key");
    std::fs::write(&path, "sk-ant-api-test\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        private_key(&path, claude_credit_router::policy::validate_key).unwrap(),
        "sk-ant-api-test"
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(private_key(&path, claude_credit_router::policy::validate_key).is_err());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::write(&path, "oauth-credential").unwrap();
    assert!(private_key(&path, claude_credit_router::policy::validate_key).is_err());
}

#[test]
fn openai_private_file_and_environment_use_the_same_project_key_validation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("key");
    std::fs::write(&path, "sk-proj-test\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        private_key(&path, claude_credit_router::openai::validate_key).unwrap(),
        "sk-proj-test"
    );
    for invalid in [
        "sk-admin-test",
        "sk-ant-api-test",
        "subscription-token",
        "sk-proj-two words",
    ] {
        std::fs::write(&path, invalid).unwrap();
        assert!(private_key(&path, claude_credit_router::openai::validate_key).is_err());
        assert!(claude_credit_router::openai::validate_key(invalid).is_err());
    }
}

async fn openai_send(fixture: &Fixture, path: &str, body: &str) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{}{path}", fixture.url))
        .header("authorization", "Bearer native-codex")
        .header("chatgpt-account-id", "native-account")
        .header("x-native-header", "keep")
        .body(body.to_owned())
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn openai_responses_and_compaction_preserve_exact_body_and_settings() {
    for path in ["/responses?beta=true", "/v1/responses/compact"] {
        let fixture = Fixture::openai(vec![Reply(StatusCode::OK, "data: output\n\n".into())]).await;
        let body = r#"{ "model":"gpt-test", "reasoning":{"effort":"high"}, "input":[{"type":"reasoning","encrypted_content":"signed-history"}], "tools":[{"type":"function","name":"read"}] }"#;
        assert_eq!(
            openai_send(&fixture, path, body)
                .await
                .text()
                .await
                .unwrap(),
            "data: output\n\n"
        );
        let seen = fixture.seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].body, body);
        assert_eq!(seen[0].headers["authorization"], "Bearer sk-proj-mock");
        assert_eq!(seen[0].headers["openai-project"], "proj_personal");
        assert!(!seen[0].headers.contains_key("chatgpt-account-id"));
        assert_eq!(seen[0].headers["x-native-header"], "keep");
        assert_eq!(
            seen[0].uri,
            format!(
                "/sleev/opencode/openai{}",
                claude_credit_router::openai::api_path(path)
            )
        );
    }
}

#[tokio::test]
async fn upload_crossing_calendar_reset_uses_native_auth_and_exact_body() {
    let fixture = Fixture::openai(vec![Reply(StatusCode::OK, "native".into())]).await;
    *fixture.clock.lock().unwrap() = date("2026-12-31T23:59:59Z");
    let clock = fixture.clock.clone();
    let bytes = Bytes::from_static(br#"{"model":"gpt-test","input":"unchanged"}"#);
    let expected = bytes.clone();
    let request = Request::builder()
        .method(Method::POST)
        .uri("/responses")
        .header("authorization", "Bearer native-codex")
        .body(Body::from_stream(futures_util::stream::once(async move {
            *clock.lock().unwrap() = date("2027-01-01T00:00:00Z");
            Ok::<_, std::io::Error>(bytes)
        })))
        .unwrap();
    let response = fixture.router.clone().app().oneshot(request).await.unwrap();
    assert_eq!(
        to_bytes(response.into_body(), 1024).await.unwrap(),
        "native"
    );
    let seen = fixture.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].headers["authorization"], "Bearer native-codex");
    assert_eq!(seen[0].body, expected);
    assert_eq!(
        fixture.store.read().unwrap().history.last().unwrap().reason,
        "calendar_reset_guard"
    );
}

#[tokio::test]
async fn exhaustion_persistence_failure_does_not_block_safe_native_fallback() {
    let fixture = Fixture::new(vec![depleted(), Reply(StatusCode::OK, "native".into())]).await;
    let lock = fixture.directory.path().join("state.lock");
    *fixture.state.before_reply.lock().unwrap() = Some(Box::new(move || {
        // State reads succeed before the API rejection; persisting it fails.
        std::fs::remove_file(&lock).unwrap();
        std::fs::create_dir(&lock).unwrap();
    }));
    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/messages")
        .header("authorization", "Bearer native-subscription")
        .body(Body::from("same-history"))
        .unwrap();
    let response = fixture.router.clone().app().oneshot(request).await.unwrap();
    assert_eq!(
        to_bytes(response.into_body(), 1024).await.unwrap(),
        "native"
    );
    let seen = fixture.seen();
    assert_eq!(seen.len(), 2);
    assert!(seen[0].headers.contains_key("x-api-key"));
    assert_eq!(
        seen[1].headers["authorization"],
        "Bearer native-subscription"
    );
    assert_eq!(seen[0].body, seen[1].body);
}

#[tokio::test]
async fn openai_billing_codes_switch_once_then_leave_native_failures_alone() {
    for code in [
        "project_spend_limit_exceeded",
        "organization_spend_limit_exceeded",
        "credit_balance_exhausted",
        "organization_usage_limit_exceeded",
    ] {
        let error =
            json!({"error":{"code":code, "message":"billing allowance exhausted"}}).to_string();
        let native = r#"{"error":{"code":"subscription_sharing_usage_limit_exceeded"}}"#;
        let fixture = Fixture::openai(vec![
            Reply(StatusCode::TOO_MANY_REQUESTS, error),
            Reply(StatusCode::TOO_MANY_REQUESTS, native.into()),
            Reply(StatusCode::SERVICE_UNAVAILABLE, "native failure".into()),
        ])
        .await;
        let body = r#"{"model":"gpt-test","input":"same history"}"#;
        let response = openai_send(&fixture, "/responses", body).await;
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.text().await.unwrap(), native);
        let response = openai_send(&fixture, "/responses", body).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.text().await.unwrap(), "native failure");
        let seen = fixture.seen();
        assert_eq!(seen.len(), 3);
        assert_eq!(seen[0].headers["authorization"], "Bearer sk-proj-mock");
        for request in &seen[1..] {
            assert_eq!(request.uri, "/sleev/opencode/codex/responses");
            assert_eq!(request.headers["authorization"], "Bearer native-codex");
            assert_eq!(request.headers["chatgpt-account-id"], "native-account");
            assert_eq!(request.body, body);
            assert!(!request.headers.contains_key("openai-project"));
        }
        assert_eq!(fixture.store.read().unwrap().active, Route::Subscription);
    }
}

#[tokio::test]
async fn openai_rate_limits_and_partial_output_never_trigger_fallback() {
    for (status, body) in [
        (
            StatusCode::TOO_MANY_REQUESTS,
            r#"{"error":{"code":"rate_limit_exceeded"}}"#,
        ),
        (
            StatusCode::TOO_MANY_REQUESTS,
            r#"{"error":{"code":"insufficient_quota"}}"#,
        ),
        (StatusCode::UNAUTHORIZED, "invalid key"),
        (
            StatusCode::OK,
            "data: tool call\n\ndata: {\"error\":{\"code\":\"credit_balance_exhausted\"}}\n\n",
        ),
    ] {
        let fixture = Fixture::openai(vec![Reply(status, body.into())]).await;
        assert_eq!(
            openai_send(&fixture, "/responses", r#"{"model":"gpt-test"}"#)
                .await
                .text()
                .await
                .unwrap(),
            body
        );
        assert_eq!(fixture.seen().len(), 1);
        assert_eq!(fixture.store.read().unwrap().active, Route::Promotional);
    }
}

#[tokio::test]
async fn codex_only_models_stay_native_without_model_or_context_remapping() {
    let fixture = Fixture::openai(vec![Reply(StatusCode::OK, "native".into())]).await;
    let body = r#"{"model":"codex-only-test","input":"unaltered"}"#;
    assert_eq!(
        openai_send(&fixture, "/responses", body)
            .await
            .text()
            .await
            .unwrap(),
        "native"
    );
    let seen = fixture.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].headers["authorization"], "Bearer native-codex");
    assert_eq!(seen[0].body, body);
}

#[tokio::test]
async fn calendar_guard_cannot_be_overridden_by_manual_recheck_or_restart() {
    let fixture = Fixture::openai(vec![]).await;
    let router = Router::new(
        fixture.config.clone(),
        fixture.store.clone(),
        "sk-proj-mock",
    )
    .unwrap();
    let first = date("2026-12-01T00:00:00Z");
    assert_eq!(router.status(first).unwrap().route, Route::Subscription);
    fixture
        .store
        .update(|state| {
            state.recheck(first);
            Ok(())
        })
        .unwrap();
    assert_eq!(router.status(first).unwrap().route, Route::Subscription);
    assert_eq!(
        Store(fixture.directory.path().join("state.json"))
            .read()
            .unwrap()
            .next_probe_at,
        Some(date("2026-12-09T00:00:00Z"))
    );
    assert_eq!(
        router.status(date("2026-12-09T00:00:00Z")).unwrap().route,
        Route::Promotional
    );
    assert_eq!(
        fixture.store.read().unwrap().history.last().unwrap().reason,
        "calendar_reset_guard"
    );
    assert!(fixture.seen().is_empty());
}

#[test]
fn every_grant_cycle_uses_at_most_one_calendar_month_allowance() {
    use chrono::Datelike;
    let anchor = date("2026-11-09T00:00:00Z");
    for offset in 0..24 {
        let start = anchor + chrono::Months::new(offset);
        let end = anchor + chrono::Months::new(offset + 1);
        let mut now = start;
        let mut months = std::collections::HashSet::new();
        while now < end {
            if claude_credit_router::openai::window_open(anchor, now) {
                months.insert((now.year(), now.month()));
            }
            now += chrono::Duration::hours(12);
        }
        assert_eq!(months.len(), 1);
    }
}

#[tokio::test]
async fn local_limit_switches_before_inference_and_cannot_be_rechecked_away() {
    let f = Fixture::openai(vec![
        Reply(StatusCode::OK, "native".into()),
        Reply(StatusCode::TOO_MANY_REQUESTS, "native limit".into()),
    ])
    .await;
    *f.state.cost.lock().unwrap() = Some(150.0);
    let body = r#"{"model":"gpt-test","input":"unchanged"}"#;
    assert_eq!(
        openai_send(&f, "/responses/compact", body)
            .await
            .text()
            .await
            .unwrap(),
        "native"
    );
    assert_eq!(
        f.store.read().unwrap().history.last().unwrap().reason,
        "local_spend_limit"
    );
    f.store
        .update(|s| {
            s.recheck(f.now());
            Ok(())
        })
        .unwrap();
    // Lower/stale reporting cannot erase the persisted high-water mark.
    *f.state.cost.lock().unwrap() = Some(0.0);
    assert_eq!(
        openai_send(&f, "/responses", body).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(*f.state.cost_checks.lock().unwrap(), 1);
    assert!(
        f.seen()
            .iter()
            .all(|r| r.headers["authorization"] == "Bearer native-codex" && r.body == body)
    );
    assert_eq!(
        Store(f.store.0.clone())
            .read()
            .unwrap()
            .spending
            .unwrap()
            .reported_usd,
        150.0
    );
}

#[tokio::test]
async fn costs_are_refreshed_for_each_admission_and_shared_project_spend_counts() {
    let f = Fixture::openai(vec![
        Reply(StatusCode::OK, "api".into()),
        Reply(StatusCode::OK, "native".into()),
    ])
    .await;
    *f.state.cost.lock().unwrap() = Some(149.0);
    openai_send(&f, "/responses", r#"{"model":"gpt-test"}"#).await;
    // Another machine spends the dollar, without any local usage event.
    *f.state.cost.lock().unwrap() = Some(150.0);
    openai_send(&f, "/responses", r#"{"model":"gpt-test"}"#).await;
    assert_eq!(*f.state.cost_checks.lock().unwrap(), 2);
    assert_eq!(f.seen()[0].headers["authorization"], "Bearer sk-proj-mock");
    assert_eq!(f.seen()[1].headers["authorization"], "Bearer native-codex");
}

#[tokio::test]
async fn unavailable_reporting_uses_subscription_and_native_does_not_recheck() {
    let f = Fixture::openai(vec![
        Reply(StatusCode::OK, "native".into()),
        Reply(StatusCode::SERVICE_UNAVAILABLE, "native failure".into()),
    ])
    .await;
    *f.state.cost.lock().unwrap() = None;
    openai_send(&f, "/responses", r#"{"model":"gpt-test"}"#).await;
    let result = openai_send(&f, "/responses", r#"{"model":"gpt-test"}"#).await;
    assert_eq!(result.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(*f.state.cost_checks.lock().unwrap(), 1);
    assert!(
        f.seen()
            .iter()
            .all(|r| r.headers["authorization"] == "Bearer native-codex")
    );
    assert_eq!(
        f.store.read().unwrap().history.last().unwrap().reason,
        "spend_check_unavailable"
    );
}

#[tokio::test]
async fn new_grant_cycle_rechecks_costs_before_returning_to_api() {
    let f = Fixture::openai(vec![
        Reply(StatusCode::OK, "native".into()),
        Reply(StatusCode::OK, "renewed".into()),
    ])
    .await;
    *f.state.cost.lock().unwrap() = Some(224.65);
    openai_send(&f, "/responses", r#"{"model":"gpt-test"}"#).await;
    *f.clock.lock().unwrap() = date("2026-12-01T00:00:01Z");
    assert_eq!(f.router.status(f.now()).unwrap().route, Route::Subscription);
    *f.clock.lock().unwrap() = date("2026-12-09T00:00:01Z");
    *f.state.cost.lock().unwrap() = Some(0.0);
    openai_send(&f, "/responses", r#"{"model":"gpt-test"}"#).await;
    assert_eq!(f.seen()[1].headers["authorization"], "Bearer sk-proj-mock");
    assert_eq!(f.store.read().unwrap().spending.unwrap().month, "2026-12");
}

#[tokio::test]
async fn concurrent_requests_observing_exhaustion_all_use_native() {
    let f = Fixture::openai(
        (0..8)
            .map(|_| Reply(StatusCode::OK, "native".into()))
            .collect(),
    )
    .await;
    *f.state.cost.lock().unwrap() = Some(150.0);
    futures_util::future::join_all(
        (0..8).map(|_| openai_send(&f, "/responses", r#"{"model":"gpt-test"}"#)),
    )
    .await;
    assert_eq!(*f.state.cost_checks.lock().unwrap(), 1);
    assert_eq!(f.seen().len(), 8);
    assert!(
        f.seen()
            .iter()
            .all(|r| r.headers["authorization"] == "Bearer native-codex")
    );
}

#[test]
fn costs_parser_rejects_untrustworthy_or_incomplete_data() {
    use claude_credit_router::spending::parse;
    let valid = json!({"object":"page","has_more":false,"data":[{"start_time":1,"end_time":2,
        "results":[{"object":"organization.costs.result","project_id":"proj_personal","amount":{"currency":"usd","value":12.34}}]}]});
    assert_eq!(
        parse(valid.to_string().as_bytes(), "proj_personal", 1, 2).unwrap(),
        12.34
    );
    for (path, value) in [
        ("/has_more", json!(true)),
        ("/data", json!([])),
        ("/data/0/start_time", json!(0)),
        ("/data/0/end_time", json!(1)),
        ("/data/0/results/0/project_id", json!("company")),
        ("/data/0/results/0/amount/currency", json!("eur")),
        ("/data/0/results/0/amount/value", json!(null)),
    ] {
        let mut invalid = valid.clone();
        *invalid.pointer_mut(path).unwrap() = value;
        assert!(
            parse(invalid.to_string().as_bytes(), "proj_personal", 1, 2).is_err(),
            "{path}"
        );
    }
}
