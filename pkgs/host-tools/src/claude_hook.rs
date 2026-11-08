use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    io,
    path::Path,
    process::{Command, Stdio},
};

pub fn run(browser_mcp: &Path) -> ! {
    match forward(browser_mcp) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("Claude browser hook failed: {error:#}");
            // Claude exit 2 blocks malformed PreToolUse routing.
            std::process::exit(2);
        }
    }
}

fn forward(browser_mcp: &Path) -> Result<i32> {
    ensure!(
        browser_mcp.is_absolute(),
        "browser-mcp path must be absolute"
    );
    let mut event: Value =
        serde_json::from_reader(io::stdin().lock()).context("invalid Claude browser hook input")?;
    adapt(&mut event)?;
    let mut child = Command::new(browser_mcp)
        .arg("codex-hook")
        .stdin(Stdio::piped())
        .spawn()
        .context("start published browser lifecycle hook")?;
    let written = serde_json::to_writer(
        child.stdin.take().expect("spawn requested piped stdin"),
        &event,
    );
    // Collect the child even when writing fails; dropping its stdin gives the
    // native hook EOF, and its stdout/stderr remain connected to Claude.
    let status = child.wait().context("wait for browser lifecycle hook")?;
    written.context("write adapted browser hook event")?;
    status
        .code()
        .context("browser lifecycle hook terminated without an exit code")
}

fn adapt(event: &mut Value) -> Result<()> {
    let id = event["session_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .context("Claude browser hook requires session_id")?;
    // The published protocol owns validation, identity injection and queued
    // release. Tag Claude IDs so they cannot share an ordinary Codex session.
    event["session_id"] = format!("claude:{id}").into();
    if event["hook_event_name"] == "PreToolUse"
        && let Some(action) = event["tool_name"]
            .as_str()
            .and_then(|name| name.strip_prefix("mcp__plugin_hm_browser__"))
    {
        event["tool_name"] = format!("mcp__browser__{action}").into();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn adapts_browser_namespaces_without_changing_arguments() {
        for name in ["mcp__plugin_hm_browser__dispatch", "mcp__browser__dispatch"] {
            let input = json!({"action":"status","__codex_session_id":"forged"});
            let mut event = json!({"hook_event_name":"PreToolUse", "session_id":"session", "tool_name":name,"tool_input":input});
            adapt(&mut event).unwrap();
            assert_eq!(event["session_id"], "claude:session");
            assert_eq!(event["tool_name"], "mcp__browser__dispatch");
            assert_eq!(event["tool_input"], input);
        }
    }

    #[test]
    fn tags_cleanup_with_the_same_identity() {
        let mut event = json!({"hook_event_name":"SessionEnd","session_id":"session"});
        adapt(&mut event).unwrap();
        assert_eq!(
            event,
            json!({"hook_event_name":"SessionEnd","session_id":"claude:session"})
        );
    }

    #[test]
    fn rejects_missing_empty_and_non_string_identity() {
        for mut event in [
            json!({}),
            json!({"session_id":""}),
            json!({"session_id":42}),
            json!(null),
        ] {
            assert!(adapt(&mut event).is_err());
        }
    }
}
