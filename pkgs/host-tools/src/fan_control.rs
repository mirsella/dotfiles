use crate::util;
use anyhow::{Context, Result, ensure};
use clap::{Args, Subcommand};
use dbus::{
    blocking::{
        LocalConnection,
        stdintf::org_freedesktop_dbus::{Properties, PropertiesPropertiesChanged},
    },
    message::MatchRule,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::Cell,
    collections::VecDeque,
    fs,
    os::{
        linux::net::SocketAddrExt,
        unix::{
            ffi::OsStrExt,
            net::{SocketAddr, UnixDatagram},
        },
    },
    path::{Path, PathBuf},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const SERVICE: &str = "org.freedesktop.UPower.PowerProfiles";
const OBJECT: &str = "/org/freedesktop/UPower/PowerProfiles";
const SPIN_UP: Duration = Duration::from_secs(1);
const STALL_LIMIT: Duration = Duration::from_secs(5);
static STOP: AtomicBool = AtomicBool::new(false);
static RELOAD: AtomicBool = AtomicBool::new(false);

#[derive(Subcommand)]
pub enum Operation {
    /// Control fans and follow power-profile/resume events in one process.
    Run {
        #[command(flatten)]
        hardware: Hardware,
        config: PathBuf,
        #[arg(long)]
        status: Option<PathBuf>,
    },
    /// Restore firmware Smart Fan IV without Linux sensor or D-Bus dependencies.
    Automatic {
        #[command(flatten)]
        hardware: Hardware,
    },
}

#[derive(Args)]
pub struct Hardware {
    #[arg(long)]
    controller: String,
    #[arg(long, value_delimiter = ',', required = true)]
    channels: Vec<u8>,
    /// Alternative hwmon inventory for isolated hardware diagnostics.
    #[arg(long, default_value = "/sys/class/hwmon")]
    hwmon_root: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    minimum_duty: u8,
    start_duty: u8,
    start_hysteresis: u8,
    case_fans: CaseFans,
    interval: u8,
    average: u8,
    profiles: std::collections::BTreeMap<String, Ramp>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseFans {
    channels: Vec<u8>,
    minimum_duty: u8,
    gpu: GpuRamp,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GpuRamp {
    minimum_temperature: u8,
    maximum_temperature: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ramp {
    minimum_temperature: u8,
    maximum_temperature: u8,
    maximum_duty: u8,
}

impl Hardware {
    fn discover(&self) -> Result<PathBuf> {
        ensure!(!self.controller.is_empty(), "Fan controller name is empty");
        ensure!(!self.channels.is_empty(), "No fan channels configured");
        for (index, channel) in self.channels.iter().enumerate() {
            ensure!((1..=6).contains(channel), "Invalid fan channel {channel}");
            ensure!(
                !self.channels[..index].contains(channel),
                "Duplicate fan channel {channel}"
            );
        }
        unique_device(
            &hwmon_devices(&self.hwmon_root)?,
            |name| name == self.controller,
            "fan controller",
        )
    }
}

impl Config {
    fn load(path: &Path, hardware: &Hardware) -> Result<Self> {
        let config: Self = serde_json::from_str(&util::read(path)?)?;
        ensure!(
            config.minimum_duty > 0 && config.minimum_duty <= config.start_duty,
            "Invalid minimum/start duty"
        );
        ensure!(
            config.interval > 0 && config.average > 0,
            "Fan sampling interval and averaging must be positive"
        );
        ensure!(
            config.start_hysteresis > 0,
            "Fan start hysteresis must be positive"
        );
        ensure!(
            config.case_fans.minimum_duty >= config.start_duty
                && config.case_fans.minimum_duty <= 100,
            "Case fan minimum must be sufficient to start a stopped fan"
        );
        ensure!(
            config.case_fans.gpu.minimum_temperature < config.case_fans.gpu.maximum_temperature
                && config.case_fans.gpu.maximum_temperature <= 100,
            "Invalid GPU fan temperature range"
        );
        for (index, channel) in config.case_fans.channels.iter().enumerate() {
            ensure!(
                hardware.channels.contains(channel),
                "Uncontrolled case fan channel {channel}"
            );
            ensure!(
                !config.case_fans.channels[..index].contains(channel),
                "Duplicate case fan channel {channel}"
            );
        }
        ensure!(
            config.profiles.len() == 3,
            "Expected exactly three power profiles"
        );
        for name in ["power-saver", "balanced", "performance"] {
            let ramp = config
                .profiles
                .get(name)
                .with_context(|| format!("Missing {name} fan ramp"))?;
            ensure!(
                ramp.minimum_temperature < ramp.maximum_temperature
                    && u16::from(ramp.minimum_temperature) + u16::from(config.start_hysteresis)
                        < u16::from(ramp.maximum_temperature)
                    && ramp.maximum_temperature <= 100,
                "Invalid {name} temperature range"
            );
            ensure!(
                config.minimum_duty < ramp.maximum_duty
                    && config.start_duty <= ramp.maximum_duty
                    && config.case_fans.minimum_duty <= ramp.maximum_duty
                    && ramp.maximum_duty <= 100,
                "Invalid {name} fan maximum/start duty"
            );
        }
        if let Ok(value) = std::env::var("WATCHDOG_USEC") {
            let watchdog: u64 = value.parse().context("Invalid WATCHDOG_USEC")?;
            ensure!(
                u64::from(config.interval) * 2_000_000 <= watchdog,
                "Sampling interval must be at most half the systemd watchdog period"
            );
        }
        Ok(config)
    }
}

fn hwmon_devices(root: &Path) -> Result<Vec<(PathBuf, String)>> {
    fs::read_dir(root)?
        .map(|entry| {
            let path = entry?.path();
            let name = util::read(path.join("name"))?.trim().to_owned();
            Ok((path, name))
        })
        .collect()
}

fn unique_device(
    devices: &[(PathBuf, String)],
    matches: impl Fn(&str) -> bool,
    description: &str,
) -> Result<PathBuf> {
    let mut candidates = devices.iter().filter(|(_, name)| matches(name));
    let (path, _) = candidates
        .next()
        .with_context(|| format!("No {description} found"))?;
    ensure!(
        candidates.next().is_none(),
        "Multiple devices match {description}"
    );
    fs::canonicalize(path).with_context(|| format!("Resolve {description}"))
}

fn automatic(hardware: &Hardware, controller: &Path) -> Result<()> {
    // Smart Fan IV uses the firmware's existing sensor selection and curve.
    // Manual control changes only duty/mode, leaving those settings intact.
    let mut errors = Vec::new();
    for channel in &hardware.channels {
        let path = controller.join(format!("pwm{channel}_enable"));
        if let Err(error) = fs::write(&path, "5\n") {
            errors.push(format!("{}: {error}", path.display()));
        }
    }
    ensure!(
        errors.is_empty(),
        "Could not restore motherboard automatic fan control: {}",
        errors.join("; ")
    );
    eprintln!(
        "Returned all {} fan channels to motherboard automatic control",
        hardware.channels.len()
    );
    Ok(())
}

fn pwm(duty: u8) -> u16 {
    (u16::from(duty) * 255 + 50) / 100
}

fn read_temperature(path: &Path) -> Result<i64> {
    let value = read_number(path)?;
    ensure!(
        (1000..=120000).contains(&value),
        "Invalid temperature {value} from {}",
        path.display()
    );
    Ok(value)
}

fn read_number(path: &Path) -> Result<i64> {
    util::read(path)?
        .trim()
        .parse()
        .with_context(|| format!("Parse {}", path.display()))
}

#[derive(Clone, Copy)]
struct Reading {
    pwm: u16,
    rpm: u32,
    manual: bool,
}

struct Fans {
    hardware: Hardware,
    controller: PathBuf,
}
impl Fans {
    fn discover(hardware: Hardware) -> Result<Self> {
        let controller = hardware.discover()?;
        Ok(Self {
            hardware,
            controller,
        })
    }

    fn readings(&self) -> Result<Vec<Reading>> {
        self.hardware
            .channels
            .iter()
            .map(|channel| {
                let pwm = read_number(&self.controller.join(format!("pwm{channel}")))?;
                let rpm = read_number(&self.controller.join(format!("fan{channel}_input")))?;
                let mode = read_number(&self.controller.join(format!("pwm{channel}_enable")))?;
                ensure!(
                    (0..=255).contains(&pwm)
                        && (0..=100000).contains(&rpm)
                        && (0..=5).contains(&mode),
                    "Invalid PWM/RPM/mode on fan {channel}: {pwm}/{rpm}/{mode}"
                );
                Ok(Reading {
                    pwm: pwm as u16,
                    rpm: rpm as u32,
                    manual: mode == 1,
                })
            })
            .collect()
    }

    fn apply(&self, targets: &[u16], readings: &[Reading]) -> Result<()> {
        ensure!(
            targets.len() == self.hardware.channels.len() && targets.len() == readings.len(),
            "Fan target count mismatch"
        );
        for ((channel, target), reading) in self.hardware.channels.iter().zip(targets).zip(readings)
        {
            let path = self.controller.join(format!("pwm{channel}"));
            if !reading.manual {
                // The driver rejects PWM writes while Smart Fan IV is active.
                fs::write(self.controller.join(format!("pwm{channel}_enable")), "1\n")
                    .with_context(|| format!("Enter manual control on fan {channel}"))?;
            }
            if reading.pwm != *target || !reading.manual {
                fs::write(&path, format!("{target}\n"))
                    .with_context(|| format!("Set fan {channel} PWM to {target}"))?;
            }
        }
        Ok(())
    }
}
impl Drop for Fans {
    fn drop(&mut self) {
        if let Err(error) = automatic(&self.hardware, &self.controller) {
            eprintln!("Fan automatic recovery failed: {error:#}");
        }
    }
}

#[derive(Clone, Copy)]
struct Sample {
    cpu: i64,
    gpu: Option<i64>,
}
struct Sensors {
    cpu: PathBuf,
    gpu: Option<PathBuf>,
}
impl Sensors {
    fn discover(fans: &mut Fans, config: &Config) -> Result<Self> {
        let devices = hwmon_devices(&fans.hardware.hwmon_root)?;
        fans.controller = unique_device(
            &devices,
            |name| name == fans.hardware.controller,
            "fan controller",
        )?;
        let cpu = unique_device(
            &devices,
            |name| matches!(name, "zenpower" | "k10temp"),
            "Ryzen temperature provider",
        )?;
        let label = util::read(cpu.join("temp1_label"))?;
        ensure!(
            matches!(label.trim(), "Tdie" | "Tctl"),
            "Unexpected CPU temperature label {label:?}"
        );
        let gpu = if config.case_fans.channels.is_empty() {
            None
        } else {
            let gpu = unique_device(
                &devices,
                |name| name == "amdgpu",
                "AMD GPU temperature provider",
            )?;
            ensure!(
                util::read(gpu.join("temp1_label"))?.trim() == "edge",
                "Expected GPU edge temperature"
            );
            Some(gpu.join("temp1_input"))
        };
        let sensors = Self {
            cpu: cpu.join("temp1_input"),
            gpu,
        };
        eprintln!(
            "CPU die sensor: {}; GPU edge sensor: {:?}",
            sensors.cpu.display(),
            sensors.gpu
        );
        Ok(sensors)
    }
    fn sample(&self) -> Result<Sample> {
        Ok(Sample {
            cpu: read_temperature(&self.cpu)?,
            gpu: self.gpu.as_deref().map(read_temperature).transpose()?,
        })
    }
}

fn interpolate(value: i64, ramp: &Ramp, floor: u16) -> u16 {
    let minimum = i64::from(ramp.minimum_temperature) * 1000;
    let range = i64::from(ramp.maximum_temperature - ramp.minimum_temperature) * 1000;
    (i64::from(floor)
        + (value - minimum).clamp(0, range) * i64::from(pwm(ramp.maximum_duty) - floor) / range)
        as u16
}
fn case_temperature(sample: Sample, ramp: &Ramp, gpu: &GpuRamp) -> Result<i64> {
    let value = sample
        .gpu
        .context("Case airflow requires a GPU temperature")?;
    let lower = i64::from(gpu.minimum_temperature) * 1000;
    let range = i64::from(gpu.maximum_temperature - gpu.minimum_temperature) * 1000;
    let equivalent = i64::from(ramp.minimum_temperature) * 1000
        + (value - lower).clamp(0, range)
            * i64::from(ramp.maximum_temperature - ramp.minimum_temperature)
            * 1000
            / range;
    Ok(sample.cpu.max(equivalent))
}

struct Channel {
    id: u8,
    requested: u16,
    kick_until: Option<Instant>,
    stalled_since: Option<Instant>,
}
struct Control {
    config: Config,
    profile: String,
    history: VecDeque<Sample>,
    channels: Vec<Channel>,
}
impl Control {
    fn new(
        config: Config,
        profile: String,
        hardware: &Hardware,
        readings: &[Reading],
    ) -> Result<Self> {
        ensure!(
            config.profiles.contains_key(&profile),
            "Unsupported profile {profile:?}"
        );
        ensure!(
            readings.len() == hardware.channels.len(),
            "Fan reading count mismatch"
        );
        let channels = hardware
            .channels
            .iter()
            .zip(readings)
            .map(|(&id, reading)| Channel {
                id,
                requested: if reading.rpm > 0 { reading.pwm } else { 0 },
                kick_until: None,
                stalled_since: None,
            })
            .collect();
        Ok(Self {
            config,
            profile,
            history: VecDeque::new(),
            channels,
        })
    }
    fn change_profile(&mut self, profile: String) -> Result<()> {
        ensure!(
            self.config.profiles.contains_key(&profile),
            "Unsupported profile {profile:?}"
        );
        self.profile = profile;
        self.history.clear();
        eprintln!("Using {} fan profile", self.profile);
        Ok(())
    }
    fn targets(&mut self, sample: Sample, readings: &[Reading], now: Instant) -> Result<Vec<u16>> {
        ensure!(
            readings.len() == self.channels.len(),
            "Fan reading count mismatch"
        );
        self.history.push_back(sample);
        while self.history.len() > usize::from(self.config.average) {
            self.history.pop_front();
        }
        let ramp = &self.config.profiles[&self.profile];
        let cpu_average =
            self.history.iter().map(|v| v.cpu).sum::<i64>() / self.history.len() as i64;
        let case_values = if self.config.case_fans.channels.is_empty() {
            None
        } else {
            let raw = case_temperature(sample, ramp, &self.config.case_fans.gpu)?;
            let total = self.history.iter().try_fold(0, |sum, &sample| {
                case_temperature(sample, ramp, &self.config.case_fans.gpu).map(|value| sum + value)
            })?;
            Some((raw, total / self.history.len() as i64))
        };
        let maximum = i64::from(ramp.maximum_temperature) * 1000;
        let minimum = i64::from(ramp.minimum_temperature) * 1000;
        self.channels
            .iter_mut()
            .zip(readings)
            .map(|(channel, reading)| {
                let target = if self.config.case_fans.channels.contains(&channel.id) {
                    let (raw, average) = case_values.context("Missing case airflow demand")?;
                    if raw >= maximum {
                        pwm(ramp.maximum_duty)
                    } else {
                        interpolate(average, ramp, pwm(self.config.case_fans.minimum_duty))
                    }
                } else if sample.cpu >= maximum || cpu_average >= maximum {
                    pwm(ramp.maximum_duty)
                } else if cpu_average <= minimum
                    || (channel.requested == 0
                        && cpu_average < minimum + i64::from(self.config.start_hysteresis) * 1000)
                {
                    0
                } else {
                    interpolate(cpu_average, ramp, pwm(self.config.minimum_duty))
                };
                if target == 0 || reading.rpm > 0 {
                    channel.stalled_since = None;
                } else {
                    let since = *channel.stalled_since.get_or_insert(now);
                    ensure!(
                        now.duration_since(since) < STALL_LIMIT,
                        "Fan {} is stalled at requested PWM {target}",
                        channel.id
                    );
                }
                if target > 0 && channel.requested == 0 {
                    channel.kick_until = Some(now + SPIN_UP);
                }
                channel.requested = target;
                if target == 0 {
                    channel.kick_until = None;
                }
                let starting = channel.kick_until.is_some_and(|until| now < until);
                Ok(if target > 0 && (starting || reading.rpm == 0) {
                    target.max(pwm(self.config.start_duty))
                } else {
                    target
                })
            })
            .collect()
    }
}

struct Events {
    profile: Rc<Cell<bool>>,
    resume: Rc<Cell<bool>>,
}
fn subscribe(connection: &LocalConnection) -> Result<Events> {
    let events = Events {
        profile: Rc::new(Cell::new(false)),
        resume: Rc::new(Cell::new(false)),
    };
    let changed = events.profile.clone();
    connection.add_match(
        MatchRule::new_signal("org.freedesktop.DBus.Properties", "PropertiesChanged")
            .with_sender(SERVICE)
            .with_path(OBJECT),
        move |event: PropertiesPropertiesChanged, _, _| {
            if event.interface_name == SERVICE
                && (event.changed_properties.contains_key("ActiveProfile")
                    || event
                        .invalidated_properties
                        .iter()
                        .any(|name| name == "ActiveProfile"))
            {
                changed.set(true);
            }
            true
        },
    )?;
    let changed = events.profile.clone();
    connection.add_match(
        MatchRule::new_signal("org.freedesktop.DBus", "NameOwnerChanged")
            .with_sender("org.freedesktop.DBus")
            .with_path("/org/freedesktop/DBus"),
        move |(name, _, _): (String, String, String), _, _| {
            if name == SERVICE {
                changed.set(true);
            }
            true
        },
    )?;
    let resumed = events.resume.clone();
    connection.add_match(
        MatchRule::new_signal("org.freedesktop.login1.Manager", "PrepareForSleep")
            .with_sender("org.freedesktop.login1")
            .with_path("/org/freedesktop/login1"),
        move |(sleeping,): (bool,), _, _| {
            if !sleeping {
                resumed.set(true);
            }
            true
        },
    )?;
    Ok(events)
}
fn active_profile(connection: &LocalConnection) -> Result<String> {
    connection
        .with_proxy(SERVICE, OBJECT, Duration::from_secs(2))
        .get(SERVICE, "ActiveProfile")
        .context("Read active power profile")
}

extern "C" fn signal(number: libc::c_int) {
    if number == libc::SIGHUP {
        RELOAD.store(true, Ordering::Relaxed);
    } else {
        STOP.store(true, Ordering::Relaxed);
    }
}
struct Signals(Vec<(libc::c_int, libc::sigaction)>);
impl Signals {
    fn install() -> Result<Self> {
        STOP.store(false, Ordering::Relaxed);
        RELOAD.store(false, Ordering::Relaxed);
        let mut signals = Self(Vec::new());
        for number in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = signal as *const () as usize;
            let mut previous = unsafe { std::mem::zeroed() };
            if unsafe { libc::sigaction(number, &action, &mut previous) } != 0 {
                return Err(std::io::Error::last_os_error()).context("Install fan signal handler");
            }
            signals.0.push((number, previous));
        }
        Ok(signals)
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        for (number, action) in &self.0 {
            if unsafe { libc::sigaction(*number, action, std::ptr::null_mut()) } != 0 {
                eprintln!(
                    "Restore fan signal handler {number}: {}",
                    std::io::Error::last_os_error()
                );
            }
        }
    }
}
struct Systemd(Option<UnixDatagram>);
impl Systemd {
    fn connect() -> Result<Self> {
        let Some(path) = std::env::var_os("NOTIFY_SOCKET") else {
            return Ok(Self(None));
        };
        let address = match path.as_bytes().strip_prefix(b"@") {
            Some(name) => SocketAddr::from_abstract_name(name)?,
            None => SocketAddr::from_pathname(path)?,
        };
        let socket = UnixDatagram::unbound()?;
        socket
            .connect_addr(&address)
            .context("Connect fan controller to systemd")?;
        Ok(Self(Some(socket)))
    }

    fn notify(&self, message: &str) -> Result<()> {
        if let Some(socket) = &self.0 {
            socket
                .send(message.as_bytes())
                .context("Notify systemd fan controller")?;
        }
        Ok(())
    }

    fn reloading(&self) -> Result<()> {
        let mut time: libc::timespec = unsafe { std::mem::zeroed() };
        ensure!(
            unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) } == 0,
            "Read monotonic clock"
        );
        let micros = time.tv_sec as u64 * 1_000_000 + time.tv_nsec as u64 / 1000;
        self.notify(&format!("RELOADING=1\nMONOTONIC_USEC={micros}"))
    }
}
#[derive(Serialize)]
struct Status<'a> {
    profile: &'a str,
    cpu_temperature: i64,
    gpu_temperature: Option<i64>,
    channels: &'a [u8],
    pwm: &'a [u16],
    rpm: Vec<u32>,
}

