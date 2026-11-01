//! Offline, independent evidence verifier: no machine operation or runner imports.
use serde_json::{Value, json};
use std::{collections::BTreeMap, env, fs, path::Path, process::Command};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const BOOT: &str = "f9b45aa6b8204ab5b18c9324d8d53087";
fn check(b: bool, why: &str) -> Result<()> {
    if b { Ok(()) } else { Err(why.into()) }
}
fn read(p: impl AsRef<Path>) -> Result<String> {
    Ok(fs::read_to_string(p)?)
}
fn sha(p: &Path) -> Result<String> {
    let o = Command::new("sha256sum").arg(p).output()?;
    check(o.status.success(), "sha256sum failed")?;
    Ok(String::from_utf8(o.stdout)?
        .split_whitespace()
        .next()
        .ok_or("missing hash")?
        .into())
}
fn field<'a>(s: &'a str, k: &str) -> Result<&'a str> {
    s.lines()
        .find_map(|l| l.strip_prefix(&format!("{k}=")))
        .ok_or_else(|| format!("missing {k}").into())
}
fn rows(s: &str) -> Result<Vec<Value>> {
    s.lines().map(|l| Ok(serde_json::from_str(l)?)).collect()
}
fn number(v: &Value, k: &str) -> Result<u64> {
    Ok(v[k]
        .as_str()
        .ok_or_else(|| format!("missing {k}"))?
        .parse()?)
}
fn one<'a>(r: &'a [Value], msg: &str) -> Result<&'a Value> {
    let v: Vec<_> = r.iter().filter(|v| v["MESSAGE"] == msg).collect();
    check(v.len() == 1, msg)?;
    Ok(v[0])
}
fn counters(s: &str) -> Result<BTreeMap<String, Vec<u64>>> {
    let mut m = BTreeMap::new();
    let mut instances = BTreeMap::<String, u64>::new();
    for l in s
        .split("wakeup_sources:\n")
        .nth(1)
        .ok_or("no wakeup table")?
        .lines()
        .skip(1)
    {
        let mut f = l.split_whitespace();
        let name = f.next().ok_or("empty source")?;
        let i = instances.entry(name.into()).or_default();
        let key = format!("{name}#{i}");
        *i += 1;
        m.insert(
            key,
            f.map(|n| Ok(n.parse()?)).collect::<Result<Vec<u64>>>()?,
        );
    }
    Ok(m)
}
fn main() -> Result<()> {
    let arg = env::args()
        .nth(1)
        .ok_or("review directory argument required")?;
    let d = Path::new(&arg);
    let orig = d.join("originals/predator-pm-wake-validation");
    let get = |suffix: &str| read(orig.join(format!("wake-20261031-{suffix}.txt")));
    let collection = read(d.join("operator-collection/primary-result.out"))?;
    let metadata = read(d.join("final-state.txt"))?;
    let mut verified = 0;
    for e in fs::read_dir(&orig)? {
        let p = e?.path();
        let n = p
            .file_name()
            .ok_or("filename")?
            .to_str()
            .ok_or("filename UTF8")?;
        check(
            collection.contains(&format!(
                "{}  /var/lib/predator-pm-wake-validation/{n}",
                sha(&p)?
            )),
            "original hash mismatch",
        )?;
        check(
            metadata.contains(&format!(
                "/var/lib/predator-pm-wake-validation/{n} root 600"
            )),
            "original owner/mode mismatch",
        )?;
        verified += 1;
    }
    check(verified == 20, "unexpected artifact count")?;
    check(
        sha(&orig.join("wake-20261031-plan.conf"))?
            == "767cd2aff3f22cc1b8691ae63e3435d83279f7952b763870a53e0b680214b942",
        "manifest changed",
    )?;
    let before = rows(&get("before-kernel")?)?;
    let after = rows(&get("after-kernel")?)?;
    let observed = rows(&get("observed-journal")?)?;
    for r in [&before, &after, &observed] {
        for v in r {
            check(v["_BOOT_ID"] == BOOT, "wrong boot")?;
        }
    }
    check(
        !before.iter().any(|v| {
            v["MESSAGE"]
                .as_str()
                .unwrap_or("")
                .contains("PM: suspend entry (")
        }),
        "prior cycle",
    )?;
    let phases = [
        "PM: suspend entry (s2idle)",
        "PM: suspend-to-idle",
        "PM: resume from suspend-to-idle",
        "PM: suspend exit",
    ];
    let mut times = Vec::new();
    let mut last = 0;
    for msg in phases {
        let v = one(&observed, msg)?;
        check(v["_TRANSPORT"] == "kernel", "non-kernel phase")?;
        let a = one(&after, msg)?;
        let mut t = serde_json::Map::new();
        t.insert("message".into(), json!(msg));
        for k in [
            "_SOURCE_MONOTONIC_TIMESTAMP",
            "_SOURCE_BOOTTIME_TIMESTAMP",
            "__MONOTONIC_TIMESTAMP",
            "__REALTIME_TIMESTAMP",
        ] {
            let n = number(v, k)?;
            check(number(a, k)? == n, "observed/after phase mismatch")?;
            t.insert(k.into(), json!(n));
        }
        let source = number(v, "_SOURCE_MONOTONIC_TIMESTAMP")?;
        check(source > last, "kernel phase order")?;
        last = source;
        times.push(Value::Object(t));
    }
    for v in &after {
        let msg = v["MESSAGE"].as_str().ok_or("non-text kernel message")?;
        for bad in [
            "PM: suspend entry (deep)",
            "Low-level resume complete",
            "PM: suspend debug:",
            "BUG:",
            "Oops:",
            "Call Trace:",
        ] {
            check(!msg.contains(bad), bad)?;
        }
    }
    let returned = one(&observed, "System returned from sleep operation 'suspend'.")?;
    let finished = one(&observed, "Finished System Suspend.")?;
    check(
        returned["_SYSTEMD_UNIT"] == "systemd-suspend.service"
            && finished["UNIT"] == "systemd-suspend.service",
        "service identity",
    )?;
    check(
        number(returned, "__REALTIME_TIMESTAMP")? < number(finished, "__REALTIME_TIMESTAMP")?,
        "service return order",
    )?;
    check(get("request-ok")?.trim() == "true", "request failed")?;
    let outcome = get("outcome")?;
    check(
        outcome.contains("operation=Ok(Cycle")
            && outcome.contains("evidence=Ok(())")
            && outcome.contains("cleanup=Ok(())"),
        "unsuccessful outcome",
    )?;
    let alarm = get("alarm-armed")?;
    let raw = get("raw-return")?;
    let completed = get("completed")?;
    for s in [&alarm, &raw] {
        check(
            field(s, "epoch")? == field(s, "rtc_epoch")?,
            "pre-correction RTC mismatch",
        )?;
        check(field(s, "markers")? == "0", "recorder markers")?;
        check(s.contains("status: 8193,"), "unexpected clock status")?;
    }
    check(
        field(&alarm, "alarm")? == "1793482500" && alarm.contains("alarm_IRQ\t: yes"),
        "alarm not armed",
    )?;
    check(
        field(&raw, "alarm")?.is_empty() && raw.contains("alarm_IRQ\t: no"),
        "alarm not expired",
    )?;
    check(
        field(&raw, "epoch")? == "1793482504" && field(&raw, "wake_irq")? == "9",
        "return epoch/IRQ",
    )?;
    one(&observed, "PM: Triggering wakeup from IRQ 9")?;
    one(&observed, "ACPI: PM: ACPI fixed event wakeup")?;
    check(
        field(&completed, "result")? == "RTC_FALLBACK_WINDOW_RETURN",
        "wrong class",
    )?;
    let start = one(&observed, phases[1])?;
    let end = one(&observed, phases[2])?;
    let mut intervals = serde_json::Map::new();
    for (k, summary) in [
        ("_SOURCE_MONOTONIC_TIMESTAMP", "source_loop_mono_us"),
        ("_SOURCE_BOOTTIME_TIMESTAMP", "source_loop_boot_us"),
        ("__MONOTONIC_TIMESTAMP", "receipt_loop_mono_us"),
        ("__REALTIME_TIMESTAMP", "receipt_loop_real_us"),
    ] {
        let delta = number(end, k)?
            .checked_sub(number(start, k)?)
            .ok_or("backwards clock")?;
        check(
            delta == field(&completed, summary)?.parse::<u64>()?,
            "computed interval mismatch",
        )?;
        intervals.insert(k.into(), json!(delta));
    }
    let b = counters(&alarm)?;
    let a = counters(&raw)?;
    let mut changes = Vec::new();
    check(b.len() == a.len(), "source table changed size")?;
    for (name, values) in &a {
        let previous = b.get(name).ok_or("new wake source")?;
        if previous != values {
            changes.push(json!({"source":name,"before":previous,"after":values}));
        }
    }
    let pi_b = fs::read(d.join("frozen-pi-baseline.log"))?;
    let pi_r = fs::read(d.join("frozen-pi-result.log"))?;
    check(
        sha(&d.join("frozen-pi-baseline.log"))?
            == "fd60ed790026c2f21c602a04069d5710d4185c0c26d114b0e800642a732ce363",
        "Pi baseline hash",
    )?;
    check(
        sha(&d.join("frozen-pi-result.log"))?
            == "c3d0833a9917ee17c95e6d228e6c0c2ae0569eb0d3a2ea8939d4fc770a1c7226",
        "Pi result hash",
    )?;
    check(pi_r.starts_with(&pi_b), "frozen baseline not preserved")?;
    check(
        String::from_utf8(pi_b.clone())?
            .contains("WAKE READY id=wake-20261031 nonce=wake-20261031-1793481300"),
        "baseline nonce",
    )?;
    let suffix = String::from_utf8(pi_r[pi_b.len()..].to_vec())?;
    check(
        suffix.matches("WAKE BEGIN id=wake-20261031").count() == 1
            && suffix.matches("WAKE RETURN id=wake-20261031").count() == 1,
        "Pi independent trial markers",
    )?;
    let pi = read(d.join("operator-collection/observer-result.out"))?;
    for t in [
        "22:31:00.620782",
        "22:31:30.653041",
        "22:36:30.807734",
        "22:38:00.583988",
    ] {
        check(pi.contains(t), "missing Pi actual event")?;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"verification":"PASS","original_artifacts_verified":verified,"original_owner_mode":"root:0600; extracted copies owned by local user","kernel_phases":times,"interval_microseconds_separate_domains":intervals,"wakeup_source_changes":changes,"rtc_before_helper":{"armed_epoch":1793481310u64,"alarm_epoch":1793482500u64,"return_epoch":1793482504u64,"rtc_matches_realtime":true,"STA_UNSYNC":false},"result":"RTC_FALLBACK_WINDOW_RETURN","wake_attribution":"strongly supported RTC alarm via ACPI fixed event/IRQ9; exact fixed-event status bit not captured at wake","Pi_baseline_is_result_prefix":true,"physical_sleep_seconds":null,"full_pass":false})
        )?
    );
    Ok(())
}
