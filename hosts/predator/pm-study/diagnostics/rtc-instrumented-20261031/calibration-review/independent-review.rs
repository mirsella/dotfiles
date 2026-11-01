use std::{fs, path::Path, process::Command};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = Path::new(
        ".scratch/pm-instrumented-execution-20261031/calibration-review-0941/original-artifacts",
    );
    let get = |s: &str| fs::read_to_string(d.join(format!("rtc-cal-core-r3-20261031-{s}.txt")));
    let before = get("before-kernel")?;
    let after = get("after-kernel")?;
    assert!(after.starts_with(before.trim_end()));
    for (text, n) in [(&before, 9), (&after, 10)] {
        assert_eq!(text.matches("PM: suspend entry (deep)").count(), n);
        assert_eq!(
            text.matches("PM: suspend debug: Waiting for 5 second(s).")
                .count(),
            n
        );
        assert_eq!(text.matches("PM: suspend exit").count(), n);
        assert!(!text.contains("Low-level resume complete"));
    }
    let start = after
        .find("RTC BEGIN id=rtc-cal-core-r3-20261031 ")
        .unwrap();
    let tail = &after[start..];
    let entry = tail.find("PM: suspend entry (deep)").unwrap();
    let checkpoint = tail
        .find("PM: suspend debug: Waiting for 5 second(s).")
        .unwrap();
    let exit = tail.find("PM: suspend exit").unwrap();
    assert!(entry < checkpoint && checkpoint < exit);
    assert_eq!(tail.matches("RTC BEGIN ").count(), 1);
    assert!(!tail.contains("Timekeeping suspended for"));
    for forbidden in [
        "BUG:",
        "Oops:",
        "Call Trace:",
        "failed to suspend",
        "Freezing of tasks failed",
    ] {
        assert!(!tail.contains(forbidden), "{forbidden}");
    }
    assert_eq!(get("request-ok")?.trim(), "true");
    assert_eq!(get("outcome")?.trim(), "operation=Ok(())\ncleanup=Ok(())");
    let raw = get("raw-return")?;
    for expected in [
        "rtc_date=2023-04-10\n",
        "rtc_time=00:00:01\n",
        "last_action=17\n",
        "last_value=3\n",
        "last_start=N\n",
        "markers=56\n",
        "status: 8257,",
    ] {
        assert!(raw.contains(expected), "{expected}");
    }
    assert!(get("clock-quiescence")?.contains("zero_offset_cancel=true"));
    assert!(get("clock-handoff")?
        .contains("Initial clock synchronization to Sat 2026-10-31 09:35:17.088762 CET."));
    let restored = get("restored")?;
    assert!(
        restored.contains("status: 8192,")
            && restored.contains("btime_observed=btime 1793381198")
            && restored.contains("rtc=2026-10-31 08:35:18.")
    );
    let completed = get("completed")?;
    for expected in [
        "result=RTC_CORE_CALIBRATION_VERIFIED\n",
        "completed_epoch=1793435719\n",
        "markers_after=56\n",
        "success_after=10\n",
        "real_s3=false\n",
        "physical_sleep_seconds=not-applicable\n",
        "full_pass=false\n",
    ] {
        assert!(completed.contains(expected), "{expected}");
    }
    assert!(get("return")?.contains("reader_joined=true"));
    let hash = Command::new("sha256sum")
        .arg(d.join("rtc-cal-core-r3-20261031-completed.txt"))
        .output()?;
    assert!(hash.status.success());
    assert!(String::from_utf8(hash.stdout)?
        .starts_with("2333c0af7c6fa351a53ad5202189df7b469963590245485cb386ae7f6b6227ed "));
    println!("INDEPENDENT_CORE_CALIBRATION_VERIFIED\noriginal_cycles=9\nnew_diagnostic_cycles=1\nreal_s3_cycles=0\nraw_hardware_marker_verified=true\nclock_restore_verified=true\npost_restore_btime=1793381198\nmarkers=56\ncompletion_epoch=1793435719\ncompletion_sha256=2333c0af7c6fa351a53ad5202189df7b469963590245485cb386ae7f6b6227ed");
    Ok(())
}