fn daemon(hardware: Hardware, path: &Path, status: Option<&Path>) -> Result<()> {
    let _signals = Signals::install()?;
    // Acquire recovery responsibility before parsing curves or contacting D-Bus.
    let mut fans = Fans::discover(hardware)?;
    let systemd = Systemd::connect()?;
    let config = Config::load(path, &fans.hardware)?;
    let mut sensors = Sensors::discover(&mut fans, &config)?;
    let connection = LocalConnection::new_system()?;
    let events = subscribe(&connection)?;
    let profile = active_profile(&connection)?;
    let mut control = Control::new(config, profile, &fans.hardware, &fans.readings()?)?;
    eprintln!("Rust fan controller using {} profile", control.profile);
    let mut next_sample = Instant::now();
    let mut ready = false;
    loop {
        if STOP.load(Ordering::Relaxed) {
            return Ok(());
        }
        let reload = RELOAD.swap(false, Ordering::Relaxed);
        let resume = events.resume.replace(false);
        let changed = events.profile.replace(false);
        if reload {
            systemd.reloading()?;
            ready = false;
        }
        if reload || resume {
            let config = Config::load(path, &fans.hardware)?;
            sensors = Sensors::discover(&mut fans, &config)?;
            control.config = config;
            if resume {
                eprintln!("Reinitialized fan sensors after resume");
            }
        }
        if reload || resume || changed {
            control.change_profile(active_profile(&connection)?)?;
            next_sample = Instant::now();
        }
        let now = Instant::now();
        if now >= next_sample {
            // Validate every input before changing any fan output.
            let sample = sensors.sample()?;
            let readings = fans.readings()?;
            let targets = control.targets(sample, &readings, now)?;
            fans.apply(&targets, &readings)?;
            if let Some(path) = status {
                util::atomic_write(
                    path,
                    &serde_json::to_vec(&Status {
                        profile: &control.profile,
                        cpu_temperature: sample.cpu,
                        gpu_temperature: sample.gpu,
                        channels: &fans.hardware.channels,
                        pwm: &targets,
                        rpm: readings.iter().map(|reading| reading.rpm).collect(),
                    })?,
                )?;
            }
            if !ready {
                systemd.notify("READY=1")?;
                ready = true;
            }
            systemd.notify("WATCHDOG=1")?;
            next_sample = Instant::now() + Duration::from_secs(u64::from(control.config.interval));
        }
        if let Err(error) =
            connection.process(next_sample.saturating_duration_since(Instant::now()))
        {
            if !STOP.load(Ordering::Relaxed) && !RELOAD.load(Ordering::Relaxed) {
                return Err(error).context("Process fan-controller D-Bus events");
            }
        }
    }
}
pub fn run(operation: Operation) -> Result<()> {
    match operation {
        Operation::Run {
            hardware,
            config,
            status,
        } => daemon(hardware, &config, status.as_deref()),
        Operation::Automatic { hardware } => {
            let controller = hardware.discover()?;
            automatic(&hardware, &controller)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> (tempfile::TempDir, Hardware, Config) {
        let root = tempfile::tempdir().unwrap();
        let hardware = Hardware {
            controller: "nct6798".into(),
            channels: vec![1, 2, 3, 4],
            hwmon_root: root.path().into(),
        };
        let path = root.path().join("config");
        fs::write(&path, include_bytes!("../tests/fixtures/fan-control.json")).unwrap();
        let config = Config::load(&path, &hardware).unwrap();
        (root, hardware, config)
    }
    fn sample(cpu: i64, gpu: i64) -> Sample {
        Sample {
            cpu: cpu * 1000,
            gpu: Some(gpu * 1000),
        }
    }

    fn feedback(pwm: u16, rpm: u32) -> [Reading; 4] {
        [Reading {
            pwm,
            rpm,
            manual: true,
        }; 4]
    }

    #[test]
    fn hysteresis_gpu_demand_and_raw_emergency_bypass() {
        let (_root, hardware, mut config) = fixture();
        config.average = 1;
        let readings = feedback(0, 600);
        let mut control = Control::new(config, "performance".into(), &hardware, &readings).unwrap();
        let start = Instant::now();
        for (seconds, cpu, gpu, expected) in [
            (0, 60, 45, vec![77, 0, 77, 77]),
            (1, 67, 45, vec![100, 0, 100, 100]),
            (2, 68, 45, vec![112, 91, 112, 112]),
            (4, 67, 45, vec![100, 78, 100, 100]),
            (5, 65, 45, vec![77, 0, 77, 77]),
            (6, 55, 80, vec![255, 0, 255, 255]),
            (7, 80, 40, vec![255, 255, 255, 255]),
        ] {
            assert_eq!(
                control
                    .targets(
                        sample(cpu, gpu),
                        &readings,
                        start + Duration::from_secs(seconds)
                    )
                    .unwrap(),
                expected
            );
        }
        control.config.average = 3;
        control.history.clear();
        for seconds in 8..11 {
            control
                .targets(
                    sample(50, 40),
                    &readings,
                    start + Duration::from_secs(seconds),
                )
                .unwrap();
        }
        assert_eq!(
            control
                .targets(sample(85, 40), &readings, start + Duration::from_secs(11))
                .unwrap(),
            vec![255; 4]
        );
        for seconds in 12..15 {
            control
                .targets(
                    sample(50, 40),
                    &readings,
                    start + Duration::from_secs(seconds),
                )
                .unwrap();
        }
        assert_eq!(
            control
                .targets(sample(50, 80), &readings, start + Duration::from_secs(15))
                .unwrap(),
            vec![255, 0, 255, 255]
        );
    }
    #[test]
    fn profile_changes_reset_averaging_without_reinitializing_channels() {
        let (_root, hardware, config) = fixture();
        let readings = feedback(0, 600);
        let mut control = Control::new(config, "power-saver".into(), &hardware, &readings).unwrap();
        let now = Instant::now();
        for _ in 0..3 {
            assert_eq!(
                control.targets(sample(55, 50), &readings, now).unwrap(),
                vec![77, 0, 77, 77]
            );
        }
        control.change_profile("performance".into()).unwrap();
        assert_eq!(
            control.targets(sample(66, 45), &readings, now).unwrap(),
            vec![88, 0, 88, 88]
        );
        assert!(control.change_profile("unknown".into()).is_err());
    }
    #[test]
    fn fan_start_does_not_block_other_outputs_and_a_stall_fails() {
        let (_root, hardware, mut config) = fixture();
        config.average = 1;
        config.start_hysteresis = 1;
        let readings = feedback(0, 0);
        let mut control = Control::new(config, "balanced".into(), &hardware, &readings).unwrap();
        let now = Instant::now();
        // Balanced's initial CPU duty is below 30%; the start kick raises it.
        assert_eq!(
            control.targets(sample(66, 45), &readings, now).unwrap()[1],
            77
        );
        let spinning = feedback(77, 500);
        assert!(
            control
                .targets(sample(66, 45), &spinning, now + Duration::from_millis(500))
                .unwrap()[1]
                >= 77
        );
        assert_eq!(
            control
                .targets(sample(66, 45), &spinning, now + Duration::from_secs(2))
                .unwrap()[1],
            61
        );
        let mut stalled = spinning;
        stalled[0].rpm = 0;
        control
            .targets(sample(66, 45), &stalled, now + Duration::from_secs(3))
            .unwrap();
        assert!(
            control
                .targets(sample(66, 45), &stalled, now + Duration::from_secs(8))
                .unwrap_err()
                .to_string()
                .contains("stalled")
        );
    }
    #[test]
    fn invalid_configuration_and_sensor_values_fail_explicitly() {
        let (root, hardware, _) = fixture();
        let path = root.path().join("config");
        let good: serde_json::Value = serde_json::from_str(&util::read(&path).unwrap()).unwrap();
        for (key, value) in [
            ("start_hysteresis", json!(0)),
            ("average", json!(0)),
            ("interval", json!(0)),
            (
                "case_fans",
                json!({"channels":[1,1],"minimum_duty":30,"gpu":{"minimum_temperature":50,"maximum_temperature":80}}),
            ),
            (
                "case_fans",
                json!({"channels":[5],"minimum_duty":30,"gpu":{"minimum_temperature":50,"maximum_temperature":80}}),
            ),
            (
                "case_fans",
                json!({"channels":[1],"minimum_duty":20,"gpu":{"minimum_temperature":50,"maximum_temperature":80}}),
            ),
        ] {
            let mut bad = good.clone();
            bad[key] = value;
            fs::write(&path, bad.to_string()).unwrap();
            assert!(Config::load(&path, &hardware).is_err());
        }
        for value in ["garbage", "0", "-1", "121000"] {
            fs::write(&path, value).unwrap();
            assert!(read_temperature(&path).is_err());
        }
        fs::remove_file(&path).unwrap();
        assert!(read_temperature(&path).is_err());
    }
    #[test]
    fn recovery_attempts_every_channel_even_when_one_write_fails() {
        let (root, hardware, _) = fixture();
        for channel in [2, 3, 4] {
            fs::write(root.path().join(format!("pwm{channel}_enable")), "1").unwrap();
        }
        fs::create_dir(root.path().join("pwm1_enable")).unwrap();
        assert!(automatic(&hardware, root.path()).is_err());
        for channel in [2, 3, 4] {
            assert_eq!(
                util::read(root.path().join(format!("pwm{channel}_enable")))
                    .unwrap()
                    .trim(),
                "5"
            );
        }
    }
}
