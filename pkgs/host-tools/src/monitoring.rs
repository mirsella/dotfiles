use crate::util;
use anyhow::{Context, Result, anyhow, ensure};
use chrono::{DateTime, Local, SecondsFormat};
use regex::Regex;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    borrow::Cow,
    collections::HashMap,
    env,
    fmt::Write as _,
    fs,
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
    sync::LazyLock,
    thread,
    time::{Duration, Instant},
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum HardwareWarning {
    ClockWatchdog,
    MachineCheck,
    GpuReset,
    Thermal,
    Pcie,
    Nvme,
}

impl HardwareWarning {
    fn classify(message: &str) -> Option<Self> {
        let lower = message.to_ascii_lowercase();
        if message.contains("clocksource: Watchdog") && message.contains("read timed out") {
            Some(Self::ClockWatchdog)
        } else if (lower.contains("machine check") || lower.contains("mce: [hardware error]"))
            && !lower.contains("decoding enabled")
            // Corrected and merely logged checks are routine; only an actionable
            // hardware error is worth a notification that stays on screen.
            && !(lower.contains("corrected") && !lower.contains("uncorrected"))
            && !lower.contains("events logged")
        {
            Some(Self::MachineCheck)
        } else if lower.contains("gpu reset")
            || (lower.contains("amdgpu") && lower.contains("ring") && lower.contains("timeout"))
        {
            Some(Self::GpuReset)
        } else if lower.contains("critical temperature reached")
            || (lower.contains("thermal") && lower.contains("critical"))
        {
            Some(Self::Thermal)
        } else if lower.contains("aer:") && lower.contains("uncorrected") {
            Some(Self::Pcie)
        } else if lower.contains("nvme")
            && (lower.contains("timeout")
                || lower.contains("controller is down")
                || lower.contains("reset controller")
                || lower.contains("removing after probe failure"))
        {
            Some(Self::Nvme)
        } else {
            None
        }
    }

    fn summary(self) -> &'static str {
        match self {
            Self::ClockWatchdog => "CPU timing watchdog timed out",
            Self::MachineCheck => "Machine check (hardware error)",
            Self::GpuReset => "GPU reset or timeout",
            Self::Thermal => "Thermal trip",
            Self::Pcie => "Uncorrected PCIe error",
            Self::Nvme => "NVMe controller error",
        }
    }
}

/// One persistent notification per warning class and window, even when a storm
/// logs a slightly different line for every affected CPU.
const ALERT_COOLDOWN: Duration = Duration::from_secs(600);

/// A sticky notification stays until dismissed and carries critical urgency;
/// a transient one expires on its own.
fn notify_send(summary: &str, body: &str, sticky: bool) -> Result<()> {
    let (urgency, expire) = if sticky {
        ("critical", "0")
    } else {
        ("normal", "10000")
    };
    // notify-send blocks while the desktop bus activates the notification daemon,
    // which must never stall the control loops that raise an alert.
    util::run(
        "timeout",
        &[
            "--signal=TERM",
            "5",
            "notify-send",
            "--app-name=Hardware",
            &format!("--urgency={urgency}"),
            &format!("--expire-time={expire}"),
            summary,
            body,
        ],
    )
}

pub fn notify(summary: &str, body: &str) {
    // Transient event toast; failures must not disturb the maintainer loops.
    if let Err(error) = notify_send(summary, body, false) {
        eprintln!("Could not show notification: {error:#}");
    }
}

fn persistent_notification(summary: &str, body: &str) -> Result<()> {
    let mut attempt = 1;
    loop {
        match notify_send(summary, body, true) {
            Ok(()) => return Ok(()),
            Err(error) if attempt == 10 => {
                return Err(error).context("show persistent notification");
            }
            Err(_) => {
                attempt += 1;
                thread::sleep(Duration::from_secs(2));
            }
        }
    }
}

fn clean_from_search(code: Option<i32>, stdout: &[u8], stderr: &[u8]) -> Result<bool> {
    match code {
        Some(0) => Ok(!stdout.is_empty()),
        // journalctl exits 1 both for "no match" and for an unknown boot, and only
        // the latter complains on stderr.
        Some(1) if stderr.is_empty() => Ok(false),
        _ => Err(anyhow!(
            "previous boot journal search exited {}: {}",
            code.map_or_else(|| "on a signal".to_owned(), |code| code.to_string()),
            String::from_utf8_lossy(stderr).trim()
        )),
    }
}

fn clean_shutdown(previous_boot: &str) -> Result<bool> {
    let output = Command::new("journalctl")
        .args([
            &format!("--boot={previous_boot}"),
            "--output=cat",
            "--no-pager",
            // logind and systemd-shutdown announce whichever action was started.
            "--grep=System is (powering down|rebooting|halting|shutting down|performing factory reset)",
        ])
        .output()
        .context("inspect previous boot journal")?;
    clean_from_search(output.status.code(), &output.stdout, &output.stderr)
}

/// The unit loads ~/.config/telegram.env when present; without it delivery is
/// skipped rather than failing the alert.
fn telegram_configured() -> bool {
    env::var("TgToken").is_ok() && env::var("TgId").is_ok()
}

fn mirror_to_telegram(text: String) {
    if !telegram_configured() {
        return;
    }
    let result = hostname().and_then(|host| telegram(&bounded_message(format!("{host}: {text}"))));
    if let Err(error) = result {
        eprintln!("Could not mirror the alert to Telegram: {error:#}");
    }
}

