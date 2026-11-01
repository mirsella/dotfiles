use std::{fs, path::Path};
#[allow(dead_code)]
mod model {
    include!("reference/rtc-model.rs");
    pub fn final_marker() {
        let observed = Rtc {
            year: 2021,
            month: 4,
            day: 10,
            hour: 4,
            minute: 11,
            second: 29,
        };
        assert_eq!(candidates(observed, 86400), vec![(17, 1, 15089)]);
        assert_eq!(raw_decode(observed), (1, 775, 160));
    }
}
fn main() {
    let argument = std::env::args()
        .nth(1)
        .expect("evidence directory required");
    let root = Path::new(&argument);
    let artifacts = root.join("originals/predator-pm-s2idle");
    let read = |suffix: &str| {
        fs::read_to_string(artifacts.join(format!("s2idle-20261031-{suffix}.txt"))).unwrap()
    };
    assert_eq!(read("request-ok").trim(), "true");
    let raw = read("raw-return");
    for expected in [
        "rtc_date=2021-04-10",
        "rtc_time=00:00:01",
        "last_action=17",
        "last_value=1",
        "last_start=N",
        "markers=26",
        "status: 8257",
    ] {
        assert!(raw.contains(expected), "missing {expected}");
    }
    let ret = read("return");
    assert!(ret.contains("operation=Ok(0.000121"));
    assert!(ret.contains("reader_joined=true"));
    for suffix in ["outcome", "restored", "completed"] {
        assert!(!artifacts
            .join(format!("s2idle-20261031-{suffix}.txt"))
            .exists());
    }
    let times = fs::read_to_string(root.join("kernel-event-times.tsv")).unwrap();
    let events: Vec<_> = times
        .lines()
        .map(|l| {
            let mut f = l.splitn(3, '\t');
            (
                f.next().unwrap().parse::<u64>().unwrap(),
                f.next().unwrap().parse::<u64>().unwrap(),
                f.next().unwrap(),
            )
        })
        .collect();
    let one = |name: &str| {
        let v: Vec<_> = events.iter().filter(|e| e.2 == name).collect();
        assert_eq!(v.len(), 1);
        *v[0]
    };
    let entry = one("PM: suspend entry (s2idle)");
    let start = one("PM: suspend-to-idle");
    let end = one("PM: resume from suspend-to-idle");
    let exit = one("PM: suspend exit");
    assert!(entry.0 < start.0 && start.0 < end.0 && end.0 < exit.0);
    assert_eq!(end.0 - start.0, 14_558_172_999);
    assert_eq!(end.1 - start.1, 121);
    let details = fs::read_to_string(root.join("details.txt")).unwrap();
    assert!(details.contains("System returned from sleep operation 'suspend'."));
    let logind = fs::read_to_string(root.join("logind-wake.txt")).unwrap();
    assert!(
        logind.contains("Lid opened.")
            && logind.contains("Power key pressed short.")
            && logind.contains("Finished System Power Off.")
    );
    let boot = fs::read_to_string(
        root.join("originals/predator-pm-study/f9b45aa6-b820-4ab5-b18c-9324d8d53087-boot-rtc.txt"),
    )
    .unwrap();
    assert!(boot.contains("PM: RTC time: 04:11:29, date: 2021-04-10"));
    assert!(boot.contains("Magic number: 1:775:160"));
    model::final_marker();
    println!("classification=S2IDLE_RETURN_ON_LOCAL_EVENT_THEN_POWEROFF_RESTORATION_INCOMPLETE");
    println!("request_ok=true\nretained_final_marker=17/1/N\nmarkers=26\nsource_kernel_loop_seconds=14558.172999\njournal_receipt_loop_seconds=0.000121\nreceipt_timestamp_duration_invalid=true");
    println!("observed_wake=IRQ9_ACPI_notify_with_lid_open_and_short_power_key\nscheduled_WoL_did_not_establish_return=true\npoweroff_confirmed=true\nrestore_interrupted_by_shutdown=true");
    println!("recovery_RTC_conditional_candidate=console_resume_all_end_state1_plus15089seconds\nRTC_assumptions=devicehash0_continuous_tick_no_intervening_rewrite\nphysical_sleep_seconds=unmeasured_no_dated_graph\nfull_pass=false");
}
