use claude_credit_router::audit::{
    Audit, ContextIds, Rates, Record, Tier, Tokens, report, thread_bindings,
};
use claude_credit_router::policy::{Mode, Route, Store};
use claude_credit_router::trace::Metadata;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn policy_outbox_recovers_after_failed_export_and_restart_without_losing_switches() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state.json");
    let spool = temp.path().join("audit-spool");
    fs::write(&spool, b"blocked").unwrap();
    Store(path.clone())
        .update(|state| {
            state.set_mode(Mode::Subscription, chrono::Utc::now());
            Ok(())
        })
        .unwrap();
    let pending: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert!(pending["pending_audit"].as_array().unwrap().len() >= 2);
    fs::remove_file(spool).unwrap();
    let restarted = Store(path);
    assert_eq!(restarted.read().unwrap().mode, Mode::Subscription);
    restarted.read().unwrap();
    let records = restarted.audit().records(None).unwrap();
    assert_eq!(
        records
            .iter()
            .filter(|record| record.reason.as_deref() == Some("manual_subscription"))
            .count(),
        1
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| record.event == "audit_started")
            .count(),
        1
    );
}

#[test]
fn anthropic_priority_usage_never_receives_a_standard_cost_estimate() {
    let mut parser = Metadata::sse();
    let mut record = Record::new("request_finished", "router");
    parser.push(b"data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":3,\"service_tier\":\"priority\"}}}\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":9}}\n",&mut record);
    assert_eq!(record.tokens.as_ref().unwrap().output, 9);
    assert_eq!(
        record.cost_unavailable_reason.as_deref(),
        Some("nonstandard_service_tier_price_not_captured")
    );
}

#[test]
fn audit_is_private_durable_deduplicated_and_not_limited_to_state_history() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store(temp.path().join("state.json"));
    for index in 0..80 {
        store
            .update(|state| {
                state.set_mode(
                    Mode::Auto,
                    chrono::Utc::now() + chrono::Duration::seconds(index),
                );
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(store.read().unwrap().history.len(), 32);
    let records = store.audit().records(None).unwrap();
    assert_eq!(
        records
            .iter()
            .filter(|record| record.event == "policy")
            .count(),
        80
    );
    assert_eq!(
        fs::metadata(store.audit().0).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let audit = Audit::beside(&store.0);
    audit.append(&records[1]).unwrap();
    assert_eq!(audit.records(None).unwrap().len(), records.len());
}

#[test]
fn partial_spools_recover_and_unknown_payloads_are_rejected_without_advancing() {
    let temp = tempfile::tempdir().unwrap();
    let audit = Audit(temp.path().join("audit.sqlite3"));
    let directory = temp.path().join("audit-spool");
    fs::create_dir(&directory).unwrap();
    let path = directory.join("opencode-test.jsonl");
    let record = Record::new("opencode_usage", "opencode");
    let bytes = serde_json::to_vec(&record).unwrap();
    fs::write(&path, &bytes[..bytes.len() / 2]).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(audit.records(None).unwrap().is_empty());
    let mut line = bytes.clone();
    line.push(b'\n');
    fs::write(&path, &line).unwrap();
    assert_eq!(audit.records(None).unwrap().len(), 1);
    assert_eq!(audit.records(None).unwrap().len(), 1);
    let mut invalid = serde_json::to_value(&record).unwrap();
    invalid["prompt"] = "PRIVATE_PROMPT".into();
    line.extend(serde_json::to_vec(&invalid).unwrap());
    line.push(b'\n');
    fs::write(&path, &line).unwrap();
    assert!(audit.records(None).is_err());
    fs::write(&path, []).unwrap();
    assert!(audit.records(None).is_err());
    assert!(
        !fs::read_to_string(&audit.0)
            .unwrap_or_default()
            .contains("PRIVATE_PROMPT")
    );
}

#[test]
fn fragmented_sse_collects_only_usage_and_never_confuses_reasoning_or_cache() {
    let mut record = Record::new("request_finished", "router");
    let mut metadata = Metadata::sse();
    let payload=b"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_test\",\"model\":\"gpt-test\",\"output\":\"PRIVATE_OUTPUT\",\"usage\":{\"input_tokens\":20,\"input_tokens_details\":{\"cached_tokens\":5},\"output_tokens\":12,\"output_tokens_details\":{\"reasoning_tokens\":4}}}}\r\n\n";
    for byte in payload {
        metadata.push(&[*byte], &mut record);
    }
    metadata.end(&mut record);
    assert_eq!(record.tokens.as_ref().unwrap().input, 15);
    assert_eq!(record.tokens.as_ref().unwrap().cache_read, 5);
    assert_eq!(record.tokens.as_ref().unwrap().output, 8);
    assert_eq!(record.tokens.as_ref().unwrap().reasoning, 4);
    assert!(
        !serde_json::to_string(&record)
            .unwrap()
            .contains("PRIVATE_OUTPUT")
    );
    let mut claude = Record::new("request_finished", "router");
    let mut metadata = Metadata::sse();
    metadata.push(b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_test\",\"usage\":{\"input_tokens\":3,\"output_tokens\":0,\"cache_read_input_tokens\":7,\"cache_creation_input_tokens\":11}}}\n",&mut claude);
    metadata.push(
        b"data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":9}}\n",
        &mut claude,
    );
    let tokens = claude.tokens.unwrap();
    assert_eq!(
        (
            tokens.input,
            tokens.output,
            tokens.cache_read,
            tokens.cache_write
        ),
        (3, 9, 7, 11)
    );
}

#[test]
fn oversized_stream_events_do_not_block_later_metadata_and_missing_usage_is_not_zero() {
    let mut record = Record::new("request_finished", "router");
    let mut metadata = Metadata::sse();
    metadata.push(&vec![b'x'; 100_000], &mut record);
    metadata.push(b"\n", &mut record);
    assert!(record.tokens.is_none());
    metadata.push(
        b"data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":0}}\n",
        &mut record,
    );
    assert!(
        record.tokens.is_none(),
        "a missing input report is not zero usage"
    );
    metadata.push(b"data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":0,\"output_tokens\":0}}}\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":0}}\n",&mut record);
    assert_eq!(record.tokens.unwrap().output, 0);
}