fn previous_boot_notice() -> Result<()> {
    let boot_id = util::read("/proc/sys/kernel/random/boot_id")?
        .trim()
        .to_owned();
    let state = Path::new(&env::var("STATE_DIRECTORY")?).join("notified-boot");
    if fs::read_to_string(&state).is_ok_and(|text| text.trim() == boot_id) {
        return Ok(());
    }
    // Inspect before claiming the boot: an unusable journal must be retried on
    // the next service start, while a delivered verdict must never repeat.
    let claim = || util::atomic_write(&state, format!("{boot_id}\n").as_bytes());
    match clean_shutdown("-1") {
        Ok(true) => {
            claim()?;
            println!("Previous boot ended with a clean shutdown");
        }
        Ok(false) => {
            claim()?;
            if let Err(error) = persistent_notification(
                "Previous shutdown was not clean",
                "The last session ended without a shutdown sequence: a reset, power loss or freeze is likely.\n\nInspect with: journalctl -b -1 -k",
            ) {
                eprintln!("Could not report the previous shutdown: {error:#}");
            }
            mirror_to_telegram(
                "previous shutdown was not clean (reset, power loss or freeze)".to_owned(),
            );
        }
        Err(error) => println!("Could not inspect the previous boot: {error:#}"),
    }
    Ok(())
}

pub fn hardware_alerts(test: bool) -> Result<()> {
    if test {
        // Report a delivery failure instead of silently passing the check.
        notify_send(
            "Hardware alert test",
            "Persistent desktop notifications work; this one stays until dismissed.",
            true,
        )?;
        println!("Test notification sent.");
        if telegram_configured() {
            telegram(&format!(
                "Hardware alert test from {}. Telegram delivery works.",
                hostname()?
            ))?;
            println!("Telegram test delivered.");
        }
        return Ok(());
    }
    previous_boot_notice()?;
    let mut child = Command::new("journalctl")
        .args([
            "--quiet",
            "--no-pager",
            "--follow",
            // Journalctl replays its last lines on start; only new events matter.
            "--lines=0",
            "--output=cat",
            "_TRANSPORT=kernel",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .context("start kernel journal follower")?;
    let result = (|| {
        let stdout = child.stdout.take().context("journal stdout missing")?;
        let mut last: HashMap<HardwareWarning, Instant> = HashMap::new();
        // Kernel messages are raw bytes; a non-UTF-8 line must not end the follower.
        for chunk in BufReader::new(stdout).split(b'\n') {
            let bytes = chunk?;
            let line = String::from_utf8_lossy(&bytes);
            let Some(warning) = HardwareWarning::classify(&line) else {
                continue;
            };
            let summary = warning.summary();
            println!("{summary}: {line}");
            if last
                .get(&warning)
                .is_some_and(|sent| sent.elapsed() < ALERT_COOLDOWN)
            {
                continue;
            }
            last.insert(warning, Instant::now());
            if let Err(error) = persistent_notification(summary, line.trim()) {
                // An undelivered alert must not consume the class's window.
                last.remove(&warning);
                eprintln!("Could not show the {summary} alert: {error:#}");
            }
            mirror_to_telegram(format!("{summary}\n{}", line.trim()));
        }
        Err(anyhow!("kernel journal follower closed"))
    })();
    let _ = child.kill();
    let _ = child.wait();
    result
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_kernel_hardware_warnings() {
        assert_eq!(
            HardwareWarning::classify("clocksource: Watchdog remote CPU 15 read timed out"),
            Some(HardwareWarning::ClockWatchdog)
        );
        assert_eq!(
            HardwareWarning::classify(
                "mce: [Hardware Error]: Uncorrected, software restartable error."
            ),
            Some(HardwareWarning::MachineCheck)
        );
        for benign in [
            "MCE: In-kernel MCE decoding enabled.",
            "mce: [Hardware Error]: Corrected error, no action required.",
            "mce: [Hardware Error]: Machine check events logged",
        ] {
            assert_eq!(HardwareWarning::classify(benign), None);
        }
        assert_eq!(
            HardwareWarning::classify("amdgpu 0000:c1:00.0: [gfxhub] ring gfx timeout"),
            Some(HardwareWarning::GpuReset)
        );
        assert_eq!(
            HardwareWarning::classify("thermal thermal_zone0: critical temperature reached"),
            Some(HardwareWarning::Thermal)
        );
        assert_eq!(
            HardwareWarning::classify(
                "pcieport 0000:00:01.1: AER: Uncorrected (Non-Fatal) error received"
            ),
            Some(HardwareWarning::Pcie)
        );
        assert_eq!(
            HardwareWarning::classify("nvme nvme0: I/O 12 QID 1 timeout, reset controller"),
            Some(HardwareWarning::Nvme)
        );
        for benign in [
            "wlp1s0: Limiting TX power",
            "pcieport 0000:00:01.1: AER: Corrected error received",
            "nvme nvme0: pci function 0000:01:00.0",
            "thermal thermal_zone0: polling every 5s",
        ] {
            assert_eq!(HardwareWarning::classify(benign), None);
        }
    }

    #[test]
    fn maps_journal_searches_to_shutdown_state() {
        assert!(clean_from_search(Some(0), b"System is powering down.\n", b"").unwrap());
        assert!(!clean_from_search(Some(0), b"", b"").unwrap());
        // A searched boot with no marker is the crash case.
        assert!(!clean_from_search(Some(1), b"", b"").unwrap());
        // Exit 1 with a complaint means the boot could not be inspected.
        assert!(clean_from_search(Some(1), b"", b"No journal boot entry found.").is_err());
        assert!(clean_from_search(None, b"", b"").is_err());
    }
}
