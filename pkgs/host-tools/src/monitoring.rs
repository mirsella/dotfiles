use crate::util;
use anyhow::{Context, Result, anyhow, ensure};
use chrono::{DateTime, Local, SecondsFormat};
use regex::Regex;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    borrow::Cow,
    env,
    fmt::Write as _,
    fs,
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
    sync::LazyLock,
};

pub fn storage_event(message: &str) -> bool {
    static EVENT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(concat!(
        r"\busb 2-(?:2|3|4)(?:\.\d+)*: (?:USB disconnect\b|reset .* USB device\b|",
        r"device descriptor read/.*error|device not accepting address|unable to enumerate USB device)|",
        r"\buas_(?:eh_\w+|zap_pending)\b|\bsd \S+: \[sd[a-z]+\] Synchronize Cache.*failed|",
        r"\bI/O error.*\bdev (?:sd[a-z]+\d*|dm-\d+)\b|\bxhci_hcd\b.*(?:HC died|host controller not responding)"
    )).unwrap()
    });
    EVENT.is_match(message)
}

fn hostname() -> Result<String> {
    Ok(util::read("/proc/sys/kernel/hostname")?.trim().into())
}

pub fn telegram(text: &str) -> Result<()> {
    let token = env::var("TgToken")?;
    let result = (|| -> Result<()> {
        let mut response = util::agent(20)
            .post(format!("https://api.telegram.org/bot{token}/sendMessage"))
            .send_json(json!({"chat_id":env::var("TgId")?,"text":text}))?;
        let status = response.status();
        let data: Value = response
            .body_mut()
            .read_json()
            .context("Telegram returned invalid JSON")?;
        ensure!(
            status.is_success() && data["ok"] == true,
            "Telegram HTTP {status}: {}",
            data["description"]
        );
        Ok(())
    })();
    result.map_err(|e| anyhow!("{}", format!("{e:#}").replace(&token, "[redacted]")))
}

pub fn bounded_message(mut text: String) -> String {
    let suffix = "\n[Details truncated; see kernel journal.]";
    if text.encode_utf16().count() <= 4096 {
        return text;
    }
    let budget = 4096 - suffix.encode_utf16().count();
    let mut used = 0;
    let mut end = 0;
    for (index, c) in text.char_indices() {
        if used + c.len_utf16() > budget {
            break;
        }
        used += c.len_utf16();
        end = index + c.len_utf8();
    }
    text.truncate(end);
    text.push_str(suffix);
    text
}

