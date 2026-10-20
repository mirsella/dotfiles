use crate::util;
use anyhow::{Context, Result, ensure};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const LIMITS: [(&str, f64); 5] = [
    ("PPT LIMIT FAST", 53.0),
    ("PPT LIMIT SLOW", 35.0),
    ("PPT LIMIT APU", 41.0),
    ("THM LIMIT CORE", 100.0),
    ("STT LIMIT APU", 90.0),
];
const SUSPECT_TIME: Duration = Duration::from_secs(20);
const RETRY_TIME: Duration = Duration::from_secs(15);
const VERIFY_TIME: Duration = Duration::from_secs(20);
const HEALTHY_TIME: Duration = Duration::from_secs(5);
const CYCLE_COOLDOWN: Duration = Duration::from_secs(600);

fn privileged_command(program: &str) -> Command {
    if unsafe { libc::geteuid() } == 0 {
        Command::new(program)
    } else {
        let mut command = Command::new("sudo");
        command.args(["-n", program]);
        command
    }
}

fn output(args: &[&str]) -> Result<String> {
    let output = privileged_command("ryzenadj")
        .args(args)
        .output()
        .context("start RyzenAdj")?;
    // RyzenAdj prints rejected SMU writes to stdout.
    ensure!(
        output.status.success(),
        "RyzenAdj exited {}: {}{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).context("RyzenAdj returned non-UTF-8 output")
}

fn apply() -> Result<String> {
    output(&[
        "--fast-limit=53000",
        "--slow-limit=35000",
        "--apu-slow-limit=41000",
        "--skin-temp-limit=45000",
        "--tctl-temp=100",
        "--apu-skin-temp=90",
        "-i",
    ])
}

fn value(table: &str, label: &str) -> Result<f64> {
    let row = table
        .lines()
        .find(|line| {
            line.split('|')
                .nth(1)
                .is_some_and(|name| name.trim() == label)
        })
        .with_context(|| format!("missing RyzenAdj {label}"))?;
    let value: f64 = row
        .split('|')
        .nth(2)
        .context("invalid RyzenAdj row")?
        .trim()
        .parse()
        .with_context(|| format!("invalid RyzenAdj {label} row: {row}"))?;
    ensure!(
        value.is_finite() && value >= 0.0,
        "invalid RyzenAdj {label}: {value}"
    );
    Ok(value)
}

fn limits_match(table: &str) -> Result<bool> {
    // Parse every monitored value even when an earlier one differs.
    let mut matched = true;
    for (label, expected) in LIMITS {
        matched &= value(table, label)? == expected;
    }
    Ok(matched)
}

fn ac_online() -> Result<bool> {
    let ac = util::read("/sys/class/power_supply/ACAD/online")?;
    ensure!(matches!(ac.trim(), "0" | "1"), "unexpected AC state: {ac}");
    Ok(ac.trim() == "1")
}

fn ac_performance() -> Result<bool> {
    Ok(ac_online()? && util::read("/sys/firmware/acpi/platform_profile")?.trim() == "performance")
}

#[derive(Clone, Copy, Debug)]
struct Sample {
    watts: f64,
    mhz: f64,
    busy: f64,
    core_c: f64,
    skin_c: f64,
    memory_stall: f64,
}

impl Sample {
    fn suspect(self) -> bool {
        // A low wattage alone is normal when idle or waiting for memory.
        self.busy >= 60.0
            && self.watts <= 17.0
            && self.mhz <= 1800.0
            && (10.0..85.0).contains(&self.core_c)
            && (10.0..50.0).contains(&self.skin_c)
            && self.memory_stall < 5.0
    }

    fn recovered(self) -> bool {
        self.busy >= 60.0 && (self.watts >= 20.0 || self.mhz >= 2400.0)
    }
}

fn cpu_ticks(stat: &str) -> Result<(u64, u64)> {
    let mut fields = stat
        .lines()
        .next()
        .context("missing CPU statistics")?
        .split_whitespace();
    ensure!(
        fields.next() == Some("cpu"),
        "missing aggregate CPU statistics"
    );
    // guest and guest_nice are already included in user and nice.
    let mut ticks = [0_u64; 8];
    for tick in &mut ticks {
        *tick = fields
            .next()
            .context("incomplete CPU statistics")?
            .parse()?;
    }
    Ok((ticks.iter().sum(), ticks[3] + ticks[4]))
}

fn memory_stall(pressure: &str) -> Result<f64> {
    let full = pressure
        .lines()
        .find(|line| line.starts_with("full "))
        .context("missing memory full PSI")?;
    let avg: f64 = full
        .split_whitespace()
        .find_map(|field| field.strip_prefix("avg10="))
        .context("missing memory PSI avg10")?
        .parse()?;
    ensure!(
        avg.is_finite() && (0.0..=100.0).contains(&avg),
        "invalid memory PSI: {avg}"
    );
    Ok(avg)
}

struct Monitor {
    clocks: Vec<PathBuf>,
    previous: Option<(u64, u64)>,
}

impl Monitor {
    fn new() -> Result<Self> {
        let mut clocks = Vec::new();
        for entry in fs::read_dir("/sys/devices/system/cpu/cpufreq")? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with("policy") {
                clocks.push(entry.path().join("scaling_cur_freq"));
            }
        }
        ensure!(!clocks.is_empty(), "no CPU frequency policies found");
        Ok(Self {
            clocks,
            previous: None,
        })
    }

    fn sample(&mut self, table: &str) -> Result<Option<Sample>> {
        let current = cpu_ticks(&util::read("/proc/stat")?)?;
        let Some(previous) = self.previous.replace(current) else {
            return Ok(None);
        };
        let total = current
            .0
            .checked_sub(previous.0)
            .context("CPU tick counter went backwards")?;
        let idle = current
            .1
            .checked_sub(previous.1)
            .context("idle tick counter went backwards")?;
        ensure!(total > 0 && idle <= total, "invalid CPU tick interval");
        let mut khz = 0_u64;
        for clock in &self.clocks {
            let value: u64 = util::read(clock)?.trim().parse()?;
            ensure!(value > 0, "zero CPU frequency at {}", clock.display());
            khz += value;
        }
        Ok(Some(Sample {
            watts: value(table, "PPT VALUE FAST")?,
            mhz: khz as f64 / self.clocks.len() as f64 / 1000.0,
            busy: 100.0 * (total - idle) as f64 / total as f64,
            core_c: value(table, "THM VALUE CORE")?,
            skin_c: value(table, "STT VALUE APU")?,
            memory_stall: memory_stall(&util::read("/proc/pressure/memory")?)?,
        }))
    }
}

