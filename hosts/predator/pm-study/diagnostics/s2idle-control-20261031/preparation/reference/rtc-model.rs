// Offline feasibility model only. No device access or clock writes.
// Matches Linux v6.18.58 drivers/base/power/trace.c and pm_stage.c.
const ACTIONS: [&str; 18] = [
    "sync_filesystems",
    "freeze_processes",
    "dpm_prepare",
    "dpm_suspend",
    "dpm_suspend_late",
    "dpm_suspend_noirq",
    "CPU_OFF",
    "syscore_suspend",
    "machine_suspend",
    "acpi_suspend",
    "syscore_resume",
    "CPU_ON",
    "dpm_resume_noirq",
    "dpm_resume_early",
    "dpm_resume",
    "dpm_complete",
    "console_resume_all",
    "suspend_enter",
];

fn file_hash(line: u32) -> u32 {
    b"predator-pm-stage".iter().fold(line, |s, &c| {
        s.wrapping_mul(65599).wrapping_add(u32::from(c))
    }) % 997
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Rtc {
    year: u32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
}

fn encode(user: u32, file: u32, device: u32) -> Rtc {
    assert!(user < 16 && file < 997 && device < 1009);
    let mut n = user + 16 * (file + 997 * device);
    let year = 2000 + n % 100;
    n /= 100;
    let month = 1 + n % 12;
    n /= 12;
    let day = 1 + n % 28;
    n /= 28;
    let hour = n % 24;
    n /= 24;
    let minute = (n % 20) * 3;
    n /= 20;
    assert_eq!(n, 0);
    Rtc {
        year,
        month,
        day,
        hour,
        minute,
        second: 0,
    }
}

fn raw_decode(r: Rtc) -> (u32, u32, u32) {
    assert!((2000..2100).contains(&r.year) && (1..=12).contains(&r.month));
    assert!((1..=28).contains(&r.day) && r.hour < 24 && r.minute < 60 && r.second < 60);
    let n = r.year % 100
        + (r.month - 1) * 100
        + (r.day - 1) * 1200
        + r.hour * 33600
        + (r.minute / 3) * 806400;
    (n % 16, (n / 16) % 997, n / (16 * 997))
}

fn seconds(r: Rtc) -> i64 {
    assert!((2000..2100).contains(&r.year));
    let leap = |y: u32| y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    let months = [
        31,
        if leap(r.year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    assert!((1..=12).contains(&r.month));
    assert!((1..=months[r.month as usize - 1]).contains(&r.day));
    assert!(r.hour < 24 && r.minute < 60 && r.second < 60);
    let years: u32 = (2000..r.year)
        .map(|y| if leap(y) { 366 } else { 365 })
        .sum();
    let days = years + months[..r.month as usize - 1].iter().sum::<u32>() + r.day - 1;
    i64::from(days) * 86400 + i64::from(r.hour * 3600 + r.minute * 60 + r.second)
}

fn candidates(observed: Rtc, max_elapsed: i64) -> Vec<(usize, u32, i64)> {
    let mut result = Vec::new();
    for line in 1..=18 {
        for user in 0..16 {
            let elapsed = seconds(observed) - seconds(encode(user, file_hash(line), 0));
            if (0..=max_elapsed).contains(&elapsed) {
                result.push((line as usize, user, elapsed));
            }
        }
    }
    result
}

fn main() {
    println!("MODEL ONLY: device hash fixed at 0; RTC continues ticking; no clock writes/reset by firmware; retention and drift not validated on hardware.");
    for (index, action) in ACTIONS.iter().enumerate() {
        let line = index as u32 + 1;
        println!(
            "line={line} action={action} file_hash={} user11={:?} user3={:?}",
            file_hash(line),
            encode(11, file_hash(line), 0),
            encode(3, file_hash(line), 0)
        );
    }
    let mut closest = i64::MAX;
    let mut pair = None;
    let states: Vec<_> = (1..=18)
        .flat_map(|line| (0..16).map(move |user| (line, user)))
        .collect();
    for (i, &(a, u)) in states.iter().enumerate() {
        for &(b, v) in &states[i + 1..] {
            let distance =
                (seconds(encode(u, file_hash(a), 0)) - seconds(encode(v, file_hash(b), 0))).abs();
            if distance < closest {
                closest = distance;
                pair = Some(((a, u), (b, v)));
            }
        }
    }
    println!(
        "minimum encoded-date spacing across all 288 action/user states: {closest}s, pair={pair:?}"
    );
    for observed in [
        Rtc {
            year: 2099,
            month: 7,
            day: 2,
            hour: 1,
            minute: 22,
            second: 25,
        },
        Rtc {
            year: 2099,
            month: 7,
            day: 2,
            hour: 15,
            minute: 5,
            second: 20,
        },
    ] {
        println!(
            "historical observed={observed:?} raw_magic={:?} conditional_candidates_max24h={:?}",
            raw_decode(observed),
            candidates(observed, 86400)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_radices_and_century() {
        assert_eq!(
            encode(0, 0, 0),
            Rtc {
                year: 2000,
                month: 1,
                day: 1,
                hour: 0,
                minute: 0,
                second: 0
            }
        );
        assert_eq!(
            encode(4, 6, 0),
            Rtc {
                year: 2000,
                month: 2,
                day: 1,
                hour: 0,
                minute: 0,
                second: 0
            }
        );
        assert_eq!(
            encode(0, 75, 0),
            Rtc {
                year: 2000,
                month: 1,
                day: 2,
                hour: 0,
                minute: 0,
                second: 0
            }
        );
        assert_eq!(
            encode(0, 106, 2),
            Rtc {
                year: 2000,
                month: 1,
                day: 1,
                hour: 1,
                minute: 0,
                second: 0
            }
        );
        assert_eq!(
            encode(0, 550, 50),
            Rtc {
                year: 2000,
                month: 1,
                day: 1,
                hour: 0,
                minute: 3,
                second: 0
            }
        );
    }

    #[test]
    fn all_hash_triplets_roundtrip() {
        for device in 0..1009 {
            for file in 0..997 {
                for user in 0..16 {
                    assert_eq!(raw_decode(encode(user, file, device)), (user, file, device));
                }
            }
        }
    }

    #[test]
    fn seconds_rollovers_and_leap_year() {
        let a = Rtc {
            year: 2000,
            month: 2,
            day: 28,
            hour: 0,
            minute: 0,
            second: 0,
        };
        assert_eq!(
            seconds(Rtc {
                month: 3,
                day: 1,
                ..a
            }) - seconds(a),
            2 * 86400
        );
        let a = Rtc { year: 2001, ..a };
        assert_eq!(
            seconds(Rtc {
                month: 3,
                day: 1,
                ..a
            }) - seconds(a),
            86400
        );
    }

    #[test]
    fn elapsed_time_changes_naive_magic() {
        let a = encode(11, file_hash(10), 0);
        assert_ne!(
            raw_decode(Rtc {
                minute: a.minute + 3,
                ..a
            }),
            raw_decode(a)
        );
        assert_eq!(
            candidates(
                Rtc {
                    hour: 12,
                    minute: 1,
                    ..a
                },
                86400
            ),
            vec![(10, 11, 43260)]
        );
    }

    #[test]
    fn every_zero_device_state_has_unique_one_day_recovery_window() {
        for line in 1..=18 {
            for user in 0..16 {
                let a = encode(user, file_hash(line), 0);
                assert_eq!((a.hour, a.minute), (0, 0));
                let b = Rtc {
                    day: a.day + 1,
                    ..a
                };
                assert_eq!(candidates(b, 86400), vec![(line as usize, user, 86400)]);
            }
        }
    }
}
