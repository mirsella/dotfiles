// Read-only recovery interpretation using the unchanged, tested RTC model.
#[allow(dead_code)]
mod model {
    include!("rtc-model.rs");

    pub fn review(artifact: &str) {
        assert!(artifact.starts_with("boot=ae29f675-9167-4975-a884-01885468bc38\n"));
        let raw: Vec<_> = artifact
            .lines()
            .filter(|l| l.contains("PM: RTC time:"))
            .collect();
        assert_eq!(raw.len(), 1);
        assert!(raw[0].contains("PM: RTC time: 00:59:40, date: 2099-07-02"));
        let observed = Rtc {
            year: 2099,
            month: 7,
            day: 2,
            hour: 0,
            minute: 59,
            second: 40,
        };
        assert_eq!(raw_decode(observed), (11, 598, 960));
        assert!(artifact.contains("Magic number: 11:598:960"));
        assert_eq!(candidates(observed, 7200), vec![(10, 11, 3580)]);
        assert_eq!(candidates(observed, 86400), vec![(10, 11, 3580)]);
        assert_eq!(file_hash(10), 118);
        assert_eq!(ACTIONS[9], "acpi_suspend");
        assert_eq!(
            encode(11, 118, 0),
            Rtc {
                year: 2099,
                month: 7,
                day: 2,
                hour: 0,
                minute: 0,
                second: 0
            }
        );
        println!("conditional_phase=acpi_suspend_start\naction=10\nvalue=3\nstart=true\nencoded_rtc=2099-07-02T00:00:00Z\nearly_observed_rtc=2099-07-02T00:59:40Z\nrtc_elapsed_seconds=3580\ncandidate_count_within_2h=1\ncandidate_count_within_24h=1\nnaive_magic=11:598:960\nassumptions=device_hash_0,continuous_rtc,no_intervening_firmware_or_clock_rewrite\nphysical_sleep_duration=unknown\nlow_level_return_marker=not_retained\nculprit=not_identified\nfull_pass=false");
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("boot RTC artifact required");
    model::review(&std::fs::read_to_string(path).expect("read original RTC artifact"));
}