#[test]
fn exact_native_bindings_and_parent_sessions_join_without_matching_titles_or_time() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("t3.sqlite3");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE orchestration_v2_projection_provider_threads(thread_id TEXT,payload_json TEXT);").unwrap();
    db.execute("INSERT INTO orchestration_v2_projection_provider_threads VALUES('thread-one',?1)",[r#"{"nativeThreadRef":{"driver":"opencode","nativeId":"ses_parent"},"text":"PRIVATE_TEXT"}"#]).unwrap();
    drop(db);
    let bindings = thread_bindings(&path).unwrap();
    assert_eq!(bindings["ses_parent"], vec!["thread-one"]);
    let mut metadata = Record::new("opencode_usage", "opencode");
    metadata.context = ContextIds {
        session_id: Some("ses_child".into()),
        parent_session_id: Some("ses_parent".into()),
        native_session_id: Some("native_uuid".into()),
        ..Default::default()
    };
    let mut request = Record::new("request_finished", "router");
    request.context.native_session_id = Some("native_uuid".into());
    request.route = Some(Route::Promotional);
    let output = report(&[metadata, request], &bindings);
    assert!(
        output["groups"]
            .as_array()
            .unwrap()
            .iter()
            .all(|group| group["t3_thread_ids"][0] == "thread-one")
    );
    assert!(!output.to_string().contains("PRIVATE_TEXT"));
    assert!(thread_bindings(&temp.path().join("absent")).is_err());
    assert!(!temp.path().join("absent").exists());
}

#[test]
fn estimates_are_api_only_historical_and_explicitly_unknown_without_rates() {
    let now = chrono::Utc::now();
    let rates = Rates {
        input: 1.0,
        output: 2.0,
        cache_read: Some(0.1),
        cache_write: Some(1.5),
        fetched_at: now - chrono::Duration::hours(1),
        tiers: vec![Tier {
            above: 100,
            input: 2.0,
            output: 4.0,
            cache_read: Some(0.2),
            cache_write: None,
        }],
    };
    let tokens = Tokens {
        input: 10,
        output: 20,
        reasoning: 5,
        cache_read: 30,
        cache_write: 0,
    };
    assert!((rates.estimate(&tokens).unwrap() - 0.000063).abs() < 1e-10);
    assert!(
        rates
            .estimate(&Tokens {
                input: 200,
                cache_write: 3,
                ..Default::default()
            })
            .is_none()
    );
    let mut api = Record::new("request_finished", "router");
    api.route = Some(Route::Promotional);
    api.context.model = Some("gpt-test".into());
    api.tokens = Some(tokens);
    api.rates = Some(rates);
    let mut native = api.clone();
    native.source = "opencode".into();
    native.event = "opencode_usage".into();
    native.route = None;
    native.opencode_cost_usd = Some(0.0);
    let output = report(&[api, native], &HashMap::new());
    let groups = output["groups"].as_array().unwrap();
    assert_eq!(
        groups
            .iter()
            .filter(|group| group["estimated_api_usd"].is_number())
            .count(),
        1
    );
    assert_eq!(
        groups
            .iter()
            .find(|group| group["source"] == "opencode")
            .unwrap()["opencode_reported_usd"],
        0.0
    );
}

#[test]
fn reports_keep_older_bindings_prices_and_explicit_coverage_when_filtering() {
    use claude_credit_router::audit::{correlate, report_filtered};
    let now = chrono::Utc::now();
    let mut binding = Record::new("opencode_binding", "opencode");
    binding.at = now - chrono::Duration::days(2);
    binding.context.session_id = Some("ses_one".into());
    binding.context.native_session_id = Some("native_one".into());
    let mut api = Record::new("request_finished", "router");
    api.at = now;
    api.context.native_session_id = Some("native_one".into());
    api.route = Some(Route::Promotional);
    let mut start = Record::new("audit_started", "policy");
    start.at = binding.at;
    let mut records = vec![start, binding.clone(), api];
    correlate(&mut records);
    let report = report_filtered(
        &records,
        &HashMap::new(),
        Some(now - chrono::Duration::hours(1)),
        Some("ses_one"),
    );
    assert_eq!(report["groups"][0]["requests"], 1);
    assert_eq!(report["groups"][0]["api_cost_missing"], 1);
    assert_eq!(
        report["coverage"]["history_before_audit_is_incomplete"],
        true
    );
    assert!(!report["coverage"]["started_at"].is_null());
    binding.context.session_id = Some("ses_ambiguous".into());
    records.push(binding);
    // Start from the original uncorrelated request; ambiguous UUIDs never guess.
    records[2].context.session_id = None;
    correlate(&mut records);
    assert!(records[2].context.session_id.is_none());
}

#[test]
fn ingestion_lock_never_delays_router_audit_or_changes_manual_route_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store(temp.path().join("state.json"));
    store.read().unwrap();
    store.audit().records(None).unwrap();
    let db = rusqlite::Connection::open(&store.audit().0).unwrap();
    db.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let start = std::time::Instant::now();
    store
        .update(|state| {
            state.set_mode(Mode::Subscription, chrono::Utc::now());
            Ok(())
        })
        .unwrap();
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
    db.execute_batch("ROLLBACK").unwrap();
    let records = store.audit().records(None).unwrap();
    let manual = records
        .iter()
        .find(|record| record.reason.as_deref() == Some("manual_subscription"))
        .unwrap();
    assert_eq!(manual.route, Some(Route::Subscription));
}

#[test]
fn cache_ttls_and_month_end_boundaries_do_not_invent_costs_or_cycle_dates() {
    let mut record = Record::new("request_finished", "router");
    let mut metadata = Metadata::sse();
    metadata.push(b"data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":3,\"output_tokens\":0,\"cache_creation_input_tokens\":5,\"cache_creation\":{\"ephemeral_1h_input_tokens\":5}}}}\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":9}}\n",&mut record);
    assert_eq!(record.tokens.unwrap().cache_write, 5);
    assert_eq!(
        record.cost_unavailable_reason.as_deref(),
        Some("one_hour_cache_write_price_not_captured")
    );
    let date = |text: &str| text.parse::<chrono::DateTime<chrono::Utc>>().unwrap();
    let anchor = date("2026-01-31T00:00:00Z");
    assert_eq!(
        claude_credit_router::policy::cycle_start(anchor, date("2026-02-15T00:00:00Z")).unwrap(),
        anchor
    );
    assert_eq!(
        claude_credit_router::policy::cycle_start(anchor, date("2026-03-05T00:00:00Z")).unwrap(),
        date("2026-02-28T00:00:00Z")
    );
}
