//! A local admission check against project-wide reported USD costs. Reports are
//! delayed: this is not a token reservation ledger or a zero-overshoot guarantee.
use crate::policy::{Store, next_renewal};
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub limit_usd: u32,
    pub codexbar_config: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub month: String,
    pub reported_usd: f64,
    pub checked_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct Guard {
    client: reqwest::Client,
    endpoint: String,
    lock: std::sync::Arc<tokio::sync::Mutex<()>>,
}

impl Guard {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(10))
                .build()?,
            endpoint: "https://api.openai.com/v1/organization/costs".into(),
            lock: Default::default(),
        })
    }

    /// Local mock servers only; production always uses the official endpoint.
    pub fn with_test_endpoint(mut self, endpoint: &str) -> Self {
        let url = reqwest::Url::parse(endpoint).expect("test URL");
        assert_eq!(url.scheme(), "http");
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        self.endpoint = endpoint.into();
        self
    }

    pub async fn check(
        &self,
        settings: &crate::openai::Settings,
        org: &str,
        store: &Store,
        anchor: DateTime<Utc>,
        now: DateTime<Utc>,
        fresh: bool,
    ) -> Result<bool> {
        let _lock = self.lock.lock().await;
        let month = now.format("%Y-%m").to_string();
        let previous = store.read()?.spending.filter(|s| s.month == month);
        if let Some(snapshot) = &previous {
            // Even manual recheck cannot forget a known exhausted month.
            if snapshot.reported_usd >= f64::from(settings.spending.limit_usd) {
                return self.stop(store, now, next_renewal(anchor, now)?, "local_spend_limit");
            }
            if !fresh
                && snapshot.checked_at <= now
                && now - snapshot.checked_at < chrono::Duration::seconds(60)
            {
                return Ok(true);
            }
        }
        match self.fetch(settings, org, now).await {
            Ok(reported) => {
                let total = reported.max(previous.map_or(0.0, |s| s.reported_usd));
                let total = store.update(|state| {
                    let total = total.max(
                        state
                            .spending
                            .as_ref()
                            .filter(|s| s.month == month)
                            .map_or(0.0, |s| s.reported_usd),
                    );
                    state.spending = Some(Snapshot {
                        month,
                        reported_usd: total,
                        checked_at: now,
                    });
                    Ok(total)
                })?;
                if total >= f64::from(settings.spending.limit_usd) {
                    self.stop(store, now, next_renewal(anchor, now)?, "local_spend_limit")
                } else {
                    Ok(true)
                }
            }
            Err(error) => {
                // Never log HTTP bodies, credentials, or request objects.
                eprintln!(
                    "credit-router: project cost check unavailable ({error}); using subscription"
                );
                self.stop(
                    store,
                    now,
                    now + chrono::Duration::minutes(1),
                    "spend_check_unavailable",
                )
            }
        }
    }

    fn stop(
        &self,
        store: &Store,
        now: DateTime<Utc>,
        until: DateTime<Utc>,
        reason: &str,
    ) -> Result<bool> {
        store.update(|state| {
            state.defer(now, until, reason);
            Ok(())
        })?;
        Ok(false)
    }

    async fn fetch(
        &self,
        settings: &crate::openai::Settings,
        org: &str,
        now: DateTime<Utc>,
    ) -> Result<f64> {
        use std::os::unix::fs::PermissionsExt;
        let path = &settings.spending.codexbar_config;
        let metadata = std::fs::metadata(path).context("reporting credential unavailable")?;
        ensure!(
            metadata.is_file() && metadata.permissions().mode() & 0o077 == 0,
            "CodexBar reporting credentials must be a private regular file"
        );
        let config: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)
            .context("invalid reporting credential file")?;
        let providers = config["providers"]
            .as_array()
            .context("missing reporting provider")?;
        let keys: Vec<_> = providers.iter().filter(|p| p["id"] == "openai").collect();
        ensure!(keys.len() == 1, "expected one OpenAI reporting provider");
        let key = keys[0]["apiKey"]
            .as_str()
            .filter(|k| !k.is_empty())
            .context("missing reporting key")?;
        let start = now
            .date_naive()
            .with_day(1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp();
        // Costs buckets are whole UTC days. An intraday end_time omits today's
        // bucket on the live API, so explicitly include the current day.
        let end = now
            .date_naive()
            .succ_opt()
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp();
        let response = self
            .client
            .get(&self.endpoint)
            .bearer_auth(key)
            .header("openai-organization", org)
            .query(&[
                ("start_time", start.to_string()),
                ("end_time", end.to_string()),
                ("bucket_width", "1d".into()),
                ("limit", "31".into()),
                ("project_ids", settings.project_id.clone()),
                ("group_by", "project_id".into()),
            ])
            .send()
            .await
            .context("costs transport failed")?;
        ensure!(
            response.status().is_success(),
            "costs HTTP {}",
            response.status()
        );
        // Bounded financial metadata, never inference or prompt contents.
        let mut response = response;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.context("costs body failed")? {
            ensure!(
                bytes.len() + chunk.len() <= 1_048_576,
                "oversized costs report"
            );
            bytes.extend_from_slice(&chunk);
        }
        parse(&bytes, &settings.project_id, start, end)
    }
}

pub fn parse(bytes: &[u8], project: &str, start: i64, end: i64) -> Result<f64> {
    let value: serde_json::Value = serde_json::from_slice(bytes).context("invalid costs JSON")?;
    ensure!(
        value["object"] == "page" && value["has_more"] == false,
        "incomplete or unexpected costs report"
    );
    let buckets = value["data"].as_array().context("missing cost buckets")?;
    ensure!(!buckets.is_empty(), "empty cost coverage");
    let mut cursor = start;
    let mut total = 0.0;
    for bucket in buckets {
        let from = bucket["start_time"]
            .as_i64()
            .context("missing bucket start")?;
        let to = bucket["end_time"].as_i64().context("missing bucket end")?;
        ensure!(from == cursor && to > from, "cost coverage gap or overlap");
        cursor = to;
        for result in bucket["results"]
            .as_array()
            .context("missing cost results")?
        {
            ensure!(
                result["object"] == "organization.costs.result"
                    && result["project_id"] == project
                    && result["amount"]["currency"] == "usd",
                "unexpected project/currency/cost result"
            );
            let amount = result["amount"]["value"]
                .as_f64()
                .context("invalid USD amount")?;
            ensure!(amount.is_finite(), "nonfinite costs");
            // Credits/adjustments must not restore already consumed allowance.
            total += amount.max(0.0);
        }
    }
    ensure!(
        cursor >= end && total.is_finite(),
        "incomplete cost coverage"
    );
    Ok(total)
}
