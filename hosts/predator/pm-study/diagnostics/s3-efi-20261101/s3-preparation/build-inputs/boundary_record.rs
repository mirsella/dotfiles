//! Pure verification for normal-clock EFI boundary records and journal domains.
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub fn require(ok: bool, why: impl Into<String>) -> Result<()> {
    if ok { Ok(()) } else { Err(why.into().into()) }
}
pub fn fields(s: &str) -> Result<BTreeMap<String, String>> {
    let mut m = BTreeMap::new();
    for l in s.lines() {
        let (k, v) = l.split_once('=').ok_or("invalid field")?;
        require(
            !k.is_empty() && !v.is_empty() && m.insert(k.into(), v.into()).is_none(),
            "duplicate/empty field",
        )?;
    }
    Ok(m)
}
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for b in data {
        c ^= *b as u32;
        for _ in 0..8 {
            c = (c >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(c & 1));
        }
    }
    !c
}
pub fn unhex(s: &str) -> Result<Vec<u8>> {
    require(s.is_ascii() && s.len().is_multiple_of(2), "invalid hex")?;
    (0..s.len())
        .step_by(2)
        .map(|i| Ok(u8::from_str_radix(&s[i..i + 2], 16)?))
        .collect()
}
pub fn expected(nonce: &str, boot: &str, phase: u32, sequence: u32, flags: u32) -> Result<Vec<u8>> {
    require(
        (8..=15).contains(&nonce.len())
            && nonce
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
        "invalid nonce",
    )?;
    require(
        boot.len() == 36 && [8, 13, 18, 23].iter().all(|i| boot.as_bytes()[*i] == b'-'),
        "invalid UUID",
    )?;
    let uuid = unhex(&boot.replace('-', ""))?;
    require(uuid.len() == 16, "UUID size")?;
    let mut d = vec![0; 64];
    d[..8].copy_from_slice(b"PMS3BND2");
    d[8..10].copy_from_slice(&2u16.to_le_bytes());
    d[10..12].copy_from_slice(&64u16.to_le_bytes());
    d[12..16].copy_from_slice(&phase.to_le_bytes());
    d[16..20].copy_from_slice(&sequence.to_le_bytes());
    d[20..24].copy_from_slice(&flags.to_le_bytes());
    d[24..40].copy_from_slice(&uuid);
    d[40..40 + nonce.len()].copy_from_slice(nonce.as_bytes());
    let crc = crc32(&d[..60]);
    d[60..].copy_from_slice(&crc.to_le_bytes());
    Ok(d)
}
pub fn verify_record(
    s: &str,
    nonce: &str,
    boot: &str,
    phase: u32,
    sequence: u32,
    flags: u32,
) -> Result<()> {
    let hex = s
        .trim()
        .strip_prefix("attributes=7 bytes=64\nhex=")
        .ok_or("record framing/attributes")?;
    require(
        unhex(hex)? == expected(nonce, boot, phase, sequence, flags)?,
        "record identity, phase, context or checksum mismatch",
    )
}
pub fn verify_stage(
    status: &str,
    record: &str,
    mode: &str,
    nonce: &str,
    boot: &str,
) -> Result<Value> {
    let m = fields(status)?;
    let limit = match mode {
        "core" => 2u64,
        "s3" => 3,
        _ => return Err("mode".into()),
    };
    let phase = if mode == "core" { 11 } else { 22 };
    let get =
        |k: &str| -> Result<&str> { Ok(m.get(k).ok_or_else(|| format!("missing {k}"))?.as_str()) };
    for (k, v) in [
        ("schema", "efi-boundary-stage-1"),
        ("mode", mode),
        ("nonce", nonce),
        ("boot", boot),
        ("armed", "0"),
        ("error", "0"),
        ("capacity_query", "0"),
        ("reserve", "5120"),
        ("margin", "1024"),
        ("per_write_budget", "256"),
        ("efi", "0,0,0"),
    ] {
        require(get(k)? == v, format!("invalid {k}"))?;
    }
    let n = |k| -> Result<u64> { Ok(get(k)?.parse()?) };
    require(
        n("limit")? == limit
            && n("attempted")? == limit
            && n("completed")? == limit
            && n("last_phase")? == phase,
        "incomplete/out-of-order recorder",
    )?;
    require(
        get("results")? == if limit == 2 { "0,0,-115" } else { "0,0,0" },
        "record write failed",
    )?;
    let before = n("before_free")?;
    let after = n("after_free")?;
    require(
        before >= 6144 + 256 * limit
            && after >= 6144
            && after <= before
            && before - after <= 256 * limit
            && n("before_total")? == n("after_total")?
            && n("after_max")? >= 96,
        "EFI capacity invariant",
    )?;
    verify_record(record, nonce, boot, phase as u32, limit as u32, 7)?;
    Ok(
        json!({"recording":"PASS","mode":mode,"writes":limit,"lastPhase":phase,"beforeFree":before,"afterFree":after,"netStorageDecrease":before-after,"resetPersistenceOfThisRecord":"not yet tested"}),
    )
}
pub fn cycle(s: &str, boot: &str, mode: &str) -> Result<Value> {
    let mut entry = Vec::new();
    let mut exit = Vec::new();
    let mut low = 0;
    let mut checkpoint = 0;
    let mut events = Vec::new();
    let compact = boot.replace('-', "");
    for line in s.lines() {
        let v: Value = serde_json::from_str(line)?;
        require(v["_BOOT_ID"] == compact, "journal boot changed")?;
        if v["_TRANSPORT"] != "kernel" {
            continue;
        }
        let m = v["MESSAGE"].as_str().ok_or("non-text kernel message")?;
        require(
            !["BUG:", "Oops:", "Call Trace:", "PM: suspend-to-idle"]
                .iter()
                .any(|x| m.contains(x)),
            "kernel error/wrong sleep mode",
        )?;
        let selected = m.contains("PM: suspend entry (")
            || m.contains("PM: suspend exit")
            || m.contains("Low-level resume complete")
            || m.contains("PM: suspend debug:")
            || m.contains("pm_boundary_stage:");
        if !selected {
            continue;
        }
        if m.contains("PM: suspend entry (") {
            require(m.contains("(deep)"), "wrong mode")?;
            entry.push(v.clone());
        }
        if m.contains("PM: suspend exit") {
            exit.push(v.clone());
        }
        if m.contains("Low-level resume complete") {
            low += 1;
        }
        if m.contains("PM: suspend debug:") {
            checkpoint += 1;
        }
        let mut stamps = BTreeMap::new();
        for k in [
            "_SOURCE_MONOTONIC_TIMESTAMP",
            "_SOURCE_BOOTTIME_TIMESTAMP",
            "__MONOTONIC_TIMESTAMP",
            "__REALTIME_TIMESTAMP",
        ] {
            let n = v[k]
                .as_str()
                .ok_or_else(|| format!("missing {k}"))?
                .parse::<u64>()?;
            stamps.insert(k, n);
        }
        events.push(json!({"message":m,"timestampsMicros":stamps}));
    }
    require(entry.len() == 1 && exit.len() == 1, "expected one cycle")?;
    require(
        if mode == "core" {
            low == 0 && checkpoint == 1
        } else {
            low == 1 && checkpoint == 0
        },
        "wrong diagnostic/real S3 evidence",
    )?;
    let mut intervals = BTreeMap::new();
    for k in [
        "_SOURCE_MONOTONIC_TIMESTAMP",
        "_SOURCE_BOOTTIME_TIMESTAMP",
        "__MONOTONIC_TIMESTAMP",
        "__REALTIME_TIMESTAMP",
    ] {
        let a = entry[0][k]
            .as_str()
            .ok_or("entry timestamp")?
            .parse::<u64>()?;
        let b = exit[0][k]
            .as_str()
            .ok_or("exit timestamp")?
            .parse::<u64>()?;
        intervals.insert(k, b.checked_sub(a).ok_or("backwards interval")?);
    }
    Ok(
        json!({"cycle":"PASS","mode":mode,"entryExitIntervalsMicros":intervals,"events":events,"physicalSleepSeconds":null}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    const B: &str = "39d7579b-b86d-4d30-b910-131943e550c8";
    fn read(n: &str, p: u32, s: u32, f: u32) -> String {
        format!(
            "attributes=7 bytes=64\nhex={}\n",
            expected(n, B, p, s, f)
                .unwrap()
                .iter()
                .map(|x| format!("{x:02x}"))
                .collect::<String>()
        )
    }
    #[test]
    fn crc_and_record_integrity() {
        assert_eq!(crc32(b"123456789"), 0xcbf43926);
        let r = read("core1101a", 11, 2, 7);
        verify_record(&r, "core1101a", B, 11, 2, 7).unwrap();
        for i in 0..r.len() - 1 {
            assert!(verify_record(&r[..i], "core1101a", B, 11, 2, 7).is_err());
        }
        for r in [
            read("core1101a", 10, 2, 7),
            read("core1101a", 11, 1, 7),
            read("core1101a", 11, 2, 3),
            read("deep1101a", 11, 2, 7),
            r.replace("attributes=7", "attributes=6"),
        ] {
            assert!(verify_record(&r, "core1101a", B, 11, 2, 7).is_err());
        }
    }
    fn status() -> String {
        format!(
            "schema=efi-boundary-stage-1\nmode=core\nnonce=core1101a\nboot={B}\narmed=0\nlimit=2\nattempted=2\ncompleted=2\nlast_phase=11\nerror=0\nresults=0,0,-115\nefi=0,0,0\ncapacity_query=0\nreserve=5120\nmargin=1024\nper_write_budget=256\nbefore_free=7342\nafter_free=7030\nbefore_total=163752\nafter_total=163752\nafter_max=6970\n"
        )
    }
    #[test]
    fn bounded_storage_and_complete_sequence() {
        let s = status();
        let r = read("core1101a", 11, 2, 7);
        verify_stage(&s, &r, "core", "core1101a", B).unwrap();
        for (a, b) in [
            ("completed=2", "completed=1"),
            ("error=0", "error=-16"),
            ("after_free=7030", "after_free=6143"),
            ("after_free=7030", "after_free=6800"),
            ("after_free=7030", "after_free=7343"),
            ("results=0,0,-115", "results=0,-5,-115"),
            ("armed=0", "armed=1"),
            ("after_total=163752", "after_total=163753"),
        ] {
            assert!(verify_stage(&s.replace(a, b), &r, "core", "core1101a", B).is_err());
        }
    }
    fn journal(messages: &[&str]) -> String {
        messages.iter().enumerate().map(|(i,m)| json!({"_BOOT_ID":B.replace('-',""),"_TRANSPORT":"kernel","MESSAGE":m,"_SOURCE_MONOTONIC_TIMESTAMP":(100+i*100).to_string(),"_SOURCE_BOOTTIME_TIMESTAMP":(100+i*100).to_string(),"__MONOTONIC_TIMESTAMP":(100+i).to_string(),"__REALTIME_TIMESTAMP":(1000+i).to_string()}).to_string()+"\n").collect()
    }
    #[test]
    fn keeps_four_domains_and_distinguishes_control() {
        let s = journal(&[
            "PM: suspend entry (deep)",
            "PM: suspend debug: Waiting for 5 second(s).",
            "PM: suspend exit",
        ]);
        let v = cycle(&s, B, "core").unwrap();
        assert_eq!(
            v["entryExitIntervalsMicros"]["_SOURCE_MONOTONIC_TIMESTAMP"],
            200
        );
        assert_eq!(v["entryExitIntervalsMicros"]["__MONOTONIC_TIMESTAMP"], 2);
        assert!(cycle(&s, B, "s3").is_err());
        assert!(
            cycle(
                &s.replace("_SOURCE_BOOTTIME_TIMESTAMP", "missing"),
                B,
                "core"
            )
            .is_err()
        );
        let real = journal(&[
            "PM: suspend entry (deep)",
            "ACPI: PM: Low-level resume complete",
            "PM: suspend exit",
        ]);
        cycle(&real, B, "s3").unwrap();
        assert!(cycle(&real, B, "core").is_err());
    }
}
