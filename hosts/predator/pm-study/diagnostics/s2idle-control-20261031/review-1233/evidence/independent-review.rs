use std::{fs, io::Write, path::Path, process::Command};

fn sha(path: &Path, expected: &str) -> Result<(), Box<dyn std::error::Error>> {
    let out = Command::new("sha256sum").arg(path).output()?;
    assert!(
        out.status.success(),
        "hash {}: {}",
        path.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout)?.split_whitespace().next(),
        Some(expected)
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(".scratch/pm-s2idle-control-20261031");
    let trial = root.join("s2idle-20261031");
    let review = root.join("review-1233");
    sha(
        &trial.join("baseline.out"),
        "a767d259d642d52e1dbebc881fd7bbf58e11f6adee27f2a7ff39f7f2d8d6cfa2",
    )?;
    sha(
        &review.join("frozen-pi-result.log"),
        "c41b683160c76853729a1de6bd461c703adecd3512abeb05b7eec9a056e5262e",
    )?;
    sha(
        &trial.join("plan.conf"),
        "f22e464bb3aae8404d25ec05f7b4ae3002ba0c757a9d5e69f1199195f13a5820",
    )?;
    let baseline = fs::read(trial.join("baseline.out"))?;
    let result = fs::read(review.join("frozen-pi-result.log"))?;
    assert_eq!(baseline.len(), 2882530);
    assert_eq!(result.len(), 2912202);
    assert!(
        result.starts_with(&baseline),
        "frozen baseline is not intact"
    );
    let delta = std::str::from_utf8(&result[baseline.len()..])?;
    assert!(delta.contains("S2IDLE READY id=s2idle-20261031 nonce=s2idle-20261031-1793444820"));
    let begin = "S2IDLE BEGIN id=s2idle-20261031 boot=ae29f675-9167-4975-a884-01885468bc38 kind=s2idle-control success_before=0 markers_before=0 nonce=s2idle-20261031-1793444820 planned_wake=1793445780 age_since_recovery_boot=4346 duration_source=external-power-graph";
    assert_eq!(delta.matches(begin).count(), 1);
    assert!(delta.contains("[ 4345.572857] S2IDLE BEGIN"));
    let tail = &delta[delta.find(begin).unwrap()..];
    assert_eq!(tail.matches("PM: suspend entry (s2idle)").count(), 1);
    let entry = tail
        .find("[ 4345.785231] PM: suspend entry (s2idle)")
        .unwrap();
    let freeze = tail
        .find("[ 4345.853031] Freezing remaining freezable tasks completed (elapsed 0.001 seconds)")
        .unwrap();
    assert!(entry < freeze);
    for absent in [
        "S2IDLE RETURN",
        "PM: suspend-to-idle",
        "PM: resume from suspend-to-idle",
        "Low-level resume complete",
        "PM: suspend exit",
        "PM: suspend entry (deep)",
        "Waiting for 5 second(s).",
        "Timekeeping suspended for",
    ] {
        assert!(!tail.contains(absent), "unexpected trial capture: {absent}");
    }
    assert!(tail.trim_end().ends_with("[ 4346.622581] r8169 0000:03:00.1: PM: calling pci_pm_suspend @ 33540, parent: 0000:00:1c.3"));
    let collected = fs::read_to_string(trial.join("observer-result.out"))?;
    let frozen = std::str::from_utf8(&result)?;
    assert!(
        collected.contains(frozen),
        "separately retrieved bytes differ from collector"
    );
    let journal = collected.split_once(frozen).unwrap().1;
    assert_eq!(
        journal
            .matches("WOL sent to 98:29:a6:3a:a7:50 via 192.168.1.255:9")
            .count(),
        2
    );
    for expected in [
        "2026-10-31T12:23:00.644935+0100",
        "2026-10-31T12:23:30.654975+0100",
        "2026-10-31T12:24:30.560617+0100",
        "2026-10-31T12:24:33.638776+0100",
        "probe.service: Main process exited, code=exited, status=1/FAILURE",
        "2026-10-31T12:26:00.583634+0100",
        "snapshot.service: Deactivated successfully.",
        "0 timers listed.",
    ] {
        assert!(
            journal.contains(expected),
            "missing Pi evidence: {expected}"
        );
    }
    assert!(fs::read(trial.join("primary-result.out"))?.is_empty());
    assert!(fs::read_to_string(trial.join("primary-result.stderr"))?.contains("No route to host"));
    assert_eq!(
        fs::read_to_string(trial.join("primary-result.status"))?,
        "exit status: 255"
    );
    let pi = fs::read_to_string(review.join("pi-final-proof.txt"))?;
    for expected in [
        "2142644e-84db-40ae-b1b9-9e8c71265cf7",
        "NTPSynchronized=yes",
        "ActiveState=active",
        "SubState=running",
        "No jobs running.",
        "ca4666560dc9cd468f3b9b6c4c68a7dcb2675b561844ca5ef6a89fa9944722e8",
    ] {
        assert!(pi.contains(expected), "missing observer proof: {expected}");
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(review.join("trial-netconsole-delta.log"))?;
    output.write_all(delta.as_bytes())?;
    output.sync_all()?;
    println!("classification=UNREACHABLE_AFTER_S2IDLE_ENTRY_AND_SCHEDULED_WOL\nindependent_pi_evidence_verified=true\nnew_s2idle_requests_observed=1\ns2idle_loop_reached=unverified\nphysical_sleep_seconds=unknown\nreturn_not_observed_in_capture=true\nprimary_artifacts=unretrieved_not_proven_absent\ncurrent_boot_counter_marker_lifecycle_health=unknown\nfull_pass=false\nfirmware_s3_or_driver_culprit=not_established\nnext_step=owner_watts_graph_and_manual_recovery_then_early_RTC_preservation");
    Ok(())
}