#[derive(Debug, PartialEq)]
enum Action {
    Reapply,
    Cycle,
    Verified,
    Failed,
    Unconfirmed,
}

#[derive(Default)]
enum Recovery {
    #[default]
    Watching,
    Suspect(Duration),
    Retried(Duration),
    Verifying {
        since: Duration,
        healthy_since: Option<Duration>,
    },
    Disabled,
}

impl Recovery {
    fn settings_applied(&mut self) {
        match self {
            Self::Verifying { healthy_since, .. } => *healthy_since = None,
            Self::Disabled => {}
            _ => *self = Self::Watching,
        }
    }

    fn observe(&mut self, now: Duration, sample: Sample, cycle_allowed: bool) -> Option<Action> {
        match self {
            Self::Disabled => None,
            Self::Verifying {
                since,
                healthy_since,
            } => {
                if sample.recovered() {
                    if now - *healthy_since.get_or_insert(now) >= HEALTHY_TIME {
                        *self = Self::Watching;
                        return Some(Action::Verified);
                    }
                } else {
                    *healthy_since = None;
                }
                if now - *since >= VERIFY_TIME {
                    *self = Self::Disabled;
                    return Some(if sample.suspect() {
                        Action::Failed
                    } else {
                        Action::Unconfirmed
                    });
                }
                None
            }
            _ if !sample.suspect() || !cycle_allowed => {
                *self = Self::Watching;
                None
            }
            Self::Watching => {
                *self = Self::Suspect(now);
                None
            }
            Self::Suspect(since) if now - *since >= SUSPECT_TIME => {
                *self = Self::Retried(now);
                Some(Action::Reapply)
            }
            Self::Retried(since) if now - *since >= RETRY_TIME => {
                *self = Self::Verifying {
                    since: now,
                    healthy_since: None,
                };
                Some(Action::Cycle)
            }
            Self::Suspect(_) | Self::Retried(_) => None,
        }
    }

