//! Shared-organization routing: use only one calendar cap per grant cycle.
//! This deliberately leaves days 1..renewal unused instead of trusting delayed
//! financial reporting or mistaking a project reset for a new grant.
use anyhow::{Result, ensure};
use chrono::{DateTime, Datelike, Utc};
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub api_upstream: String,
    pub project_id: String,
    pub enforced_project_limit_usd: u32,
    pub grant_usd: u32,
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        let url = reqwest::Url::parse(&self.api_upstream)?;
        ensure!(
            url.scheme() == "http"
                && url.host_str() == Some("127.0.0.1")
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "OpenAI API must use local Sleev"
        );
        ensure!(
            self.project_id.starts_with("proj_")
                && self
                    .project_id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
            "invalid project identity"
        );
        ensure!(
            self.enforced_project_limit_usd > 0 && self.enforced_project_limit_usd < self.grant_usd,
            "project hard limit must leave a positive grant buffer"
        );
        Ok(())
    }

    pub async fn verify(
        &self,
        client: &reqwest::Client,
        key: reqwest::header::HeaderValue,
        org: &str,
    ) -> Result<HashSet<String>> {
        let response = client
            .get(format!(
                "{}/v1/models",
                self.api_upstream.trim_end_matches('/')
            ))
            .bearer_auth(key.to_str()?)
            .header("openai-organization", org)
            .header("openai-project", &self.project_id)
            .send()
            .await?;
        ensure!(
            response.status().is_success(),
            "OpenAI project metadata check failed: {}",
            response.status()
        );
        let value: serde_json::Value = response.json().await?;
        ensure!(value["object"] == "list", "invalid OpenAI model inventory");
        let data = value["data"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing model inventory"))?;
        let models: HashSet<_> = data
            .iter()
            .filter_map(|model| model["id"].as_str().map(str::to_owned))
            .collect();
        ensure!(!models.is_empty(), "empty OpenAI project model inventory");
        Ok(models)
    }
}

pub fn window_open(anchor: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    // Month-end anchors are clamped, matching the policy's renewal arithmetic.
    let next_month = now.date_naive().with_day(1).unwrap() + chrono::Months::new(1);
    let last_day = next_month.pred_opt().unwrap().day();
    now.day() >= anchor.day().min(last_day)
}

pub fn api_path(path: &str) -> String {
    if path.starts_with("/v1/") {
        path.into()
    } else {
        format!("/v1{path}")
    }
}

pub fn exhausted(status: reqwest::StatusCode, bytes: &[u8]) -> bool {
    if status != reqwest::StatusCode::TOO_MANY_REQUESTS {
        return false;
    }
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return false;
    };
    matches!(
        value["error"]["code"].as_str(),
        Some(
            "credit_balance_exhausted"
                | "project_spend_limit_exceeded"
                | "organization_spend_limit_exceeded"
                | "organization_usage_limit_exceeded"
        )
    )
}
