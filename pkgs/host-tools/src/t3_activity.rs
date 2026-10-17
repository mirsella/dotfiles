//! Read-only guard for the local T3 environment, independent of its SSH tunnel.
use crate::util;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{path::Path, process::Command};

pub fn blocker(state: &Path, now: f64) -> Result<Option<String>> {
    ensure!(now.is_finite(), "non-finite T3 activity check time");
    let cutoff = (now - 1800.0) as i64;
    // V2 projections are authoritative. Provider processes/sessions stay alive
    // between turns, and the legacy projection_turns table is no longer current.
    // Unknown work statuses block conservatively; schema errors fail the check.
    let query = format!(
        r#"
        WITH inactive_runs AS (
            SELECT run_id FROM orchestration_v2_projection_runs
            WHERE status = 'rolled_back'
               OR (status = 'queued' AND json_extract(payload_json, '$.queueHeld') IS 1)
        )
        SELECT reason FROM (
            SELECT 'unfinished T3 turn ' || run_id || ' (' || status || ')' AS reason
            FROM orchestration_v2_projection_runs
            WHERE status NOT IN ('completed', 'interrupted', 'failed', 'cancelled', 'rolled_back')
              AND NOT (status = 'queued' AND json_extract(payload_json, '$.queueHeld') IS 1)
            UNION ALL
            SELECT 'unfinished T3 execution'
            FROM orchestration_v2_projection_nodes
            WHERE kind IN ('root_turn', 'tool_call', 'subagent', 'hook', 'approval_request', 'user_input_request')
              AND status NOT IN ('idle', 'completed', 'interrupted', 'failed', 'cancelled', 'rolled_back')
              AND NOT EXISTS (SELECT 1 FROM inactive_runs WHERE run_id = orchestration_v2_projection_nodes.run_id)
            UNION ALL
            SELECT 'unfinished T3 tool or subagent'
            FROM orchestration_v2_projection_turn_items
            WHERE type IN ('command_execution', 'dynamic_tool', 'subagent')
              AND status NOT IN ('idle', 'completed', 'interrupted', 'failed', 'cancelled', 'rolled_back')
              AND NOT EXISTS (SELECT 1 FROM inactive_runs WHERE run_id = orchestration_v2_projection_turn_items.run_id)
            UNION ALL
            SELECT 'T3 background task'
            FROM orchestration_v2_projection_provider_threads
            WHERE json_array_length(payload_json, '$.pendingBackgroundTasks') > 0
            UNION ALL
            SELECT 'pending T3 effect'
            FROM orchestration_v2_effect_outbox
            WHERE status NOT IN ('succeeded', 'failed', 'cancelled')
            UNION ALL
            SELECT 'T3 thread preparation'
            FROM orchestration_v2_thread_launch_workflows
            WHERE status NOT IN ('completed', 'failed', 'cancelled')
            UNION ALL
            SELECT 'T3 turn finished within the 1800s quiet period'
            FROM orchestration_v2_projection_runs
            WHERE unixepoch(completed_at) >= {cutoff}
        ) LIMIT 1
        "#
    );
    let output = util::command_output(
        Command::new("sqlite3")
            .args(["-readonly", "-json", "-cmd", ".timeout 3000"])
            .arg(state)
            .arg(query),
    )
    .with_context(|| format!("read T3 activity from {}", state.display()))?;
    // sqlite3 emits no bytes for an empty result set.
    if output.is_empty() {
        return Ok(None);
    }
    #[derive(Deserialize)]
    struct Row {
        reason: String,
    }
    let rows: Vec<Row> = serde_json::from_slice(&output).context("invalid T3 activity result")?;
    Ok(rows.into_iter().next().map(|row| row.reason))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture(path: &Path, data: &str) -> Result<()> {
        // Only columns read by the guard; fixture data is synthetic.
        let schema = r#"
            CREATE TABLE orchestration_v2_projection_runs (run_id TEXT, status TEXT, completed_at TEXT, payload_json TEXT);
            CREATE TABLE orchestration_v2_projection_nodes (run_id TEXT, kind TEXT, status TEXT);
            CREATE TABLE orchestration_v2_projection_turn_items (run_id TEXT, type TEXT, status TEXT);
            CREATE TABLE orchestration_v2_projection_provider_threads (payload_json TEXT);
            CREATE TABLE orchestration_v2_effect_outbox (status TEXT);
            CREATE TABLE orchestration_v2_thread_launch_workflows (status TEXT);
        "#;
        util::command_output(
            Command::new("sqlite3")
                .arg(path)
                .arg(format!("{schema}{data}")),
        )?;
        Ok(())
    }

    #[test]
    fn idle_environment_and_held_queue_do_not_block_sleep() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("statev2.sqlite");
        fixture(
            &path,
            r#"
            INSERT INTO orchestration_v2_projection_runs VALUES ('held', 'queued', NULL, '{"queueHeld":true}');
            INSERT INTO orchestration_v2_projection_provider_threads VALUES ('{"pendingBackgroundTasks":[]}');
            INSERT INTO orchestration_v2_projection_nodes VALUES (NULL, 'root_turn', 'idle');
            INSERT INTO orchestration_v2_projection_nodes VALUES ('held', 'root_turn', 'pending');
            INSERT INTO orchestration_v2_projection_nodes VALUES ('old', 'todo_list', 'running');
            INSERT INTO orchestration_v2_projection_turn_items VALUES (NULL, 'todo_list', 'running');
            INSERT INTO orchestration_v2_effect_outbox VALUES ('succeeded');
            INSERT INTO orchestration_v2_thread_launch_workflows VALUES ('completed');
        "#,
        )?;
        assert!(blocker(&path, 7200.)?.is_none());
        Ok(())
    }

    #[test]
    fn unfinished_work_blocks_even_without_an_ssh_session() -> Result<()> {
        for data in [
            "INSERT INTO orchestration_v2_projection_runs VALUES ('run', 'queued', NULL, '{}');",
            "INSERT INTO orchestration_v2_projection_runs VALUES ('run', 'running', NULL, '{}');",
            "INSERT INTO orchestration_v2_projection_runs VALUES ('run', 'waiting', NULL, '{}');",
            "INSERT INTO orchestration_v2_projection_runs VALUES ('run', 'future-status', NULL, '{}');",
            "INSERT INTO orchestration_v2_projection_nodes VALUES (NULL, 'root_turn', 'running');",
            "INSERT INTO orchestration_v2_projection_turn_items VALUES (NULL, 'command_execution', 'running');",
            "INSERT INTO orchestration_v2_projection_turn_items VALUES (NULL, 'subagent', 'waiting');",
            r#"INSERT INTO orchestration_v2_projection_provider_threads VALUES ('{"pendingBackgroundTasks":[{"taskId":"build"}]}');"#,
            "INSERT INTO orchestration_v2_effect_outbox VALUES ('pending');",
            "INSERT INTO orchestration_v2_thread_launch_workflows VALUES ('preparing');",
        ] {
            let temp = tempfile::tempdir()?;
            let path = temp.path().join("statev2.sqlite");
            fixture(&path, data)?;
            assert!(blocker(&path, 7200.)?.is_some(), "{data}");
        }
        Ok(())
    }

    #[test]
    fn completed_turns_observe_the_quiet_period_without_writing_state() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("statev2.sqlite");
        fixture(
            &path,
            "INSERT INTO orchestration_v2_projection_runs VALUES ('run', 'completed', '1970-01-01T01:30:00.000Z', '{}');",
        )?;
        let before = fs::read(&path)?;
        assert!(blocker(&path, 7199.)?.is_some());
        assert!(blocker(&path, 7201.)?.is_none());
        assert_eq!(fs::read(&path)?, before);
        Ok(())
    }

    #[test]
    fn rolled_back_work_is_not_still_running() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("statev2.sqlite");
        fixture(
            &path,
            r#"
            INSERT INTO orchestration_v2_projection_runs VALUES ('old', 'rolled_back', NULL, '{}');
            INSERT INTO orchestration_v2_projection_nodes VALUES ('old', 'root_turn', 'running');
            INSERT INTO orchestration_v2_projection_turn_items VALUES ('old', 'command_execution', 'running');
        "#,
        )?;
        assert!(blocker(&path, 7200.)?.is_none());
        Ok(())
    }

    #[test]
    fn missing_corrupt_or_incompatible_state_never_reports_idle() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("statev2.sqlite");
        assert!(blocker(&path, 7200.).is_err());
        assert!(!path.exists());
        fs::write(&path, "broken database")?;
        assert!(blocker(&path, 7200.).is_err());
        fs::remove_file(&path)?;
        util::command_output(
            Command::new("sqlite3")
                .arg(&path)
                .arg("CREATE TABLE unrelated(x);"),
        )?;
        assert!(blocker(&path, 7200.).is_err());
        Ok(())
    }
}