    fn execute(&mut self, action: Action, cooldown: &mut Cooldown) {
        match action {
            Action::Reapply => {
                if let Err(error) = apply() {
                    eprintln!(
                        "RyzenAdj retry failed: {error:#}; waiting for telemetry before profile recovery"
                    );
                }
            }
            Action::Cycle => {
                if let Err(error) = recover_profile(cooldown) {
                    *self = Self::Disabled;
                    eprintln!(
                        "Profile recovery failed: {error:#}; disabling further cycles for this AC performance session"
                    );
                } else {
                    println!(
                        "Cycled balanced -> performance and reapplied limits; checking recovery under load"
                    );
                }
            }
            Action::Verified => {
                println!("Power or clocks recovered under sustained load after the profile cycle")
            }
            Action::Failed | Action::Unconfirmed => eprintln!(
                "Profile recovery did not produce a verified improvement; disabling further cycles for this AC performance session"
            ),
        }
    }
}

struct Cooldown {
    path: PathBuf,
    ready_at: Instant,
}

impl Cooldown {
    fn load(directory: &Path, now: SystemTime, monotonic: Instant) -> Result<Self> {
        fs::create_dir_all(directory)?;
        let path = directory.join("last-profile-cycle");
        let remaining = match fs::read_to_string(&path) {
            Ok(text) => {
                let last = UNIX_EPOCH
                    .checked_add(Duration::from_secs(text.trim().parse()?))
                    .context("invalid profile recovery timestamp")?;
                let elapsed = now
                    .duration_since(last)
                    .context("profile recovery timestamp is in the future")?;
                CYCLE_COOLDOWN.saturating_sub(elapsed)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Duration::ZERO,
            Err(error) => return Err(error).context("read profile recovery cooldown"),
        };
        Ok(Self {
            path,
            ready_at: monotonic + remaining,
        })
    }

    fn record(&mut self) -> Result<()> {
        self.ready_at = Instant::now() + CYCLE_COOLDOWN;
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        // Persist before changing profiles so a service restart cannot bypass the cooldown.
        util::atomic_write(&self.path, format!("{timestamp}\n").as_bytes())
    }
}

fn state_directory() -> Result<PathBuf> {
    let path = if let Some(path) = env::var_os("STATE_DIRECTORY") {
        PathBuf::from(path)
    } else if let Some(path) = env::var_os("XDG_STATE_HOME") {
        PathBuf::from(path).join("ryzenadj-laptop")
    } else {
        PathBuf::from(env::var_os("HOME").context("HOME is unset")?)
            .join(".local/state/ryzenadj-laptop")
    };
    ensure!(
        path.is_absolute(),
        "RyzenAdj state directory must be absolute"
    );
    Ok(path)
}

fn cycle_profile(
    mut still_selected: impl FnMut(&str) -> Result<bool>,
    mut set: impl FnMut(&str) -> Result<()>,
) -> Result<bool> {
    if !still_selected("performance")? {
        return Ok(false);
    }
    set("balanced")?;
    // Do not force performance after unplugging or a concurrent user profile change.
    if !still_selected("balanced")? {
        return Ok(false);
    }
    set("performance")?;
    Ok(true)
}

fn recover_profile(cooldown: &mut Cooldown) -> Result<()> {
    cooldown.record()?;
    let cycled = cycle_profile(
        |expected| {
            Ok(ac_online()?
                && util::read("/sys/firmware/acpi/platform_profile")?.trim() == expected
                && util::output("powerprofilesctl", &["get"])?.trim() == expected)
        },
        |profile| {
            // A user service has no active login session for polkit authorization.
            util::command_output(privileged_command("powerprofilesctl").args(["set", profile]))?;
            Ok(())
        },
    )?;
    ensure!(
        cycled && ac_performance()?,
        "profile recovery aborted: AC or profile selection changed"
    );
    apply()?;
    Ok(())
}

pub fn ryzenadj(watch: bool) -> Result<()> {
    if !watch {
        if ac_performance()? {
            print!("{}", apply()?);
        } else {
            eprintln!("skipped: requires AC power and the performance profile");
        }
        return Ok(());
    }
    let mut monitor = Monitor::new()?;
    let mut cooldown = Cooldown::load(&state_directory()?, SystemTime::now(), Instant::now())?;
    let mut recovery = Recovery::default();
    let mut entered = false;
    let start = Instant::now();
    loop {
        if ac_performance()? {
            let table = output(&["-i"])?;
            if !entered || !limits_match(&table)? {
                apply()?;
                println!("Applied AC performance limits on entry or readback drift");
                entered = true;
                // Never diagnose a failed recovery using samples from before a settings change.
                monitor.previous = None;
                recovery.settings_applied();
            } else if let Some(sample) = monitor.sample(&table)?
                && let Some(action) =
                    recovery.observe(start.elapsed(), sample, Instant::now() >= cooldown.ready_at)
            {
                println!("RyzenAdj recovery {action:?}: {sample:?}");
                recovery.execute(action, &mut cooldown);
            }
        } else {
            entered = false;
            monitor.previous = None;
            recovery = Recovery::default();
        }
        thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const TABLE: &str = "| STAPM LIMIT | 53.000 | stapm-limit |\n\
        | PPT LIMIT FAST | 53.000 | fast-limit |\n\
        | PPT LIMIT SLOW | 35.000 | slow-limit |\n\
        | PPT LIMIT APU | 41.000 | apu-slow-limit |\n\
        | THM LIMIT CORE | 100.000 | tctl-temp |\n\
        | STT LIMIT APU | 90.000 | apu-skin-temp |\n\
        | PPT VALUE FAST | 13.500 | |\n\
        | THM VALUE CORE | 69.000 | |\n\
        | STT VALUE APU | 46.000 | |\n\
        | CCLK BUSY VALUE | nan | |";

    fn capped() -> Sample {
        Sample {
            watts: 13.5,
            mhz: 1400.0,
            busy: 80.0,
            core_c: 69.0,
            skin_c: 46.0,
            memory_stall: 0.5,
        }
    }

    #[test]
    fn privileged_commands_do_not_request_interactive_authorization() {
        for program in ["ryzenadj", "powerprofilesctl"] {
            let command = privileged_command(program);
            if unsafe { libc::geteuid() } == 0 {
                assert_eq!(command.get_program(), program);
                assert_eq!(command.get_args().count(), 0);
            } else {
                assert_eq!(command.get_program(), "sudo");
                assert_eq!(command.get_args().collect::<Vec<_>>(), ["-n", program]);
            }
        }
    }

    fn recovered() -> Sample {
        Sample {
            watts: 35.0,
            mhz: 3600.0,
            core_c: 100.0,
            ..capped()
        }
    }

    fn at(seconds: u64) -> Duration {
        Duration::from_secs(seconds)
    }

    fn cycled() -> Recovery {
        let mut recovery = Recovery::default();
        assert_eq!(recovery.observe(at(0), capped(), true), None);
        assert_eq!(
            recovery.observe(at(20), capped(), true),
            Some(Action::Reapply)
        );
        assert_eq!(recovery.observe(at(34), capped(), true), None);
        assert_eq!(
            recovery.observe(at(35), capped(), true),
            Some(Action::Cycle)
        );
        recovery
    }

    #[test]
    fn parses_limits_and_telemetry_without_accepting_invalid_values() {
        assert!(limits_match(TABLE).unwrap());
        assert_eq!(value(TABLE, "PPT VALUE FAST").unwrap(), 13.5);
        assert!(!limits_match(&TABLE.replace("35.000", "15.000")).unwrap());
        assert!(limits_match(&TABLE.replace("STT LIMIT APU", "unknown")).is_err());
        for invalid in ["nan", "inf", "-1"] {
            assert!(value(&TABLE.replace("13.500", invalid), "PPT VALUE FAST").is_err());
        }
        let error = value(&TABLE.replace("13.500", "invalid"), "PPT VALUE FAST").unwrap_err();
        assert!(error.to_string().contains("PPT VALUE FAST"));
        // Do not hide an invalid later row behind an earlier mismatched limit.
        assert!(limits_match(&TABLE.replace("53.000", "15.000").replace("90.000", "nan")).is_err());
    }

    #[test]
    fn reads_cpu_ticks_without_double_counting_guests_and_validates_psi() {
        assert_eq!(
            cpu_ticks("cpu 1 2 3 4 5 6 7 8 100 200\ncpu0 0").unwrap(),
            (36, 9)
        );
        assert!(cpu_ticks("cpu0 1 2 3 4 5 6 7 8").is_err());
        assert!(cpu_ticks("cpu 1 2 3").is_err());
        assert_eq!(
            memory_stall("some avg10=12.0\nfull avg10=4.20 avg60=2.0").unwrap(),
            4.2
        );
        for invalid in [
            "some avg10=0",
            "full total=0",
            "full avg10=nan",
            "full avg10=101",
            "full avg10=-1",
        ] {
            assert!(memory_stall(invalid).is_err());
        }
    }

    #[test]
    fn retries_before_cycling_and_requires_sustained_recovery_under_load() {
        let mut recovery = cycled();
        // The cooldown must not prevent verification of the cycle already performed.
        assert_eq!(recovery.observe(at(36), recovered(), false), None);
        assert_eq!(recovery.observe(at(40), recovered(), false), None);
        assert_eq!(
            recovery.observe(at(41), recovered(), false),
            Some(Action::Verified)
        );
        assert_eq!(recovery.observe(at(42), capped(), false), None);
        assert_eq!(recovery.observe(at(100), capped(), false), None);
    }

    #[test]
    fn does_not_recover_idle_hot_memory_stalled_or_already_fast_samples() {
        for sample in [
            Sample {
                busy: 59.0,
                ..capped()
            },
            Sample {
                watts: 18.0,
                ..capped()
            },
            Sample {
                mhz: 1801.0,
                ..capped()
            },
            Sample {
                core_c: 85.0,
                ..capped()
            },
            Sample {
                skin_c: 50.0,
                ..capped()
            },
            Sample {
                core_c: 0.0,
                ..capped()
            },
            Sample {
                skin_c: 0.0,
                ..capped()
            },
            Sample {
                memory_stall: 5.0,
                ..capped()
            },
        ] {
            assert!(!sample.suspect());
            let mut recovery = Recovery::default();
            assert_eq!(recovery.observe(at(0), sample, true), None);
            assert_eq!(recovery.observe(at(60), sample, true), None);
        }
    }

    #[test]
    fn transient_caps_and_successful_reapplication_do_not_cycle_profiles() {
        let mut recovery = Recovery::default();
        assert_eq!(recovery.observe(at(0), capped(), true), None);
        assert_eq!(recovery.observe(at(19), recovered(), true), None);
        assert_eq!(recovery.observe(at(20), capped(), true), None);
        assert_eq!(recovery.observe(at(39), capped(), true), None);
        assert_eq!(
            recovery.observe(at(40), capped(), true),
            Some(Action::Reapply)
        );
        assert_eq!(recovery.observe(at(41), recovered(), true), None);
        assert_eq!(recovery.observe(at(60), recovered(), true), None);
        assert_eq!(recovery.observe(at(61), capped(), true), None);
    }

    #[test]
    fn failed_or_unverifiable_cycles_disable_further_attempts() {
        for (sample, expected) in [
            (capped(), Action::Failed),
            (
                Sample {
                    busy: 10.0,
                    ..recovered()
                },
                Action::Unconfirmed,
            ),
            (
                Sample {
                    core_c: 100.0,
                    ..capped()
                },
                Action::Unconfirmed,
            ),
        ] {
            let mut recovery = cycled();
            assert_eq!(recovery.observe(at(55), sample, false), Some(expected));
            assert_eq!(recovery.observe(at(1000), capped(), true), None);
        }
    }

    #[test]
    fn recovery_verification_resets_on_a_relapse() {
        let mut recovery = cycled();
        assert_eq!(recovery.observe(at(36), recovered(), false), None);
        assert_eq!(recovery.observe(at(40), capped(), false), None);
        assert_eq!(recovery.observe(at(41), recovered(), false), None);
        assert_eq!(recovery.observe(at(45), recovered(), false), None);
        assert_eq!(
            recovery.observe(at(46), recovered(), false),
            Some(Action::Verified)
        );
    }

    #[test]
    fn settings_changes_restart_detection_without_reenabling_failed_cycles() {
        let mut recovery = Recovery::default();
        recovery.observe(at(0), capped(), true);
        recovery.settings_applied();
        assert_eq!(recovery.observe(at(20), capped(), true), None);
        assert_eq!(
            recovery.observe(at(40), capped(), true),
            Some(Action::Reapply)
        );
        recovery.settings_applied();
        assert_eq!(recovery.observe(at(55), capped(), true), None);

        let mut recovery = cycled();
        recovery.observe(at(36), recovered(), false);
        recovery.settings_applied();
        assert_eq!(recovery.observe(at(41), recovered(), false), None);
        assert_eq!(
            recovery.observe(at(46), recovered(), false),
            Some(Action::Verified)
        );

        let mut recovery = cycled();
        recovery.observe(at(36), recovered(), false);
        recovery.settings_applied();
        assert_eq!(
            recovery.observe(at(55), capped(), false),
            Some(Action::Failed)
        );
        recovery.settings_applied();
        assert_eq!(recovery.observe(at(1000), capped(), true), None);
    }

    #[test]
    fn profile_cycle_checks_intent_before_each_change_and_stops_on_errors() {
        for stop_after_check in [0, 1, 2] {
            let log = RefCell::new(Vec::new());
            let mut checks = 0;
            let did_cycle = cycle_profile(
                |profile| {
                    log.borrow_mut().push(format!("check:{profile}"));
                    let allowed = checks < stop_after_check;
                    checks += 1;
                    Ok(allowed)
                },
                |profile| {
                    log.borrow_mut().push(format!("set:{profile}"));
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(did_cycle, stop_after_check == 2);
            let expected = [
                "check:performance",
                "set:balanced",
                "check:balanced",
                "set:performance",
            ];
            let length = [1, 3, 4][stop_after_check];
            assert_eq!(*log.borrow(), &expected[..length]);
        }
        let sets = RefCell::new(Vec::new());
        assert!(
            cycle_profile(
                |_| Ok(true),
                |profile| {
                    sets.borrow_mut().push(profile.to_owned());
                    anyhow::bail!("simulated profile control error")
                }
            )
            .is_err()
        );
        assert_eq!(*sets.borrow(), ["balanced"]);
    }

    #[test]
    fn cooldown_survives_restart_and_rejects_invalid_state() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let time = UNIX_EPOCH + at(10000);
        let monotonic = Instant::now();
        let mut cooldown = Cooldown::load(directory.path(), time, monotonic)?;
        assert_eq!(cooldown.ready_at, monotonic);
        fs::write(&cooldown.path, "9900\n")?;
        assert_eq!(
            Cooldown::load(directory.path(), time, monotonic)?.ready_at,
            monotonic + at(500)
        );
        fs::write(&cooldown.path, "9000\n")?;
        assert_eq!(
            Cooldown::load(directory.path(), time, monotonic)?.ready_at,
            monotonic
        );
        for invalid in ["10001", "invalid", "18446744073709551615"] {
            fs::write(&cooldown.path, invalid)?;
            assert!(Cooldown::load(directory.path(), time, monotonic).is_err());
        }
        cooldown.record()?;
        let now = Instant::now();
        let reloaded = Cooldown::load(directory.path(), SystemTime::now(), now)?;
        assert!(reloaded.ready_at > now + at(590));
        Ok(())
    }
}
