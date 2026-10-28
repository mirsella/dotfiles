use crate::{monitoring, util};
use anyhow::{Context, Result, ensure};
use std::{
    env, fs,
    ops::Range,
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
const SUSPECT_TIME: Duration = Duration::from_secs(5);
const RETRY_TIME: Duration = Duration::from_secs(5);
const MAX_REAPPLIES: u32 = 3;
const VERIFY_TIME: Duration = Duration::from_secs(180);
const HEALTHY_TIME: Duration = Duration::from_secs(5);
const CYCLE_COOLDOWN: Duration = Duration::from_secs(600);
const RESTORE_RETRY: Duration = Duration::from_secs(5);
// One cheap telemetry tick, and one retry, every five seconds.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(5);
const TABLE_REFRESH: Duration = Duration::from_secs(30);
// A capped chip reports low power and low clocks while the CPU stays busy.
const CAP_BUSY: f64 = 60.0;
const CAP_WATTS: f64 = 17.0;
const CAP_MHZ: f64 = 1800.0;
const CAP_CORE_C: Range<f64> = 10.0..85.0;
const CAP_SKIN_C: Range<f64> = 10.0..50.0;
const RECOVERED_WATTS: f64 = 20.0;
const RECOVERED_MHZ: f64 = 2400.0;

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
    apply_with(output)
}