#[derive(Deserialize)]
pub struct JournalEntry {
    #[serde(rename = "__CURSOR")]
    cursor: String,
    #[serde(rename = "__REALTIME_TIMESTAMP")]
    timestamp: Option<String>,
    #[serde(rename = "MESSAGE")]
    message: Option<JournalMessage>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum JournalMessage {
    Text(String),
    Bytes(Vec<u8>),
}

pub struct JournalUpdate {
    cursor: String,
    message: Option<String>,
}

impl JournalUpdate {
    pub fn deliver(self, cursor_file: &Path, send: impl FnOnce(&str) -> Result<()>) -> Result<()> {
        if let Some(message) = self.message {
            send(&message)?;
            println!("Telegram storage notification delivered.");
        }
        util::atomic_write(cursor_file, format!("{}\n", self.cursor).as_bytes())
    }
}

pub fn collect_entries(
    entries: impl IntoIterator<Item = Result<JournalEntry>>,
    initial: bool,
    host: &str,
) -> Result<Option<JournalUpdate>> {
    let mut last = None;
    let mut count = 0;
    let mut details = String::new();
    let mut detail_budget = 4096usize;
    for entry in entries {
        let entry = entry?;
        last = Some(entry.cursor);
        if initial {
            continue;
        }
        let message = match entry.message.as_ref().context("missing journal MESSAGE")? {
            JournalMessage::Text(s) => Cow::Borrowed(s.as_str()),
            JournalMessage::Bytes(bytes) => String::from_utf8_lossy(bytes),
        };
        if storage_event(&message) {
            count += 1;
            if detail_budget > 0 {
                let micros = entry
                    .timestamp
                    .as_deref()
                    .context("missing journal timestamp")?
                    .parse()?;
                let stamp = DateTime::from_timestamp_micros(micros)
                    .context("invalid journal timestamp")?
                    .with_timezone(&Local)
                    .to_rfc3339_opts(SecondsFormat::Secs, false);
                let start = details.len();
                write!(details, "\n\n{stamp}  ")?;
                details.extend(message.chars().take(4096));
                detail_budget =
                    detail_budget.saturating_sub(details[start..].encode_utf16().count());
            }
        }
    }
    let Some(cursor) = last else {
        ensure!(
            !initial,
            "No kernel journal entries; check journal permissions"
        );
        return Ok(None);
    };
    let message = (count > 0).then(|| {
        bounded_message(format!(
            "Storage connection alert on {host}\nMatching kernel messages: {count}{details}"
        ))
    });
    Ok(Some(JournalUpdate { cursor, message }))
}

pub fn read_journal(
    command: &mut Command,
    initial: bool,
    host: &str,
) -> Result<Option<JournalUpdate>> {
    let mut child = command
        .stdout(Stdio::piped())
        .spawn()
        .context("start kernel journal reader")?;
    let result = (|| {
        let stdout = child.stdout.take().context("journal stdout missing")?;
        let entries = BufReader::new(stdout)
            .lines()
            .map(|line| Ok(serde_json::from_str(&line?)?));
        let update = collect_entries(entries, initial, host)?;
        let status = child.wait()?;
        ensure!(
            status.success(),
            "journal reader exited {status}; cursor not acknowledged"
        );
        Ok(update)
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

pub fn alerts(test: bool) -> Result<()> {
    if test {
        telegram(&format!(
            "Storage alert test from {}. Telegram delivery works.",
            hostname()?
        ))?;
        println!("Telegram test delivered.");
        return Ok(());
    }
    let cursor_file = Path::new(&env::var("STATE_DIRECTORY")?).join("cursor");
    let cursor = match fs::read_to_string(&cursor_file) {
        Ok(s) => {
            ensure!(
                !s.trim().is_empty(),
                "Empty journal checkpoint: {}",
                cursor_file.display()
            );
            Some(s.trim().to_owned())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let from = cursor
        .as_ref()
        .map(|c| format!("--after-cursor={c}"))
        .unwrap_or_else(|| "--lines=1".into());
    let host = hostname()?;
    let update = read_journal(
        Command::new("journalctl").args([
            "--quiet",
            "--no-pager",
            "--all",
            "--output=json",
            "--output-fields=__CURSOR,__REALTIME_TIMESTAMP,MESSAGE",
            "_TRANSPORT=kernel",
            &from,
        ]),
        cursor.is_none(),
        &host,
    )?;
    if let Some(update) = update {
        update.deliver(&cursor_file, telegram)?;
    }
    if cursor.is_none() {
        println!("Monitoring starts at the current journal tail; historical events skipped.");
    }
    Ok(())
}

pub fn mail(key_file: &Path, recipient: &str, subject: &str, text: &str) -> Result<()> {
    let key = util::read(key_file)?.trim().to_owned();
    ensure!(!key.is_empty(), "empty Resend key");
    let result = (|| -> Result<()> {
        let mut response = util::agent(30).post("https://api.resend.com/emails")
            .header("Authorization", format!("Bearer {key}"))
            .send_json(json!({"from":"Predator storage <noreply@voxride.com>","to":[recipient],"subject":subject,"text":text}))?;
        ensure!(
            response.status().is_success(),
            "Resend HTTP {}",
            response.status()
        );
        let data: Value = response.body_mut().read_json()?;
        ensure!(
            data["id"].is_string(),
            "Resend did not acknowledge delivery"
        );
        println!("Storage notification accepted by Resend");
        Ok(())
    })();
    result.map_err(|e| anyhow!("{}", format!("{e:#}").replace(&key, "[redacted]")))
}
