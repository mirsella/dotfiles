use std::{fs, path::Path, process::Command};

fn sha(path: &Path, expected: &str) -> Result<(), Box<dyn std::error::Error>> {
    let out = Command::new("sha256sum").arg(path).output()?;
    assert!(
        out.status.success(),
        "sha256sum {}: {}",
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
    let root = Path::new(".scratch/pm-instrumented-execution-20261031");
    let trial = root.join("rtc-s3-20261031");
    let review = root.join("s3-review-1021");
    let baseline_path = trial.join("baseline.out");
    let result_path = review.join("frozen-pi-result.log");
    sha(
        &baseline_path,
        "d9acaf48a67f432635116320aa1dae036f03edf218e788605fcee8508a246b40",
    )?;
    sha(
        &result_path,
        "794c318ad424206e7b6fadd56df0a24ca601c4bd8857b4250e4df5926400cb57",
    )?;
    sha(
        &trial.join("plan.conf"),
        "aa5be693e84beca952edddf18a7594169c3b08e3885bf61caaf4ca68d1ba5fa9",
    )?;
    let baseline = fs::read(&baseline_path)?;
    let result = fs::read(&result_path)?;
    assert!(
        result.starts_with(&baseline),
        "frozen result lost baseline prefix"
    );
    let delta = std::str::from_utf8(&result[baseline.len()..])?;
    assert!(delta.contains("RTC READY id=rtc-s3-20261031 nonce=rtc-s3-20261031-1793436900"));
    let begin = "RTC BEGIN id=rtc-s3-20261031 boot=f8eb6e44-85ae-4e56-bc00-587876699945 kind=s3 success_before=10 markers_before=56 nonce=rtc-s3-20261031-1793436900 planned_wake=1793437860 age_since_original_boot=55709 duration_source=external-power-graph";
    assert_eq!(delta.matches(begin).count(), 1);
    let tail = &delta[delta.find(begin).unwrap()..];
    assert_eq!(tail.matches("PM: suspend entry (deep)").count(), 1);
    assert!(tail.contains("[55708.931939] PM: suspend entry (deep)"));
    assert!(tail.contains("Freezing remaining freezable tasks completed (elapsed 0.001 seconds)"));
    for absent in [
        "RTC RETURN",
        "Low-level resume complete",
        "PM: suspend exit",
        "Waiting for 5 second(s).",
        "Timekeeping suspended for",
    ] {
        assert!(
            !tail.contains(absent),
            "unexpected current-trial capture: {absent}"
        );
    }
    assert!(tail.trim_end().ends_with("[55710.269890] r8169 0000:03:00.1: PM: calling pci_pm_suspend @ 384989, parent: 0000:00:1c.3"));
    let collected = fs::read_to_string(trial.join("observer-result.out"))?;
    let frozen = std::str::from_utf8(&result)?;
    assert!(
        collected.contains(frozen),
        "collector and separately retrieved frozen bytes differ"
    );
    let journal = collected.split_once(frozen).unwrap().1;
    assert_eq!(
        journal
            .matches("WOL sent to 98:29:a6:3a:a7:50 via 192.168.1.255:9")
            .count(),
        2
    );
    for expected in [
        "2026-10-31T10:11:00.650997+0100",
        "2026-10-31T10:11:30.644462+0100",
        "2026-10-31T10:12:30.552599+0100",
        "probe.service: Main process exited, code=exited, status=1/FAILURE",
        "2026-10-31T10:14:00.563874+0100",
        "snapshot.service: Deactivated successfully.",
        "0 timers listed.",
    ] {
        assert!(
            journal.contains(expected),
            "missing Pi observation: {expected}"
        );
    }
    assert!(fs::read(trial.join("primary-result.out"))?.is_empty());
    assert!(fs::read_to_string(trial.join("primary-result.stderr"))?.contains("No route to host"));
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
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(review.join("trial-netconsole-delta.log"))?;
    std::io::Write::write_all(&mut file, delta.as_bytes())?;
    file.sync_all()?;
    println!("classification=UNREACHABLE_AFTER_S3_ENTRY_AND_SCHEDULED_WOL\nindependent_pi_evidence_verified=true\nnew_real_s3_requests_observed=1\nphysical_s3_entry=unverified\nphysical_sleep_seconds=unknown\nreturn_not_observed_in_capture=true\nprimary_artifacts=unretrieved_not_proven_absent\ncurrent_counter_and_marker_values=unknown\ncurrent_primary_lifecycle_and_health=unknown\nfull_pass=false\nnext_step=owner_watts_graph_and_manual_recovery_then_early_RTC_preservation");
    Ok(())
}
