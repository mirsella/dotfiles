//! The actionable "idle workspaces need attention" notification.
//!
//! The hourly `sweep auto` is a oneshot unit, and systemd stops its leftover
//! processes with it, so it hands the notification to a transient user unit
//! that waits for a click. Copy puts the list on the clipboard through Klipper;
//! Ask T3 Code starts a thread in ~/dev/dotfiles on this machine's T3 server
//! that explains each workspace and links the T3 threads behind it, and opens
//! it in the desktop app.
use crate::{t3_settings, util};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// The T3 project the explaining thread runs in, relative to HOME.
const PROJECT: &str = "dev/dotfiles";
const TITLE: &str = "Why idle workspaces are kept";
/// How long to wait for a notification service that is not up yet, such as
/// right after login; headless hosts give up after this.
const SERVICE_WAIT: Duration = Duration::from_secs(600);

#[derive(Deserialize, Serialize)]
pub struct Request {
    pub summary: String,
    pub body: String,
    /// Every workspace that needs attention: a `- path: reason` line each,
    /// then the T3 threads bound to it as Markdown links, when known.
    pub details: String,
}

/// Queue the notification in a transient unit that outlives this run.
pub fn post(state_dir: &Path, request: &Request) -> Result<()> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    fs::create_dir_all(state_dir)?;
    let file = state_dir.join(format!("attention-{stamp}.json"));
    util::atomic_write(&file, &serde_json::to_vec(request)?)?;
    let exe = std::env::current_exe().context("locate host-tools")?;
    let mut command = Command::new("systemd-run");
    command.args([
        "--user",
        "--quiet",
        "--collect",
        "--description=Idle workspaces need attention",
        // A notification nobody answers must not keep its unit forever.
        "--property=RuntimeMaxSec=7d",
    ]);
    command.arg(format!("--unit=sweep-attention-{stamp}"));
    // The manager's environment may lack this unit's notify-send.
    for key in ["PATH", "DBUS_SESSION_BUS_ADDRESS"] {
        if let Some(value) = std::env::var_os(key) {
            let mut setting = OsString::from(format!("--setenv={key}="));
            setting.push(value);
            command.arg(setting);
        }
    }
    command
        .arg("--")
        .arg(exe)
        .args(["sweep", "notify"])
        .arg(&file);
    let result = util::command_output(&mut command).map(|_| ());
    if result.is_err() {
        let _ = fs::remove_file(&file);
    }
    result.context("start the attention notification unit")
}

/// Show the queued notification and run the clicked action.
pub fn wait(file: &Path) -> Result<()> {
    let request: Request = serde_json::from_slice(
        &fs::read(file).with_context(|| format!("read {}", file.display()))?,
    )
    .context("parse the attention request")?;
    fs::remove_file(file).with_context(|| format!("remove {}", file.display()))?;
    let (label, result) = match show(&request)?.as_str() {
        "copy" => ("copy the list", copy(&clipboard_text(&request.details)?)),
        "ask" => ("ask T3 Code", ask(&request.details)),
        // Dismissed or closed.
        _ => return Ok(()),
    };
    if let Err(error) = &result {
        let summary = format!("Could not {label}");
        let body = format!("{error:#}");
        let _ = util::run(
            "timeout",
            &[
                "--signal=TERM",
                "5",
                "notify-send",
                "--app-name=Storage",
                "--urgency=normal",
                &summary,
                &body,
            ],
        );
    }
    result
}

/// The clicked action's key, or an empty string when the notification closed.
fn show(request: &Request) -> Result<String> {
    let deadline = Instant::now() + SERVICE_WAIT;
    loop {
        let output = Command::new("notify-send")
            .args([
                "--app-name=Storage",
                "--urgency=normal",
                "--expire-time=0",
                "--action=copy=Copy",
                "--action=ask=Ask T3 Code",
                "--wait",
                &request.summary,
                &request.body,
            ])
            .stdin(Stdio::null())
            .output()
            .context("start notify-send")?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned());
        }
        ensure!(
            Instant::now() < deadline,
            "notify-send exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
        std::thread::sleep(Duration::from_secs(30));
    }
}

