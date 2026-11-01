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
        .take_while(|l| *l != "interrupts:")
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
    let orig = d.join("originals");
    let id = "path-d3-v2-20261101";
    let get = |suffix: &str| read(orig.join(format!("{id}-{suffix}.txt")));
    let collection = read(d.join("operator-collection/primary-result.out"))?;
    let metadata = read(d.join("final-state.out"))?;
    let mut verified = 0;
    for e in fs::read_dir(&orig)? {
        let p = e?.path();
        let n = p
            .file_name()
            .ok_or("filename")?
            .to_str()
            .ok_or("filename UTF8")?;
        check(
            collection.contains(&format!("{}  /var/lib/predator-pm-wake-path/{n}", sha(&p)?)),
            "original hash mismatch",
        )?;
        check(
            metadata.contains(&format!("/var/lib/predator-pm-wake-path/{n} root 600")),
            "original owner/mode mismatch",
        )?;
        verified += 1;
    }
    check(verified == 24, "unexpected artifact count")?;
    check(
        sha(&orig.join(format!("{id}-plan.conf")))?
            == "ab8a52f58f39a2dddb7363a0b8d3290a346856c6d9d2dbf9991124be522ea653",
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
    let phases = [
        "PM: suspend entry (s2idle)",
        "PM: suspend-to-idle",
        "PM: resume from suspend-to-idle",
        "PM: suspend exit",
    ];
    let mut times = Vec::new();
    let mut last = 0;
    let historical = [6790650591u64, 6842844799, 7979482880, 7983055184];
    for (index, msg) in phases.into_iter().enumerate() {
        let old = one(&before, msg)?;
        check(old["_TRANSPORT"] == "kernel", "non-kernel historical phase")?;
        for key in ["_SOURCE_MONOTONIC_TIMESTAMP", "_SOURCE_BOOTTIME_TIMESTAMP"] {
            check(
                number(old, key)? == historical[index],
                "historical phase changed",
            )?;
        }
        let v = one(&observed, msg)?;
        check(v["_TRANSPORT"] == "kernel", "non-kernel phase")?;
        let matching: Vec<_> = after.iter().filter(|a| a["MESSAGE"] == msg).collect();
        check(
            matching.len() == 2,
            "expected exactly two cumulative cycles",
        )?;
        check(
            matching[0]["__CURSOR"] == old["__CURSOR"],
            "old cursor changed",
        )?;
        let a = matching[1];
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
    for old in &before {
        let a: Vec<_> = after
            .iter()
            .filter(|v| v["__CURSOR"] == old["__CURSOR"])
            .collect();
        check(
            a.len() == 1 && a[0] == old,
            "existing kernel record changed/disappeared",
        )?;
    }
    check(
        before
            .iter()
            .filter(|v| {
                v["MESSAGE"]
                    .as_str()
                    .unwrap_or("")
                    .starts_with("WAKE BEGIN id=wake-20261031 ")
            })
            .count()
            == 1,
        "old BEGIN",
    )?;
    check(before.iter().filter(|v| v["MESSAGE"] == "WAKE RETURN id=wake-20261031 result=RTC_FALLBACK_WINDOW_RETURN full_pass=false").count() == 1, "old RETURN")?;
    check(
        !before.iter().any(|v| {
            v["MESSAGE"]
                .as_str()
                .unwrap_or("")
                .contains("WAKEPATH BEGIN")
        }),
        "prior new protocol attempt",
    )?;
    check(
        after
            .iter()
            .filter(|v| {
                v["MESSAGE"]
                    .as_str()
                    .unwrap_or("")
                    .starts_with(&format!("WAKEPATH BEGIN id={id} "))
            })
            .count()
            == 1,
        "new BEGIN",
    )?;
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
        field(&alarm, "alarm")? == "1793522100" && alarm.contains("alarm_IRQ\t: yes"),
        "alarm not armed",
    )?;
    check(
        field(&raw, "alarm")?.is_empty() && raw.contains("alarm_IRQ\t: no"),
        "alarm not expired",
    )?;
    check(
        field(&raw, "epoch")? == "1793522104" && field(&raw, "wake_irq")? == "9",
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
    check(
        field(&completed, "success_before")? == "1"
            && field(&completed, "success_after")? == "2"
            && field(&completed, "markers_after")? == "0",
        "cumulative counters",
    )?;
    let state_messages = [
        "r8169 0000:03:00.1: PME# enabled",
        "pcieport 0000:00:1c.3: Wakeup enabled by ACPI",
        "r8169 0000:03:00.1: PCI PM: Suspend power state: D3hot",
        "pcieport 0000:00:1c.3: PCI PM: Suspend power state: D0",
    ];
    let mut states = Vec::new();
    for msg in state_messages {
        let v = one(&observed, msg)?;
        let t = number(v, "_SOURCE_MONOTONIC_TIMESTAMP")?;
        check(
            v["_TRANSPORT"] == "kernel"
                && t > number(one(&observed, phases[0])?, "_SOURCE_MONOTONIC_TIMESTAMP")?
                && t < number(start, "_SOURCE_MONOTONIC_TIMESTAMP")?,
            "PCI/ACPI arming outside entry",
        )?;
        states.push(json!({"message":msg,"source_mono_us":t}));
    }
    check(
        get("measurement")?.contains("verified_D0_bridge_D3hot_NIC_PME_and_ACPI_arming=Ok(())"),
        "measurement failure",
    )?;
    let restored = get("restored")?;
    check(
        field(&restored, "nic_d3cold_allowed")? == "1"
            && restored.matches(" =_ ").count() == 6
            && field(&restored, "mem_sleep")? == "s2idle [deep]"
            && field(&restored, "btime_observed")? == "btime 1793474520",
        "restoration evidence",
    )?;
    check(
        field(&raw, "PCI 0000:03:00.1 d3cold_allowed")? == "0",
        "intervention absent before cleanup",
    )?;
    check(
        field(&alarm, "PCI 0000:03:00.1 d3cold_allowed")? == "0"
            && alarm.matches(" =p ").count() == 6,
        "intervention/debug not applied",
    )?;
    let mut acpi = Vec::new();
    for key in [
        "ff_rt_clk",
        "ff_pwr_btn",
        "ff_slp_btn",
        "gpe14",
        "gpe66",
        "sci",
        "sci_not",
        "gpe_all",
        "error",
    ] {
        let k = format!("ACPI {key}");
        let b: u64 = field(&alarm, &k)?
            .split_whitespace()
            .next()
            .ok_or("empty ACPI")?
            .parse()?;
        let a: u64 = field(&raw, &k)?
            .split_whitespace()
            .next()
            .ok_or("empty ACPI")?
            .parse()?;
        acpi.push(json!({"counter":key,"before":b,"after":a,"delta":a.checked_sub(b).ok_or("backwards ACPI")?}));
    }
    let mut root_status = Vec::new();
    for (label, text) in [("armed", &alarm), ("raw_return", &raw)] {
        let hex = field(text, "PCI 0000:00:1c.3 config256")?;
        check(hex.len() == 512, "PCI config length")?;
        let bytes: Vec<u8> = (0..512)
            .step_by(2)
            .map(|i| Ok(u8::from_str_radix(&hex[i..i + 2], 16)?))
            .collect::<Result<_>>()?;
        check(
            bytes[0x34] == 0x40 && bytes[0x40] == 0x10 && (bytes[0x42] >> 4) == 4,
            "not expected PCIe root-port capability",
        )?;
        let status = u32::from_le_bytes(bytes[0x60..0x64].try_into()?);
        let ctl = u16::from_le_bytes(bytes[0x5c..0x5e].try_into()?);
        root_status.push(json!({"snapshot":label,"root_status_hex":format!("{status:#010x}"),"requester_id_hex":format!("{:#06x}",status & 0xffff),"PME_status":status & 0x10000 != 0,"PME_pending":status & 0x20000 != 0,"native_PME_interrupt_enable":ctl & 8 != 0}));
    }
    let pi_bp = d.join(format!("pi-originals/{id}-baseline.log"));
    let pi_rp = d.join(format!("pi-originals/{id}-result.log"));
    let pi_b = fs::read(&pi_bp)?;
    let pi_r = fs::read(&pi_rp)?;
    check(
        sha(&pi_bp)? == "7a048b004674d029f7db85b196fedd4df07821bdeaab81d2863044143c0c72c2",
        "Pi baseline hash",
    )?;
    check(
        sha(&pi_rp)? == "1f9fd1adc4703040d09ec15f05561ac14e8174de9c8443d521609926a51825b8",
        "Pi result hash",
    )?;
    check(pi_r.starts_with(&pi_b), "frozen baseline not preserved")?;
    check(
        String::from_utf8(pi_b.clone())?
            .contains(&format!("WAKEPATH READY id={id} nonce={id}-1793520900")),
        "baseline nonce",
    )?;
    let suffix = String::from_utf8(pi_r[pi_b.len()..].to_vec())?;
    check(
        suffix.matches(&format!("WAKEPATH BEGIN id={id}")).count() == 1
            && suffix.matches(&format!("WAKEPATH RETURN id={id}")).count() == 1,
        "Pi independent trial markers",
    )?;
    let pi = read(d.join("operator-collection/observer-result.out"))?;
    for t in [
        "09:31:00.650861",
        "09:31:30.641785",
        "09:36:30.807938",
        "09:38:00.571598",
    ] {
        check(pi.contains(t), "missing Pi actual event")?;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"verification":"PASS","original_artifacts_verified":verified,"original_owner_mode":"root:0600; extracted copies owned by local user","kernel_phases":times,"interval_microseconds_separate_domains":intervals,"wakeup_source_changes":changes,"ACPI_counters":acpi,"PCI_ACPI_entry_evidence":states,"root_port_status":root_status,"root_port_interpretation":"PME status/pending already set before entry; requester changes 0201 to NIC 0301 after return. No event timing or sleeping-packet receipt proven.","rtc_before_helper":{"armed_epoch":1793520911u64,"alarm_epoch":1793522100u64,"return_epoch":1793522104u64,"rtc_matches_realtime":true,"STA_UNSYNC":false},"result":"RTC_FALLBACK_WINDOW_RETURN","wake_attribution":"strongly supported RTC alarm via ACPI fixed event/IRQ9 and RTC fixed-event counter change; exact fixed-event status bit not captured at wake","Pi_baseline_is_result_prefix":true,"cumulative_success_before":1,"cumulative_success_after":2,"new_cycles":1,"old_kernel_records_preserved":before.len(),"physical_sleep_seconds":null,"owner_reported_watts":6,"full_pass":false})
        )?
    );
    Ok(())
}