fn apply_with(mut run: impl FnMut(&[&str]) -> Result<String>) -> Result<String> {
    // Select the policy first, then restore our limits rather than firmware defaults.
    run(&["--max-performance"]).context("set RyzenAdj maximum-performance policy")?;
    run(&[
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

fn ac_profile(expected: &str) -> Result<bool> {
    Ok(ac_online()? && util::read("/sys/firmware/acpi/platform_profile")?.trim() == expected)
}

#[derive(Clone, Copy, Debug)]
struct Sample {
    watts: f64,
    mhz: f64,
    busy: f64,
    core_c: f64,
    skin_c: f64,
}

impl Sample {
    fn parse(table: &str, signals: Signals) -> Result<Self> {
        Ok(Self {
            watts: value(table, "PPT VALUE FAST")?,
            mhz: signals.mhz,
            busy: signals.busy,
            core_c: value(table, "THM VALUE CORE")?,
            skin_c: value(table, "STT VALUE APU")?,
        })
    }

    fn underpowered(self) -> bool {
        self.busy >= CAP_BUSY && self.watts <= CAP_WATTS && self.mhz <= CAP_MHZ
    }

    fn suspect(self) -> bool {
        // Memory pressure can coexist with a CPU cap. CPU busy time excludes I/O wait.
        self.underpowered()
            && CAP_CORE_C.contains(&self.core_c)
            && CAP_SKIN_C.contains(&self.skin_c)
    }

    fn recovered(self) -> bool {
        // Power above the observed cap is evidence even after the workload eases.
        // High idle clocks alone do not establish that the cap has lifted.
        self.watts >= RECOVERED_WATTS || (self.busy >= CAP_BUSY && self.mhz >= RECOVERED_MHZ)
    }

    fn sustained_recovery(self, now: Duration, since: &mut Option<Duration>) -> bool {
        if !self.recovered() {
            *since = None;
            return false;
        }
        now - *since.get_or_insert(now) >= HEALTHY_TIME
    }
}

#[derive(Clone, Copy, Debug)]
struct Signals {
    busy: f64,
    mhz: f64,
    core_c: Option<f64>,
}

impl Signals {
    /// Cheap telemetry gate: a privileged RyzenAdj read is only worth a fork when
    /// a stuck cap is plausible and the core is cool enough for one to hide.
    fn warrants_table(self) -> bool {
        self.busy >= CAP_BUSY
            && self.mhz <= CAP_MHZ
            && self.core_c.is_none_or(|core_c| core_c < CAP_CORE_C.end)
    }
}

#[derive(Default)]
enum Episode {
    #[default]
    Clear,
    Underpowered {
        since: Duration,
        healthy_since: Option<Duration>,
    },
}

#[derive(Debug, PartialEq)]
enum EpisodeEvent {
    Started,
    Recovered(Duration),
}

impl Episode {
    // Track fixable caps independently of recovery attempts and their cooldown.
    // An underpowered sample with a hot core is thermal throttling, not a stuck cap.
    fn observe(&mut self, now: Duration, sample: Sample) -> Option<EpisodeEvent> {
        match self {
            Self::Clear if sample.suspect() => {
                *self = Self::Underpowered {
                    since: now,
                    healthy_since: None,
                };
                Some(EpisodeEvent::Started)
            }
            Self::Underpowered {
                since,
                healthy_since,
            } => {
                if sample.sustained_recovery(now, healthy_since) {
                    let duration = now - *since;
                    *self = Self::Clear;
                    return Some(EpisodeEvent::Recovered(duration));
                }
                None
            }
            Self::Clear => None,
        }
    }
}

fn announce(event: EpisodeEvent, sample: Sample) {
    match event {
        EpisodeEvent::Started => monitoring::notify(
            "Low power: laptop is capped",
            &format!(
                "{:.0} W at {:.0} MHz while busy; the power limit looks stuck.",
                sample.watts, sample.mhz
            ),
        ),
        EpisodeEvent::Recovered(duration) => monitoring::notify(
            "Full power restored",
            &format!(
                "The power cap cleared after {:.0} s.",
                duration.as_secs_f64()
            ),
        ),
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

struct Monitor {
    clocks: Vec<PathBuf>,
    core: Option<PathBuf>,
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
        let mut core = None;
        for entry in fs::read_dir("/sys/class/hwmon")? {
            let directory = entry?.path();
            if fs::read_to_string(directory.join("name")).is_ok_and(|name| name.trim() == "k10temp")
            {
                let sensor = directory.join("temp1_input");
                if util::read(&sensor).is_err() {
                    eprintln!(
                        "Core temperature sensor {} is unreadable; gating on load and clocks only",
                        sensor.display()
                    );
                }
                core = Some(sensor);
                break;
            }
        }
        Ok(Self {
            clocks,
            core,
            previous: None,
        })
    }

    fn reset(&mut self) {
        self.previous = None;
    }

    fn telemetry(&mut self) -> Result<Option<Signals>> {
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
        // A missing or momentarily unreadable sensor only costs the gate its
        // temperature hint; RyzenAdj itself still reports core and skin.
        let core_c = self
            .core
            .as_ref()
            .and_then(|path| util::read(path).ok())
            .and_then(|text| text.trim().parse::<f64>().ok())
            .map(|millidegrees| millidegrees / 1000.0);
        let signals = Signals {
            busy: 100.0 * (total - idle) as f64 / total as f64,
            mhz: khz as f64 / self.clocks.len() as f64 / 1000.0,
            core_c,
        };
        Ok(Some(signals))
    }
}

#[derive(Debug, PartialEq)]
enum Action {
    Reapply,
    Cycle,
    Verified,
    StillCapped,
    Unconfirmed,
}

#[derive(Debug, Default)]
enum Recovery {
    #[default]
    Watching,
    Suspect(Duration),
    /// Reapplying limits every RETRY_TIME while the cap and its recovery are observed.
    Retrying {
        applied: Duration,
        reapplies: u32,
        healthy_since: Option<Duration>,
    },
    Verifying {
        since: Duration,
        healthy_since: Option<Duration>,
    },
}

impl Recovery {
    fn settings_applied(&mut self) {
        match self {
            Self::Verifying { healthy_since, .. } => *healthy_since = None,
            _ => *self = Self::Watching,
        }
    }

    fn cycle_completed(&mut self, now: Duration) {
        *self = Self::Verifying {
            since: now,
            healthy_since: None,
        };
    }

    fn observe(&mut self, now: Duration, sample: Sample, cycle_allowed: bool) -> Option<Action> {
        match self {
            Self::Verifying {
                since,
                healthy_since,
            } => {
                if sample.sustained_recovery(now, healthy_since) {
                    *self = Self::Watching;
                    return Some(Action::Verified);
                }
                if now - *since >= VERIFY_TIME {
                    *self = Self::Watching;
                    return Some(if sample.suspect() {
                        Action::StillCapped
                    } else {
                        Action::Unconfirmed
                    });
                }
                None
            }
            Self::Watching => {
                if sample.suspect() {
                    *self = Self::Suspect(now);
                }
                None
            }
            Self::Suspect(since) => {
                if !sample.suspect() {
                    *self = Self::Watching;
                } else if now - *since >= SUSPECT_TIME {
                    *self = Self::Retrying {
                        applied: now,
                        reapplies: 1,
                        healthy_since: None,
                    };
                    return Some(Action::Reapply);
                }
                None
            }
            Self::Retrying {
                applied,
                reapplies,
                healthy_since,
            } => {
                if sample.suspect() {
                    *healthy_since = None;
                    if now - *applied < RETRY_TIME {
                        return None;
                    }
                    if *reapplies >= MAX_REAPPLIES {
                        // The limited budget is spent: cycle once the cooldown
                        // allows, otherwise wait instead of hammering the SMU.
                        return cycle_allowed.then_some(Action::Cycle);
                    }
                    *reapplies += 1;
                    *applied = now;
                    return Some(Action::Reapply);
                }
                if sample.sustained_recovery(now, healthy_since) {
                    *self = Self::Watching;
                    return Some(Action::Verified);
                }
                if !sample.recovered() {
                    // Neither a fixable cap nor evidence of recovery (idle or
                    // hot): stop spending privileged reads until fresh suspicion.
                    *self = Self::Watching;
                }
                None
            }
        }
    }

    fn execute(&mut self, action: Action, cooldown: &mut Cooldown, start: Instant) {
        match action {
            Action::Reapply => match apply() {
                Ok(_) => println!(
                    "Reapplied maximum-performance policy and custom limits; continuing observation"
                ),
                Err(error) => eprintln!(
                    "RyzenAdj retry failed: {error:#}; waiting for telemetry before profile recovery"
                ),
            },
            Action::Cycle => {
                if let Err(error) = recover_profile(cooldown) {
                    *self = Self::Watching;
                    eprintln!(
                        "Profile recovery command failed: {error:#}; monitoring continues, with further attempts subject to the {CYCLE_COOLDOWN:?} cooldown"
                    );
                } else {
                    // Start the verification window after all recovery commands finish.
                    self.cycle_completed(start.elapsed());
                    println!(
                        "Cycled balanced -> performance and reapplied policy and limits; observing recovery for up to {VERIFY_TIME:?}"
                    );
                }
            }
            Action::Verified => {
                println!("Power or clocks improved for {HEALTHY_TIME:?} after recovery actions")
            }
            Action::StillCapped | Action::Unconfirmed => eprintln!(
                "Post-cycle observation {action:?} after {VERIFY_TIME:?}; monitoring continues, with further attempts subject to the {CYCLE_COOLDOWN:?} cooldown"
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
    mut transition: impl FnMut(&str, &str) -> Result<bool>,
    mut wait_before_retry: impl FnMut(),
) -> Result<bool> {
    if !transition("performance", "balanced")? {
        return Ok(false);
    }
    loop {
        match transition("balanced", "performance") {
            Ok(restored) => return Ok(restored),
            Err(error) => {
                eprintln!(
                    "Could not restore performance after selecting balanced: {error:#}; retrying the guarded return step in {RESTORE_RETRY:?}"
                );
                wait_before_retry();
            }
        }
    }
}

fn recover_profile(cooldown: &mut Cooldown) -> Result<()> {
    cooldown.record()?;
    let cycled = cycle_profile(
        |from, to| {
            // Do not force performance after unplugging or a concurrent user profile change.
            if !ac_profile(from)? || util::output("powerprofilesctl", &["get"])?.trim() != from {
                return Ok(false);
            }
            // A user service has no active login session for polkit authorization.
            util::command_output(privileged_command("powerprofilesctl").args(["set", to]))?;
            Ok(true)
        },
        || thread::sleep(RESTORE_RETRY),
    )?;
    ensure!(
        cycled && ac_profile("performance")?,
        "profile recovery aborted: AC or profile selection changed"
    );
    apply()?;
    Ok(())
}

pub fn ryzenadj(watch: bool) -> Result<()> {
    if !watch {
        if ac_profile("performance")? {
            print!("{}", apply()?);
        } else {
            eprintln!("skipped: requires AC power and the performance profile");
        }
        return Ok(());
    }
    let mut monitor = Monitor::new()?;
    let mut cooldown = Cooldown::load(&state_directory()?, SystemTime::now(), Instant::now())?;
    let mut recovery = Recovery::default();
    let mut episode = Episode::default();
    let mut entered = false;
    let mut last_status = None::<Instant>;
    let mut last_table = None::<Instant>;
    let start = Instant::now();
    loop {
        let tick = Instant::now();
        if ac_profile("performance")? {
            let signals = monitor.telemetry()?;
            // An open episode alone must not pin the privileged cadence: idle and
            // hot samples need no reads, and a new cap re-enters Suspect on its own.
            let following = !matches!(recovery, Recovery::Watching);
            // RyzenAdj needs root, so read the power table only on entry, while
            // diagnosing, on a slow refresh, or when a stuck cap looks plausible.
            if !entered
                || following
                || last_table.is_none_or(|read| read.elapsed() >= TABLE_REFRESH)
                || signals.is_some_and(Signals::warrants_table)
            {
                let table = output(&["-i"])?;
                last_table = Some(Instant::now());
                if !entered || !limits_match(&table)? {
                    apply()?;
                    println!("Applied AC performance policy and limits on entry or readback drift");
                    entered = true;
                    // Never diagnose a failed recovery using samples from before a settings change.
                    monitor.reset();
                    recovery.settings_applied();
                } else if let Some(signals) = signals {
                    let sample = Sample::parse(&table, signals)?;
                    let now = Instant::now();
                    if let Some(event) = episode.observe(start.elapsed(), sample) {
                        println!("Low-power episode {event:?}: {sample:?}; recovery={recovery:?}");
                        announce(event, sample);
                        last_status = None;
                    }
                    if sample.underpowered()
                        && last_status.is_none_or(|last| now - last >= Duration::from_secs(60))
                    {
                        println!(
                            "Low-power performance status: {sample:?}; recovery={recovery:?}; cooldown_remaining={:?}",
                            cooldown.ready_at.saturating_duration_since(now)
                        );
                        last_status = Some(now);
                    }
                    if let Some(action) =
                        recovery.observe(start.elapsed(), sample, now >= cooldown.ready_at)
                    {
                        println!("RyzenAdj recovery {action:?}: {sample:?}");
                        recovery.execute(action, &mut cooldown, start);
                    }
                }
            }
        } else {
            if matches!(episode, Episode::Underpowered { .. }) {
                println!("Stopped observing low-power episode: left AC performance mode");
            }
            episode = Episode::default();
            entered = false;
            last_table = None;
            monitor.reset();
            recovery = Recovery::default();
            last_status = None;
        }
        thread::sleep(SAMPLE_INTERVAL.saturating_sub(tick.elapsed()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        }
    }

    #[test]
    fn only_reads_the_power_table_for_plausible_caps() {
        let capped = Signals {
            busy: 80.0,
            mhz: 1400.0,
            core_c: Some(69.0),
        };
        assert!(capped.warrants_table());
        let unknown_temperature = Signals {
            core_c: None,
            ..capped
        };
        assert!(unknown_temperature.warrants_table());
        let idle = Signals {
            busy: 20.0,
            ..capped
        };
        assert!(!idle.warrants_table());
        let already_fast = Signals {
            mhz: 3200.0,
            ..capped
        };
        assert!(!already_fast.warrants_table());
        let hot = Signals {
            core_c: Some(97.0),
            ..capped
        };
        assert!(!hot.warrants_table());
    }

    #[test]
    fn selects_maximum_performance_before_custom_limits() -> Result<()> {
        let mut commands = Vec::new();
        let table = apply_with(|args| {
            commands.push(args.join(" "));
            Ok(if commands.len() == 1 {
                "policy enabled".to_owned()
            } else {
                TABLE.to_owned()
            })
        })?;
        assert_eq!(
            commands,
            [
                "--max-performance",
                "--fast-limit=53000 --slow-limit=35000 --apu-slow-limit=41000 --skin-temp-limit=45000 --tctl-temp=100 --apu-skin-temp=90 -i",
            ]
        );
        assert_eq!(table, TABLE);
        Ok(())
    }

    #[test]
    fn stops_if_maximum_performance_policy_is_rejected() {
        let mut commands = Vec::new();
        let error = apply_with(|args| {
            commands.push(args.join(" "));
            anyhow::bail!("SMU rejected policy")
        })
        .unwrap_err();
        assert_eq!(commands, ["--max-performance"]);
        assert!(format!("{error:#}").contains("SMU rejected policy"));
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

    /// Moment of a recovery reapply, counted from the first capped sample.
    fn reapply(attempt: u32) -> Duration {
        at(SUSPECT_TIME.as_secs() + u64::from(attempt) * RETRY_TIME.as_secs())
    }

    fn cycle_at() -> Duration {
        reapply(MAX_REAPPLIES)
    }

    fn verification_end() -> Duration {
        cycle_at().saturating_add(VERIFY_TIME)
    }

    fn cycled() -> Recovery {
        let mut recovery = Recovery::default();
        assert_eq!(recovery.observe(at(0), capped(), true), None);
        for attempt in 0..MAX_REAPPLIES {
            assert_eq!(
                recovery.observe(reapply(attempt), capped(), true),
                Some(Action::Reapply)
            );
        }
        assert_eq!(
            recovery.observe(cycle_at(), capped(), true),
            Some(Action::Cycle)
        );
        recovery.cycle_completed(cycle_at());
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
    fn reads_cpu_ticks_without_double_counting_guests() {
        assert_eq!(
            cpu_ticks("cpu 1 2 3 4 5 6 7 8 100 200\ncpu0 0").unwrap(),
            (36, 9)
        );
        assert!(cpu_ticks("cpu0 1 2 3 4 5 6 7 8").is_err());
        assert!(cpu_ticks("cpu 1 2 3").is_err());
    }

    #[test]
    fn retries_before_cycling_and_requires_sustained_recovery_under_load() {
        let mut recovery = cycled();
        // The cooldown must not prevent verification of the cycle already performed.
        let healthy = cycle_at() + at(1);
        assert_eq!(recovery.observe(healthy, recovered(), false), None);
        assert_eq!(
            recovery.observe(healthy + HEALTHY_TIME, recovered(), false),
            Some(Action::Verified)
        );
        // A new cap during the cooldown keeps reapplying limits instead of cycling.
        let relapsed = healthy + HEALTHY_TIME + at(1);
        assert_eq!(recovery.observe(relapsed, capped(), false), None);
        assert_eq!(
            recovery.observe(relapsed + SUSPECT_TIME, capped(), false),
            Some(Action::Reapply)
        );
    }

    #[test]
    fn does_not_recover_idle_hot_or_already_fast_samples() {
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
        ] {
            assert!(!sample.suspect());
            let mut recovery = Recovery::default();
            assert_eq!(recovery.observe(at(0), sample, true), None);
            assert_eq!(recovery.observe(at(60), sample, true), None);
        }
    }

    #[test]
    fn observed_cap_with_concurrent_swapping_still_reaches_profile_recovery() {
        // Live recurrence: memory full PSI was 5.84%, but CPUs remained 90% busy.
        let observed = Sample {
            watts: 16.681,
            mhz: 1170.25,
            busy: 90.0,
            core_c: 69.815,
            skin_c: 45.131,
        };
        let mut recovery = Recovery::default();
        for second in 0..=cycle_at().as_secs() {
            let expected = if (0..MAX_REAPPLIES).any(|attempt| second == reapply(attempt).as_secs())
            {
                Some(Action::Reapply)
            } else if second == cycle_at().as_secs() {
                Some(Action::Cycle)
            } else {
                None
            };
            assert_eq!(recovery.observe(at(second), observed, true), expected);
        }
    }

    #[test]
    fn idle_samples_release_the_retry_window_instead_of_pinning_it() {
        let mut recovery = Recovery::default();
        assert_eq!(recovery.observe(at(0), capped(), true), None);
        assert_eq!(
            recovery.observe(reapply(0), capped(), true),
            Some(Action::Reapply)
        );
        let idle = Sample {
            busy: 10.0,
            mhz: 400.0,
            watts: 15.0,
            ..capped()
        };
        assert_eq!(recovery.observe(reapply(0) + RETRY_TIME, idle, true), None);
        // Fresh suspicion is required before another write.
        assert_eq!(
            recovery.observe(reapply(0) + RETRY_TIME * 2, capped(), true),
            None
        );
        assert_eq!(
            recovery.observe(reapply(0) + RETRY_TIME * 3, capped(), true),
            Some(Action::Reapply)
        );
    }

    #[test]
    fn transient_caps_and_successful_reapplication_do_not_cycle_profiles() {
        let mut recovery = Recovery::default();
        assert_eq!(recovery.observe(at(0), capped(), true), None);
        // A cap shorter than the detection window never triggers a write.
        assert_eq!(recovery.observe(at(2), recovered(), true), None);
        assert_eq!(recovery.observe(at(6), capped(), true), None);
        let first = at(6) + SUSPECT_TIME;
        assert_eq!(
            recovery.observe(first, capped(), true),
            Some(Action::Reapply)
        );
        // Recovery during the retry window ends the episode without a profile cycle.
        let healthy = first + at(1);
        assert_eq!(recovery.observe(healthy, recovered(), true), None);
        assert_eq!(
            recovery.observe(healthy + HEALTHY_TIME, recovered(), true),
            Some(Action::Verified)
        );
    }

    #[test]
    fn unsuccessful_cycles_resume_detection_after_the_cooldown() {
        for (sample, expected) in [
            (capped(), Action::StillCapped),
            (
                Sample {
                    busy: 10.0,
                    ..capped()
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
            assert_eq!(recovery.observe(cycle_at() + at(1), sample, false), None);
            assert_eq!(
                recovery.observe(verification_end(), sample, false),
                Some(expected)
            );
            // The persisted cooldown blocks profile cycles but not limit reapplies.
            let relapsed = verification_end() + at(1);
            assert_eq!(recovery.observe(relapsed, capped(), false), None);
            for attempt in 0..MAX_REAPPLIES {
                assert_eq!(
                    recovery.observe(
                        relapsed + SUSPECT_TIME + RETRY_TIME * attempt,
                        capped(),
                        false
                    ),
                    Some(Action::Reapply)
                );
            }
            // Once the budget is spent and cycling is on cooldown, stop writing.
            let exhausted = relapsed + SUSPECT_TIME + RETRY_TIME * MAX_REAPPLIES;
            assert_eq!(recovery.observe(exhausted, capped(), false), None);
            assert_eq!(
                recovery.observe(exhausted + RETRY_TIME, capped(), true),
                Some(Action::Cycle)
            );
        }
    }

    #[test]
    fn delayed_recovery_counts_even_after_cpu_load_drops() {
        let mut recovery = cycled();
        let start = cycle_at() + at(5);
        assert_eq!(recovery.observe(start, capped(), false), None);
        assert_eq!(recovery.observe(start + at(95), capped(), false), None);
        let partial_load = Sample {
            busy: 35.0,
            ..recovered()
        };
        let partial_at = start + at(105);
        assert_eq!(recovery.observe(partial_at, partial_load, false), None);
        assert_eq!(
            recovery.observe(partial_at + at(4), partial_load, false),
            None
        );
        assert_eq!(
            recovery.observe(partial_at + HEALTHY_TIME, partial_load, false),
            Some(Action::Verified)
        );
        // A new episode is still eligible once the persisted cooldown permits it.
        let relapsed = partial_at + HEALTHY_TIME + at(1);
        assert_eq!(recovery.observe(relapsed, capped(), true), None);
        assert_eq!(
            recovery.observe(relapsed + SUSPECT_TIME, capped(), true),
            Some(Action::Reapply)
        );
    }

    #[test]
    fn idle_clocks_do_not_verify_recovery_but_sustained_power_does() {
        let idle = Sample {
            busy: 10.0,
            mhz: 4000.0,
            ..capped()
        };
        assert!(!idle.recovered());
        assert!(
            Sample {
                watts: 20.0,
                ..idle
            }
            .recovered()
        );
        assert!(Sample { busy: 60.0, ..idle }.recovered());
        let mut recovery = cycled();
        assert_eq!(recovery.observe(cycle_at() + at(1), idle, false), None);
        assert_eq!(
            recovery.observe(verification_end(), idle, false),
            Some(Action::Unconfirmed)
        );
    }

    #[test]
    fn verification_window_starts_when_commands_finish() {
        let mut recovery = cycled();
        let finished = cycle_at() + at(30);
        recovery.cycle_completed(finished);
        assert_eq!(
            recovery.observe(finished + VERIFY_TIME - at(1), capped(), false),
            None
        );
        assert_eq!(
            recovery.observe(finished + VERIFY_TIME, capped(), false),
            Some(Action::StillCapped)
        );
    }

    #[test]
    fn episodes_only_start_for_fixable_caps() {
        let mut episode = Episode::default();
        let hot = Sample {
            core_c: 100.0,
            ..capped()
        };
        assert_eq!(episode.observe(at(0), hot), None);
        let idle = Sample {
            busy: 10.0,
            ..capped()
        };
        assert_eq!(episode.observe(at(5), idle), None);
        assert_eq!(
            episode.observe(at(10), capped()),
            Some(EpisodeEvent::Started)
        );
    }

    #[test]
    fn episodes_log_late_recovery_and_recurrence_independently_of_attempts() {
        let mut episode = Episode::default();
        assert_eq!(
            episode.observe(at(0), capped()),
            Some(EpisodeEvent::Started)
        );
        assert_eq!(episode.observe(at(180), capped()), None);
        let partial_load = Sample {
            busy: 35.0,
            ..recovered()
        };
        assert_eq!(episode.observe(at(240), partial_load), None);
        // A relapse resets the healthy streak rather than ending the episode.
        assert_eq!(episode.observe(at(244), capped()), None);
        assert_eq!(episode.observe(at(245), partial_load), None);
        assert_eq!(
            episode.observe(at(250), partial_load),
            Some(EpisodeEvent::Recovered(at(250)))
        );
        assert_eq!(episode.observe(at(251), partial_load), None);
        assert_eq!(
            episode.observe(at(260), capped()),
            Some(EpisodeEvent::Started)
        );
        // Becoming idle is not evidence that the cap lifted.
        let idle = Sample {
            busy: 5.0,
            ..capped()
        };
        assert_eq!(episode.observe(at(270), idle), None);
        assert_eq!(episode.observe(at(280), idle), None);
        assert_eq!(episode.observe(at(300), recovered()), None);
        assert_eq!(
            episode.observe(at(305), recovered()),
            Some(EpisodeEvent::Recovered(at(45)))
        );
    }

    #[test]
    fn recovery_verification_resets_on_a_relapse() {
        let mut recovery = cycled();
        let healthy = cycle_at() + at(1);
        assert_eq!(recovery.observe(healthy, recovered(), false), None);
        assert_eq!(recovery.observe(healthy + at(2), capped(), false), None);
        let again = healthy + at(3);
        assert_eq!(recovery.observe(again, recovered(), false), None);
        assert_eq!(recovery.observe(again + at(4), recovered(), false), None);
        assert_eq!(
            recovery.observe(again + HEALTHY_TIME, recovered(), false),
            Some(Action::Verified)
        );
    }

    #[test]
    fn settings_changes_do_not_reset_the_observed_episode_recovery() {
        let mut episode = Episode::default();
        let mut recovery = cycled();
        assert_eq!(
            episode.observe(cycle_at(), capped()),
            Some(EpisodeEvent::Started)
        );
        let healthy = cycle_at() + at(1);
        assert_eq!(episode.observe(healthy, recovered()), None);
        assert_eq!(recovery.observe(healthy, recovered(), false), None);
        recovery.settings_applied();
        let reset = healthy + HEALTHY_TIME;
        assert_eq!(
            episode.observe(reset, recovered()),
            Some(EpisodeEvent::Recovered(at(6)))
        );
        assert_eq!(recovery.observe(reset, recovered(), false), None);
        assert_eq!(
            recovery.observe(reset + HEALTHY_TIME, recovered(), false),
            Some(Action::Verified)
        );
    }

    #[test]
    fn settings_changes_restart_detection_without_shortening_verification() {
        let mut recovery = Recovery::default();
        recovery.observe(at(0), capped(), true);
        recovery.settings_applied();
        assert_eq!(recovery.observe(at(20), capped(), true), None);
        assert_eq!(
            recovery.observe(at(20) + SUSPECT_TIME, capped(), true),
            Some(Action::Reapply)
        );
        recovery.settings_applied();
        assert_eq!(recovery.observe(at(40), capped(), true), None);

        let mut recovery = cycled();
        let healthy = cycle_at() + at(1);
        recovery.observe(healthy, recovered(), false);
        recovery.settings_applied();
        assert_eq!(recovery.observe(healthy + at(1), recovered(), false), None);
        assert_eq!(
            recovery.observe(healthy + at(1) + HEALTHY_TIME, recovered(), false),
            Some(Action::Verified)
        );

        let mut recovery = cycled();
        recovery.observe(healthy, recovered(), false);
        recovery.settings_applied();
        assert_eq!(
            recovery.observe(verification_end(), capped(), false),
            Some(Action::StillCapped)
        );
        recovery.settings_applied();
        assert_eq!(recovery.observe(at(1000), capped(), true), None);
        assert_eq!(
            recovery.observe(at(1000) + SUSPECT_TIME, capped(), true),
            Some(Action::Reapply)
        );
    }

    #[test]
    fn reapplies_every_five_seconds_until_the_cap_lifts() {
        assert_eq!(SAMPLE_INTERVAL, Duration::from_secs(5));
        assert_eq!(SUSPECT_TIME, Duration::from_secs(5));
        assert_eq!(RETRY_TIME, Duration::from_secs(5));
        let mut recovery = Recovery::default();
        let mut actions = Vec::new();
        for tick in 0..=4 {
            let elapsed = at(tick * SAMPLE_INTERVAL.as_secs());
            if let Some(action) = recovery.observe(elapsed, capped(), true) {
                actions.push((elapsed.as_secs(), action));
            }
        }
        assert_eq!(
            actions,
            vec![
                (reapply(0).as_secs(), Action::Reapply),
                (reapply(1).as_secs(), Action::Reapply),
                (reapply(2).as_secs(), Action::Reapply),
                (cycle_at().as_secs(), Action::Cycle),
            ]
        );
    }

    #[test]
    fn profile_cycle_stops_on_cancellation_or_initial_failure() {
        for allowed in 0..=2 {
            let mut transitions = Vec::new();
            let did_cycle = cycle_profile(
                |from, to| {
                    transitions.push(format!("{from}->{to}"));
                    Ok(transitions.len() <= allowed)
                },
                || panic!("cancelled transitions must not retry"),
            )
            .unwrap();
            assert_eq!(did_cycle, allowed == 2);
            assert_eq!(
                transitions,
                ["performance->balanced", "balanced->performance"][..(allowed + 1).min(2)]
            );
        }
        let mut sets = Vec::new();
        assert!(
            cycle_profile(
                |_, to| {
                    sets.push(to.to_owned());
                    anyhow::bail!("simulated profile control error")
                },
                || panic!("failed initial transitions must not retry"),
            )
            .is_err()
        );
        assert_eq!(sets, ["balanced"]);
    }

    #[test]
    fn failed_return_retries_only_the_guarded_performance_transition() -> Result<()> {
        for failures in [1, 3] {
            for cancelled in [false, true] {
                let mut transitions = Vec::new();
                let mut waits = 0;
                let restored = cycle_profile(
                    |from, to| {
                        transitions.push(format!("{from}->{to}"));
                        if (2..2 + failures).contains(&transitions.len()) {
                            anyhow::bail!("performance setter temporarily unavailable");
                        }
                        Ok(transitions.len() == 1 || !cancelled)
                    },
                    || waits += 1,
                )?;
                assert_eq!(restored, !cancelled);
                assert_eq!(waits, failures);
                assert_eq!(transitions.len(), failures + 2);
                assert_eq!(transitions[0], "performance->balanced");
                assert!(
                    transitions[1..]
                        .iter()
                        .all(|step| step == "balanced->performance")
                );
            }
        }
        Ok(())
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
