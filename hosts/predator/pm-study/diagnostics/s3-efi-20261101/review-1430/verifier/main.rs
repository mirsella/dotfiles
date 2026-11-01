use serde_json::{Value, json};
use std::{collections::BTreeMap, env, fs, path::Path, process::Command};
type R<T> = Result<T, Box<dyn std::error::Error>>;
const BOOT: &str = "39d7579bb86d4d30b910131943e550c8";
fn read(p: impl AsRef<Path>) -> R<String> {
    Ok(fs::read_to_string(p)?)
}
fn hash(p: &Path) -> R<String> {
    let v = Command::new("sha256sum").arg(p).output()?;
    assert!(v.status.success());
    Ok(String::from_utf8(v.stdout)?
        .split_whitespace()
        .next()
        .unwrap()
        .into())
}
fn jsonl(s: &str) -> R<Vec<Value>> {
    s.lines()
        .map(|l| {
            let v: Value = serde_json::from_str(l)?;
            assert_eq!(v["_BOOT_ID"], BOOT);
            Ok(v)
        })
        .collect()
}
fn msg(v: &Value) -> &str {
    v["MESSAGE"].as_str().unwrap()
}
fn crc(b: &[u8]) -> u32 {
    let mut c = 0xffffffffu32;
    for x in b {
        c ^= *x as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                (c >> 1) ^ 0xedb88320
            } else {
                c >> 1
            };
        }
    }
    !c
}
fn sources(s: &str) -> BTreeMap<String, Vec<u64>> {
    let mut out = BTreeMap::new();
    let mut seen = BTreeMap::new();
    let block = s
        .split("name\t")
        .nth(1)
        .unwrap()
        .split("/proc/interrupts:")
        .next()
        .unwrap();
    for line in block.lines().skip(1) {
        let f: Vec<_> = line.split_whitespace().collect();
        if f.len() != 10 {
            break;
        }
        let count = seen.entry(f[0]).or_insert(0);
        let key = format!("{}#{}", f[0], count);
        *count += 1;
        out.insert(key, f[1..].iter().map(|x| x.parse().unwrap()).collect());
    }
    out
}
fn main() -> R<()> {
    let root = std::path::PathBuf::from(env::args().nth(1).ok_or("review directory")?);
    let orig = root.join("originals/predator-pm-efi-s3-reviewed");
    let prefix = "s3-efi-v2-20261101-";
    let collection = read(root.join("collection.out"))?;
    let mut files = 0;
    for entry in fs::read_dir(&orig)? {
        let p = entry?.path();
        let n = p.file_name().unwrap().to_str().unwrap();
        if !n.starts_with(prefix) {
            continue;
        }
        assert!(p.is_file());
        let remote = format!("/var/lib/predator-pm-efi-s3-reviewed/{n}");
        let sha = hash(&p)?;
        assert!(collection.lines().any(|l| l == format!("{sha}  {remote}")));
        assert!(
            collection
                .lines()
                .any(|l| l.starts_with("root 600 ") && l.ends_with(&remote))
        );
        files += 1;
    }
    assert_eq!(files, 21);
    assert_eq!(
        hash(&orig.join("core-control-review.json"))?,
        "78a64967499872a4cd565d4c4788402b55432fb418c884b9c297d6a93af31896"
    );
    assert!(
        read(root.join("final-state.out"))?.contains(
            "root 600 2945 /var/lib/predator-pm-efi-s3-reviewed/core-control-review.json"
        )
    );
    assert_eq!(
        hash(&orig.join(format!("{prefix}plan.conf")))?,
        "bb535457f04bf08d1663900ff454339b1521f68e81de6cf1f98b111f2183bd5c"
    );
    let artifact = |s: &str| read(orig.join(format!("{prefix}{s}")));
    let before = jsonl(&artifact("before-kernel.jsonl")?)?;
    let after = jsonl(&artifact("after-kernel.jsonl")?)?;
    let observed = jsonl(&artifact("observed.jsonl")?)?;
    for needle in [
        "PM: suspend entry (deep)",
        "PM: suspend exit",
        "PM: suspend debug:",
        "BOUNDARY BEGIN id=core-efi-20261101",
    ] {
        assert_eq!(
            before.iter().filter(|v| msg(v).contains(needle)).count(),
            1,
            "prior {needle}"
        );
    }
    assert!(
        !before
            .iter()
            .any(|v| msg(v).contains("Low-level resume complete")
                || msg(v).contains("BOUNDARY BEGIN id=s3-"))
    );
    let core = jsonl(&read(root.parent().unwrap().join(
        "core-review/originals/predator-pm-efi-s3/core-efi-20261101-observed.jsonl",
    ))?)?;
    let before_cursors: BTreeMap<_, _> = before
        .iter()
        .map(|v| (v["__CURSOR"].as_str().unwrap(), v))
        .collect();
    for v in core.iter().filter(|v| v["_TRANSPORT"] == "kernel") {
        assert_eq!(
            *before_cursors.get(v["__CURSOR"].as_str().unwrap()).unwrap(),
            v
        );
    }
    let indexed: BTreeMap<_, _> = after
        .iter()
        .map(|v| (v["__CURSOR"].as_str().unwrap(), v))
        .collect();
    for v in &before {
        assert_eq!(*indexed.get(v["__CURSOR"].as_str().unwrap()).unwrap(), v);
    }
    let kern: Vec<_> = observed
        .iter()
        .filter(|v| v["_TRANSPORT"] == "kernel")
        .collect();
    for bad in [
        "PM: suspend debug:",
        "PM: suspend-to-idle",
        "BUG:",
        "Oops:",
        "Call Trace:",
    ] {
        assert!(!kern.iter().any(|v| msg(v).contains(bad)));
    }
    let one = |needle: &str| -> &Value {
        let found: Vec<_> = kern.iter().filter(|v| msg(v).contains(needle)).collect();
        assert_eq!(found.len(), 1, "{needle}");
        found[0]
    };
    let entry = one("PM: suspend entry (deep)");
    let exit = one("PM: suspend exit");
    let selected = [
        entry,
        one("pm_boundary_stage: begin nonce=deep1101a phase=20 sequence=1 context=7"),
        one("pm_boundary_stage: end nonce=deep1101a phase=20 sequence=1 error=0 efi=0"),
        one("ACPI: PM: Low-level resume complete"),
        one("pm_boundary_stage: begin nonce=deep1101a phase=21 sequence=2 context=7"),
        one("pm_boundary_stage: end nonce=deep1101a phase=21 sequence=2 error=0 efi=0"),
        one("pm_boundary_stage: begin nonce=deep1101a phase=22 sequence=3 context=7"),
        one("pm_boundary_stage: end nonce=deep1101a phase=22 sequence=3 error=0 efi=0"),
        exit,
    ];
    let keys = [
        "_SOURCE_MONOTONIC_TIMESTAMP",
        "_SOURCE_BOOTTIME_TIMESTAMP",
        "__MONOTONIC_TIMESTAMP",
        "__REALTIME_TIMESTAMP",
    ];
    let mut stamps = Vec::new();
    for v in selected {
        let mut t = BTreeMap::new();
        for k in keys {
            t.insert(k, v[k].as_str().unwrap().parse::<u64>()?);
        }
        assert_eq!(*indexed.get(v["__CURSOR"].as_str().unwrap()).unwrap(), v);
        stamps.push(json!({"message":msg(v),"microseconds":t}));
    }
    for k in keys {
        let ns: Vec<u64> = selected
            .iter()
            .map(|v| v[k].as_str().unwrap().parse().unwrap())
            .collect();
        assert!(ns.windows(2).all(|w| w[1] >= w[0]));
    }
    assert!(
        observed
            .iter()
            .any(|v| v["_SYSTEMD_UNIT"] == "systemd-suspend.service"
                && msg(v).contains("System returned from sleep operation"))
    );
    assert!(
        observed
            .iter()
            .any(|v| v["UNIT"] == "systemd-suspend.service"
                && msg(v).contains("Finished System Suspend"))
    );
    assert_eq!(
        after
            .iter()
            .filter(|v| msg(v).contains("PM: suspend entry (deep)"))
            .count(),
        2
    );
    assert_eq!(artifact("request-ok.txt")?.lines().next(), Some("true"));
    let outcome = artifact("outcome.txt")?;
    for s in ["operation=Ok(())", "evidence=Ok(())", "cleanup=Ok(())"] {
        assert!(outcome.contains(s));
    }
    let status = artifact("stage-status.txt")?;
    let mut m = BTreeMap::new();
    for l in status.lines() {
        let (k, v) = l.split_once('=').unwrap();
        assert!(m.insert(k, v).is_none());
    }
    for (k, v) in [
        ("schema", "efi-boundary-stage-1"),
        ("mode", "s3"),
        ("nonce", "deep1101a"),
        ("boot", "39d7579b-b86d-4d30-b910-131943e550c8"),
        ("armed", "0"),
        ("limit", "3"),
        ("attempted", "3"),
        ("completed", "3"),
        ("last_phase", "22"),
        ("error", "0"),
        ("results", "0,0,0"),
        ("efi", "0,0,0"),
        ("capacity_query", "0"),
        ("reserve", "5120"),
        ("margin", "1024"),
        ("per_write_budget", "256"),
    ] {
        assert_eq!(m[k], v, "{k}");
    }
    let n = |k| m[k].parse::<u64>().unwrap();
    let cost = n("before_free") - n("after_free");
    assert!(
        n("before_free") >= 6912 && n("after_free") >= 6144 && cost <= 768 && n("after_max") >= 94
    );
    assert_eq!(n("before_total"), n("after_total"));
    let r = artifact("stage-readback.txt")?;
    let h = r
        .trim()
        .strip_prefix("attributes=7 bytes=64\nhex=")
        .unwrap();
    assert_eq!(h.len(), 128);
    let b: Vec<u8> = (0..128)
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
        .collect();
    assert_eq!(&b[..12], b"PMS3BND2\x02\0\x40\0");
    for (i, v) in [(12, 22u32), (16, 3), (20, 7)] {
        assert_eq!(u32::from_le_bytes(b[i..i + 4].try_into()?), v);
    }
    assert_eq!(
        b[24..40]
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect::<String>(),
        BOOT
    );
    assert_eq!(&b[40..49], b"deep1101a");
    assert!(b[49..60].iter().all(|b| *b == 0));
    assert_eq!(u32::from_le_bytes(b[60..64].try_into()?), crc(&b[..60]));
    let complete: Value = serde_json::from_str(&artifact("completed.json")?)?;
    assert_eq!(complete["result"], "S3_LOWLEVEL_RETURN");
    assert_eq!(complete["boot"], "39d7579b-b86d-4d30-b910-131943e550c8");
    assert_eq!(complete["id"], "s3-efi-v2-20261101");
    assert_eq!(complete["successBefore"], 1);
    assert_eq!(complete["successAfter"], 2);
    assert_eq!(complete["rtcTarget"], 1793539500u64);
    assert_eq!(complete["restoration"], "PASS");
    let runtime_check: Value = serde_json::from_str(&artifact("verification.json")?)?;
    let mut intervals = BTreeMap::new();
    for k in keys {
        let delta = exit[k].as_str().unwrap().parse::<u64>()?
            - entry[k].as_str().unwrap().parse::<u64>()?;
        assert_eq!(runtime_check["cycle"]["entryExitIntervalsMicros"][k], delta);
        intervals.insert(k, delta);
    }
    let restored = artifact("restored.txt")?;
    assert!(restored.contains("[none]"));
    assert!(collection.contains("s2idle [deep]"));
    assert!(restored.contains("/sys/power/pm_async:\n1\n"));
    let raw = artifact("raw-return.txt")?;
    let realtime: u64 = raw
        .lines()
        .find_map(|l| l.strip_prefix("epoch="))
        .unwrap()
        .parse()?;
    let rtc: u64 = raw
        .lines()
        .find_map(|l| l.strip_prefix("rtc_epoch="))
        .unwrap()
        .parse()?;
    assert!(rtc.abs_diff(realtime) <= 2);
    assert!(raw.lines().any(|l| l == "alarm="));
    let timex: i32 = raw
        .split("status: ")
        .nth(1)
        .unwrap()
        .split(',')
        .next()
        .unwrap()
        .parse()?;
    assert_eq!(timex & 0x40, 0);
    assert_eq!(realtime, 1793539504);
    assert_eq!(rtc, 1793539505);
    assert!(raw.contains("alarm_IRQ\t: no"));
    let armed = artifact("alarm-armed.txt")?;
    for s in [
        "epoch=1793539210\n",
        "rtc_epoch=1793539210\n",
        "alarm=1793539500\n",
        "alarm_IRQ\t: yes",
    ] {
        assert!(armed.contains(s));
    }
    assert_eq!(artifact("alarm-intent.txt")?.trim(), "1793539500");
    let before_sources = sources(&armed);
    let after_sources = sources(&raw);
    assert_eq!(
        before_sources.keys().collect::<Vec<_>>(),
        after_sources.keys().collect::<Vec<_>>()
    );
    let changes: Vec<_> = before_sources
        .iter()
        .filter(|(name, v)| after_sources[*name] != **v)
        .map(|(name, v)| json!({"source":name,"before":v,"after":after_sources[name]}))
        .collect();
    let counters = |s: &str| -> BTreeMap<String, u64> {
        s.lines()
            .filter_map(|l| l.strip_prefix("acpi_"))
            .map(|l| {
                let (k, v) = l.split_once('=').unwrap();
                (
                    k.into(),
                    v.split_whitespace().next().unwrap().parse().unwrap(),
                )
            })
            .collect()
    };
    let ca = counters(&armed);
    let cb = counters(&raw);
    let counter_changes:Vec<_>=ca.iter().map(|(k,a)|json!({"counter":k,"armed":a,"rawReturn":cb[k],"delta":cb[k].checked_sub(*a).unwrap()})).collect();
    let pci = |s: &str| -> Vec<u8> {
        let h = s
            .lines()
            .find_map(|l| l.strip_prefix("pci_0000:00:1c.3="))
            .unwrap();
        assert_eq!(h.len(), 512);
        (0..h.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
            .collect()
    };
    let pa = pci(&armed);
    let pb = pci(&raw);
    let rs = |b: &[u8]| u32::from_le_bytes(b[0x60..0x64].try_into().unwrap());
    assert_eq!(pa[0x40], 0x10);
    assert_eq!(pb[0x40], 0x10);
    assert!(raw.contains("wake_irq_read_error=/sys/power/pm_wakeup_irq: No data available"));
    let baseline = root.join("pi-originals/s3-efi-v2-20261101-baseline.log");
    let frozen = root.join("pi-originals/s3-efi-v2-20261101-result.log");
    assert_eq!(
        hash(&baseline)?,
        "1c6b69b7ca5f33d7a31d400ac42acaf365403dcbbf103952de1999e0679588e5"
    );
    assert_eq!(
        hash(&frozen)?,
        "ca27a5afad8035b7a0f38eeac245393d3fe665c126df8e0401970eb3d58402ef"
    );
    let base_bytes = fs::read(baseline)?;
    let frozen_bytes = fs::read(frozen)?;
    assert!(frozen_bytes.starts_with(&base_bytes));
    let suffix = std::str::from_utf8(&frozen_bytes[base_bytes.len()..])?;
    assert_eq!(
        suffix
            .matches("BOUNDARY BEGIN id=s3-efi-v2-20261101")
            .count(),
        1
    );
    assert_eq!(
        suffix
            .matches("BOUNDARY RETURN id=s3-efi-v2-20261101 result=S3_LOWLEVEL_RETURN")
            .count(),
        1
    );
    let observer = read(root.join("observer-collection.out"))?;
    for t in ["14:26:30.866014", "14:26:30.869447", "14:28:00.578920"] {
        assert!(observer.contains(t));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"independentVerification":"PASS","originalHarnessResult":"S3_LOWLEVEL_RETURN","trialOriginals":files,"independentCoreAnchor":"PINNED_AND_PRESERVED","priorKernelRecordsPreserved":before.len(),"mode":"s3","counterBefore":1,"counterAfter":2,"recordPhase":22,"recordSequence":3,"contextFlags":7,"reportedStorageCost":cost,"remainingFree":n("after_free"),"usableAboveLinuxReserve":n("after_free")-5120,"rawEpoch":realtime,"rawRtcEpoch":rtc,"timestamps":stamps,"entryExitIntervalsMicros":intervals,"physicalSleepSeconds":null,"fullPhysicalPass":false,"realS3":true,"piFrozenHashesAndPrefix":"PASS","piProbeAndSnapshot":"PASS","wakeAttribution":"RTC timing, expired alarm and RTC counter support RTC wake; no wake IRQ captured, power-button source counts also rose, exact triggering status bit and owner interaction remain unverified","wakeupSourceChanges":changes,"acpiCounterChanges":counter_changes,"rootPortStatus":{"before":format!("0x{:08x}",rs(&pa)),"after":format!("0x{:08x}",rs(&pb)),"timing":"boundary snapshots only; no in-sleep capture"}})
        )?
    );
    Ok(())
}