fn clipboard_text(details: &str) -> Result<String> {
    Ok(format!(
        "Idle workspaces sweep keeps on {}:\n{details}\n",
        crate::monitoring::hostname()?
    ))
}

fn copy(text: &str) -> Result<()> {
    let connection = dbus::blocking::Connection::new_session().context("connect to D-Bus")?;
    connection
        .with_proxy("org.kde.klipper", "/klipper", Duration::from_secs(5))
        .method_call::<(), _, _, _>("org.kde.klipper.klipper", "setClipboardContents", (text,))
        .context("set the clipboard through Klipper")
}

fn prompt(host: &str, details: &str) -> String {
    format!(
        "The hourly sweep cleanup on {host} keeps these idle workspaces because removing them \
         would lose work. Under some are the T3 threads bound to them, as T3 thread links:\n\n\
         {details}\n\n\
         For each workspace:\n\
         1. Link the T3 threads that created it or worked in it, as \
         [title](t3-thread://v2/<environment>/<thread>) links, so I can open the one that holds \
         the work. Start from the threads listed. For the others, search read-only: thread \
         bindings and messages that mention its path in ~/.t3/userdata/statev2.sqlite, OpenCode \
         sessions in ~/.local/share/opencode/opencode.db whose directory is in it, and your T3 \
         thread tools. Say so when you find none.\n\
         2. Explain why it is kept and what unsaved work it holds. Inspect it read-only (git \
         status, unpushed commits, stashes, nested clones and worktrees, unreadable files) and \
         show the evidence.\n\
         3. Recommend what to do: save or push the work, discard it, or keep it with \
         `git worktree lock` or `sweep keep PATH`.\n\n\
         `sweep` lists every workspace with its branch and why it stays; the rules live in \
         pkgs/host-tools/src/worktree_prune.rs and pkgs/host-tools/src/sweep.rs. Don't delete or \
         change anything until I confirm.\n"
    )
}

/// T3 ids that are safe to put in a `t3code://threads/<environment>/<thread>` path.
fn link_segment(id: &str) -> Result<&str> {
    ensure!(
        !id.is_empty()
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte)),
        "unexpected T3 id {id:?}"
    );
    Ok(id)
}

fn ask(details: &str) -> Result<()> {
    let home = PathBuf::from(std::env::var_os("HOME").context("HOME is unset")?);
    let base = t3_settings::resolve_base_dir(&home, None);
    let cli = t3_settings::running_cli(&base)?.context("T3 Code is not running")?;
    let mut child = Command::new(cli)
        .args(["thread", "start", "--json", "--title", TITLE, "--base-dir"])
        .arg(&base)
        .arg(home.join(PROJECT))
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("start t3 thread start")?;
    child
        .stdin
        .take()
        .context("t3 has no stdin")?
        .write_all(prompt(&crate::monitoring::hostname()?, details).as_bytes())?;
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "t3 thread start exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let launched: Value =
        serde_json::from_slice(&output.stdout).context("parse t3 thread start output")?;
    let thread = link_segment(
        launched["threadId"]
            .as_str()
            .context("t3 printed no thread id")?,
    )?;
    let environment = util::read(base.join("userdata/environment-id"))?;
    let environment = link_segment(environment.trim())?;
    let link = format!("t3code://threads/{environment}/{thread}");
    util::run("xdg-open", &[&link])
        .with_context(|| format!("started thread {thread}, but could not open {link}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_only_accept_plain_ids() {
        assert!(link_segment("020898b6-4a15-44cb-ae04-0ef53d75919f").is_ok());
        assert!(link_segment("thread:subagent:d9f9377789c9a786").is_ok());
        for id in ["", "a/b", "a b", "a?b", "../x"] {
            assert!(link_segment(id).is_err(), "accepted {id:?}");
        }
    }

    #[test]
    fn the_prompt_lists_the_workspaces_and_forbids_changes() {
        let text = prompt("main", "- ~/dev/x: has uncommitted changes (2 files)");
        assert!(text.contains("cleanup on main keeps"));
        assert!(text.contains("t3-thread://v2/<environment>/<thread>"));
        assert!(text.contains("- ~/dev/x: has uncommitted changes (2 files)"));
        assert!(text.contains("Don't delete or change anything until I confirm."));
    }
}
