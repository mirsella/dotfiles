use serde_json::{json, Value};
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
fn main() -> R<()> {
    let root = std::path::PathBuf::from(env::args().nth(1).ok_or("review directory")?);
    let orig = root.join("originals/predator-pm-efi-s3");
    let prefix = "core-efi-20261101-";
    let collection = read(root.join("collection.out"))?;
    let mut files = 0;
    for entry in fs::read_dir(&orig)? {
        let p = entry?.path();
        let n = p.file_name().unwrap().to_str().unwrap();
        if !n.starts_with(prefix) {
            continue;
        }
        assert!(p.is_file());
        let remote = format!("/var/lib/predator-pm-efi-s3/{n}");
        let sha = hash(&p)?;
        assert!(collection.lines().any(|l| l == format!("{sha}  {remote}")));
        assert!(collection
            .lines()
            .any(|l| l.starts_with("root 600 ") && l.ends_with(&remote)));
        files += 1;
    }
    assert_eq!(files, 16);
    assert_eq!(
        hash(&orig.join(format!("{prefix}plan.conf")))?,
        "95344780cda5a343d3bbe5c757456aff0408d5ae027587d5da0d4215f7f3e5e3"
    );
    let artifact = |s: &str| read(orig.join(format!("{prefix}{s}")));
    let before = jsonl(&artifact("before-kernel.jsonl")?)?;
    let after = jsonl(&read(root.join("after-review-kernel.jsonl"))?)?;
    let observed = jsonl(&artifact("observed.jsonl")?)?;
    assert!(!before
        .iter()
        .any(|v| msg(v).contains("PM: suspend entry (") || msg(v).contains("BOUNDARY BEGIN")));
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
        "Low-level resume complete",
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
    one("PM: suspend debug:");
    let selected = [
        entry,
        one("pm_boundary_stage: begin nonce=core1101a phase=10 sequence=1 context=7"),
        one("pm_boundary_stage: end nonce=core1101a phase=10 sequence=1 error=0 efi=0"),
        one("pm_boundary_stage: begin nonce=core1101a phase=11 sequence=2 context=7"),
        one("pm_boundary_stage: end nonce=core1101a phase=11 sequence=2 error=0 efi=0"),
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
    assert!(observed
        .iter()
        .any(|v| v["_SYSTEMD_UNIT"] == "systemd-suspend.service"
            && msg(v).contains("System returned from sleep operation")));
    assert!(observed
        .iter()
        .any(|v| v["UNIT"] == "systemd-suspend.service"
            && msg(v).contains("Finished System Suspend")));
    assert_eq!(
        after
            .iter()
            .filter(|v| msg(v).contains("PM: suspend entry (deep)"))
            .count(),
        1
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
        ("mode", "core"),
        ("nonce", "core1101a"),
        ("boot", "39d7579b-b86d-4d30-b910-131943e550c8"),
        ("armed", "0"),
        ("limit", "2"),
        ("attempted", "2"),
        ("completed", "2"),
        ("last_phase", "11"),
        ("error", "0"),
        ("results", "0,0,-115"),
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
        n("before_free") >= 6656 && n("after_free") >= 6144 && cost <= 512 && n("after_max") >= 94
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
    for (i, v) in [(12, 11u32), (16, 2), (20, 7)] {
        assert_eq!(u32::from_le_bytes(b[i..i + 4].try_into()?), v);
    }
    assert_eq!(
        b[24..40]
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect::<String>(),
        BOOT
    );
    assert_eq!(&b[40..49], b"core1101a");
    assert!(b[49..60].iter().all(|b| *b == 0));
    assert_eq!(u32::from_le_bytes(b[60..64].try_into()?), crc(&b[..60]));
    // The deployed harness failed its exact btime check after successful cleanup.
    // Keep that failed run intact; this is a separate review, never a replacement completion.
    for name in ["completed.json", "verification.json", "after-kernel.jsonl"] {
        assert!(!orig.join(format!("{prefix}{name}")).exists());
    }
    assert!(collection.contains("boot/history identity changed"));
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
    let baseline = root.join("pi-originals/core-efi-20261101-baseline.log");
    let frozen = root.join("pi-originals/core-efi-20261101-result.log");
    assert_eq!(
        hash(&baseline)?,
        "5c4218f7de7088a87b9fdc354e1e552d7c33e6313d9209f093542ac8e2055188"
    );
    assert_eq!(
        hash(&frozen)?,
        "1f98c7583596a04a525373f7810cf0eed60b979674cc6ed59447b0400d75f077"
    );
    let base_bytes = fs::read(baseline)?;
    let frozen_bytes = fs::read(frozen)?;
    assert!(frozen_bytes.starts_with(&base_bytes));
    let suffix = std::str::from_utf8(&frozen_bytes[base_bytes.len()..])?;
    assert_eq!(
        suffix
            .matches("BOUNDARY BEGIN id=core-efi-20261101")
            .count(),
        1
    );
    assert!(!suffix.contains("BOUNDARY RETURN id=core-efi-20261101"));
    let observer = read(root.join("observer-collection.out"))?;
    for t in ["13:48:00.981505", "13:48:00.984925", "13:49:00.576856"] {
        assert!(observer.contains(t));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"independentVerification":"PASS","originalHarnessResult":"POST_CLEANUP_EXACT_BTIME_GUARD_FAILURE","originalCompletionAbsent":true,"originals":files,"priorKernelRecordsPreserved":before.len(),"mode":"core","counterBefore":0,"counterAfter":1,"recordPhase":11,"recordSequence":2,"contextFlags":7,"reportedStorageCost":cost,"remainingFree":n("after_free"),"rawEpoch":realtime,"rawRtcEpoch":rtc,"timestamps":stamps,"physicalSleepSeconds":null,"realS3":false,"piFrozenHashesAndPrefix":"PASS","piProbeAndSnapshot":"PASS"})
        )?
    );
    Ok(())
}
