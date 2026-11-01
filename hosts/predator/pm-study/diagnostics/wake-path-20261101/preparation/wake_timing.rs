//! Journal JSON validation with explicitly separate timestamp domains.
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stamp {
    pub source_mono: u64,
    pub source_boot: u64,
    pub receipt_mono: u64,
    pub receipt_real: u64,
}
#[derive(Debug, Clone, Copy)]
pub struct Cycle {
    pub start: Stamp,
    pub end: Stamp,
}
fn field(v: &serde_json::Value, k: &str) -> Result<u64> {
    Ok(v[k]
        .as_str()
        .ok_or_else(|| format!("missing/scalar timestamp {k}"))?
        .parse()?)
}
fn stamp(v: &serde_json::Value, boot: &str) -> Result<Stamp> {
    if v["_BOOT_ID"].as_str() != Some(boot) || v["_TRANSPORT"].as_str() != Some("kernel") {
        return Err("wrong boot or non-kernel phase message".into());
    }
    Ok(Stamp {
        source_mono: field(v, "_SOURCE_MONOTONIC_TIMESTAMP")?,
        source_boot: field(v, "_SOURCE_BOOTTIME_TIMESTAMP")?,
        receipt_mono: field(v, "__MONOTONIC_TIMESTAMP")?,
        receipt_real: field(v, "__REALTIME_TIMESTAMP")?,
    })
}
pub fn cycle(s: &str, boot: &str) -> Result<Cycle> {
    let (mut phase, mut start, mut end, mut returned, mut finished) = (0, None, None, false, false);
    for line in s.lines() {
        let v: serde_json::Value = serde_json::from_str(line)?;
        if v["_BOOT_ID"].as_str() != Some(boot) {
            return Err("journal boot changed".into());
        }
        let msg = v["MESSAGE"].as_str().ok_or("non-text journal message")?;
        for forbidden in [
            "Low-level resume complete",
            "PM: suspend debug:",
            "BUG:",
            "Oops:",
            "Call Trace:",
        ] {
            if msg.contains(forbidden) {
                return Err(format!("unexpected {forbidden}").into());
            }
        }
        // Timekeeping suspension is valid in s2idle's cpuidle/tick_freeze path.
        if msg.contains("PM: suspend entry (") {
            if phase != 0 || !msg.contains("(s2idle)") {
                return Err("not exactly one s2idle entry".into());
            }
            stamp(&v, boot)?;
            phase = 1;
        } else if msg == "PM: suspend-to-idle" {
            if phase != 1 {
                return Err("unexpected idle loop".into());
            }
            start = Some(stamp(&v, boot)?);
            phase = 2;
        } else if msg == "PM: resume from suspend-to-idle" {
            if phase != 2 {
                return Err("unexpected idle return".into());
            }
            end = Some(stamp(&v, boot)?);
            phase = 3;
        } else if msg == "PM: suspend exit" {
            if phase != 3 {
                return Err("exit without idle return".into());
            }
            stamp(&v, boot)?;
            phase = 4;
        }
        if v["_SYSTEMD_UNIT"].as_str() == Some("systemd-suspend.service")
            && msg.contains("System returned from sleep operation")
        {
            returned = true;
        }
        if v["UNIT"].as_str() == Some("systemd-suspend.service")
            && (msg.contains("Finished System Suspend.")
                || msg.contains("Finished systemd-suspend.service - System Suspend."))
        {
            finished = true;
        }
    }
    if phase != 4 || !returned || !finished {
        return Err("incomplete kernel/service return".into());
    }
    let c = Cycle {
        start: start.ok_or("no start")?,
        end: end.ok_or("no end")?,
    };
    if c.end.source_mono < c.start.source_mono
        || c.end.source_boot < c.start.source_boot
        || c.end.receipt_mono < c.start.receipt_mono
        || c.end.receipt_real < c.start.receipt_real
    {
        return Err("backwards timestamp domain".into());
    }
    Ok(c)
}
pub fn receive(
    rx: &mpsc::Receiver<std::io::Result<String>>,
    boot: &str,
) -> (String, Result<Cycle>) {
    let deadline = Instant::now() + Duration::from_secs(1800);
    let (mut s, mut exit, mut finished) = (String::new(), false, false);
    let r = (|| loop {
        let left = deadline
            .checked_duration_since(Instant::now())
            .ok_or("return deadline")?;
        let line = rx.recv_timeout(left)??;
        s.push_str(&line);
        s.push('\n');
        let v: serde_json::Value = serde_json::from_str(&line)?;
        let msg = v["MESSAGE"].as_str().ok_or("non-text journal message")?;
        if v["_TRANSPORT"].as_str() == Some("kernel") && msg == "PM: suspend exit" {
            exit = true;
        }
        if v["UNIT"].as_str() == Some("systemd-suspend.service")
            && (msg.contains("Finished System Suspend.")
                || msg.contains("Finished systemd-suspend.service - System Suspend."))
        {
            finished = true;
        }
        if exit && finished {
            return cycle(&s, boot);
        }
        if msg.contains("Failed to suspend system")
            || msg.contains("Failed to start System Suspend")
        {
            return Err("suspend service failed".into());
        }
    })();
    (s, r)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn record(message: &str, source: u64, receipt: u64) -> String {
        serde_json::json!({"_BOOT_ID":"boot","_TRANSPORT":"kernel","MESSAGE":message,
            "_SOURCE_MONOTONIC_TIMESTAMP":source.to_string(),"_SOURCE_BOOTTIME_TIMESTAMP":source.to_string(),
            "__MONOTONIC_TIMESTAMP":receipt.to_string(),"__REALTIME_TIMESTAMP":(1_700_000_000_000_000u64+receipt).to_string()}).to_string()
    }
    fn records() -> Vec<String> {
        vec![record("PM: suspend entry (s2idle)",90,90),record("PM: suspend-to-idle",100,100),
        record("Timekeeping suspended for 960.000 seconds",950_000_000,150),record("PM: resume from suspend-to-idle",960_000_100,200),
        record("PM: suspend exit",960_000_200,250),
        serde_json::json!({"_BOOT_ID":"boot","_SYSTEMD_UNIT":"systemd-suspend.service","MESSAGE":"System returned from sleep operation 'suspend'."}).to_string(),
        serde_json::json!({"_BOOT_ID":"boot","UNIT":"systemd-suspend.service","MESSAGE":"Finished System Suspend."}).to_string()]
    }
    #[test]
    fn source_is_never_receipt_and_timekeeping_is_allowed() {
        let c = cycle(&records().join("\n"), "boot").unwrap();
        assert_eq!(c.end.source_mono - c.start.source_mono, 960_000_000);
        assert_eq!(c.end.receipt_mono - c.start.receipt_mono, 100);
    }
    #[test]
    fn missing_source_is_not_silently_substituted() {
        let mut r = records();
        let mut v: serde_json::Value = serde_json::from_str(&r[1]).unwrap();
        v.as_object_mut()
            .unwrap()
            .remove("_SOURCE_MONOTONIC_TIMESTAMP");
        r[1] = v.to_string();
        assert!(cycle(&r.join("\n"), "boot").is_err());
    }
    #[test]
    fn rejects_wrong_boot_deep_duplicate_and_fake_service() {
        let original = records().join("\n");
        for bad in [
            original.replace("s2idle)", "deep)"),
            original.replace("\"boot\"", "\"other\""),
            format!("{original}\n{}", record("PM: suspend entry (s2idle)", 1, 1)),
            original.replace("\"UNIT\"", "\"FAKE_UNIT\""),
            original.replace(
                "PM: resume from suspend-to-idle",
                "Low-level resume complete",
            ),
        ] {
            assert!(cycle(&bad, "boot").is_err());
        }
    }
    #[test]
    fn waits_for_late_kernel_exit() {
        let (mut r, (tx, rx)) = (records(), mpsc::channel());
        let exit = r.remove(4);
        for l in r {
            tx.send(Ok(l)).unwrap();
        }
        tx.send(Ok(exit)).unwrap();
        assert!(receive(&rx, "boot").1.is_ok());
    }
}
