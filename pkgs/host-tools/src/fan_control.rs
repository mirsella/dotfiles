use crate::util;
use anyhow::{Context, Result, ensure};
use clap::{Args, Subcommand};
use dbus::blocking::{
    LocalConnection,
    stdintf::org_freedesktop_dbus::{Properties, PropertiesPropertiesChanged},
};
use dbus::message::MatchRule;
use serde::Deserialize;
use std::{
    cell::Cell,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

const SERVICE: &str = "org.freedesktop.UPower.PowerProfiles";
const OBJECT: &str = "/org/freedesktop/UPower/PowerProfiles";

#[derive(Subcommand)]
pub enum Operation {
    Prepare {
        #[command(flatten)]
        hardware: Hardware,
        config: PathBuf,
        output: PathBuf,
    },
    FullSpeed {
        #[command(flatten)]
        hardware: Hardware,
    },
    Watch,
}

#[derive(Args)]
pub struct Hardware {
    #[arg(long)]
    controller: String,
    #[arg(long, value_delimiter = ',', required = true)]
    channels: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    minimum_duty: u8,
    start_duty: u8,
    interval: u8,
    average: u8,
    profiles: std::collections::BTreeMap<String, Ramp>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ramp {
    minimum_temperature: u8,
    maximum_temperature: u8,
    maximum_duty: u8,
}

impl Hardware {
    fn validate(&self) -> Result<()> {
        ensure!(!self.controller.is_empty(), "Fan controller name is empty");
        ensure!(!self.channels.is_empty(), "No fan channels configured");
        for (index, channel) in self.channels.iter().enumerate() {
            ensure!((1..=6).contains(channel), "Invalid fan channel {channel}");
            ensure!(
                !self.channels[..index].contains(channel),
                "Duplicate fan channel {channel}"
            );
        }
        Ok(())
    }
}

impl Config {
    fn load(path: &Path) -> Result<Self> {
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
                    && ramp.maximum_temperature <= 100,
                "Invalid {name} temperature range"
            );
            ensure!(
                config.minimum_duty < ramp.maximum_duty
                    && config.start_duty <= ramp.maximum_duty
                    && ramp.maximum_duty <= 100,
                "Invalid {name} fan maximum/start duty"
            );
        }
        Ok(config)
    }
}

fn hwmon_devices() -> Result<Vec<(PathBuf, String)>> {
    fs::read_dir("/sys/class/hwmon")?
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

fn full_speed(hardware: &Hardware, controller: &Path) -> Result<()> {
    // NCT6798 mode 0 disables regulation and drives full speed, not zero duty.
    let mut errors = Vec::new();
    for channel in &hardware.channels {
        let path = controller.join(format!("pwm{channel}_enable"));
        if let Err(error) = fs::write(&path, "0\n") {
            errors.push(format!("{}: {error}", path.display()));
        }
    }
    ensure!(
        errors.is_empty(),
        "Could not set full-speed fan control: {}",
        errors.join("; ")
    );
    Ok(())
}

fn pwm(duty: u8) -> u16 {
    (u16::from(duty) * 255 + 50) / 100
}

fn fancontrol_config(
    hardware: &Hardware,
    config: &Config,
    profile: &str,
    controller: &Path,
    sensor: &Path,
) -> Result<String> {
    let ramp = config
        .profiles
        .get(profile)
        .with_context(|| format!("Unsupported power profile {profile:?}"))?;
    let controller = util::path(controller)?;
    let sensor = util::path(sensor)?;
    ensure!(
        !controller.chars().any(char::is_whitespace) && !sensor.chars().any(char::is_whitespace),
        "fancontrol paths contain whitespace"
    );
    let mut contents = format!(
        "# Generated for power profile: {profile}\nINTERVAL={}\n",
        config.interval
    );
    let mut setting = |key: &str, value: &dyn Fn(&mut String, u8)| {
        write!(contents, "{key}=").unwrap();
        for (index, &channel) in hardware.channels.iter().enumerate() {
            if index > 0 {
                contents.push(' ');
            }
            write!(contents, "{controller}/pwm{channel}=").unwrap();
            value(&mut contents, channel);
        }
        contents.push('\n');
    };
    setting("FCTEMPS", &|out, _| {
        write!(out, "{sensor}/temp1_input").unwrap()
    });
    setting("FCFANS", &|out, n| {
        write!(out, "{controller}/fan{n}_input").unwrap()
    });
    for (key, value) in [
        ("MINTEMP", u16::from(ramp.minimum_temperature)),
        ("MAXTEMP", u16::from(ramp.maximum_temperature)),
        ("MINSTART", pwm(config.start_duty)),
        ("MINSTOP", pwm(config.minimum_duty)),
        ("MINPWM", pwm(config.minimum_duty)),
        ("MAXPWM", pwm(ramp.maximum_duty)),
        ("AVERAGE", u16::from(config.average)),
    ] {
        setting(key, &|out, _| write!(out, "{value}").unwrap());
    }
    Ok(contents)
}

fn watch() -> Result<()> {
    let connection = LocalConnection::new_system()?;
    let changed = Rc::new(Cell::new(true));
    let flag = changed.clone();
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
                flag.set(true);
            }
            true
        },
    )?;
    let flag = changed.clone();
    connection.add_match(
        MatchRule::new_signal("org.freedesktop.DBus", "NameOwnerChanged")
            .with_sender("org.freedesktop.DBus")
            .with_path("/org/freedesktop/DBus"),
        move |(name, _, new_owner): (String, String, String), _, _| {
            if name == SERVICE && !new_owner.is_empty() {
                flag.set(true);
            }
            true
        },
    )?;
    let flag = changed.clone();
    connection.add_match(
        MatchRule::new_signal("org.freedesktop.login1.Manager", "PrepareForSleep")
            .with_sender("org.freedesktop.login1")
            .with_path("/org/freedesktop/login1"),
        move |(sleeping,): (bool,), _, _| {
            if !sleeping {
                flag.set(true);
            }
            true
        },
    )?;
    loop {
        if changed.replace(false) {
            util::run("systemctl", &["try-restart", "fancontrol.service"])?;
        }
        connection.process(Duration::from_secs(86400))?;
    }
}

pub fn run(operation: Operation) -> Result<()> {
    let (hardware, preparation) = match operation {
        Operation::Prepare {
            hardware,
            config,
            output,
        } => (hardware, Some((config, output))),
        Operation::FullSpeed { hardware } => (hardware, None),
        Operation::Watch => return watch(),
    };
    hardware.validate()?;
    let devices = hwmon_devices()?;
    let controller = unique_device(
        &devices,
        |name| name == hardware.controller,
        "configured fan controller",
    )?;
    full_speed(&hardware, &controller)?;
    if let Some((path, output)) = preparation {
        let config = Config::load(&path)?;
        let sensor = unique_device(
            &devices,
            |name| matches!(name, "zenpower" | "k10temp"),
            "Ryzen temperature provider",
        )?;
        let label = util::read(sensor.join("temp1_label"))?;
        ensure!(
            matches!(label.trim(), "Tdie" | "Tctl"),
            "Unexpected Ryzen temperature label {label:?}"
        );
        let temperature: u32 = util::read(sensor.join("temp1_input"))?.trim().parse()?;
        ensure!(
            (1000..=120000).contains(&temperature),
            "Invalid CPU temperature {temperature}"
        );
        let connection = LocalConnection::new_system()?;
        let profile: String = connection
            .with_proxy(SERVICE, OBJECT, Duration::from_secs(5))
            .get(SERVICE, "ActiveProfile")
            .context("Read active power profile")?;
        let contents = fancontrol_config(&hardware, &config, &profile, &controller, &sensor)?;
        util::atomic_write(&output, contents.as_bytes())?;
        eprintln!(
            "Prepared {profile} fan ramp; {} uses {} ({})",
            controller.display(),
            sensor.display(),
            label.trim()
        );
    } else {
        eprintln!(
            "Fancontrol stopped; set {} fan channels to full speed",
            hardware.channels.len()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hardware() -> Hardware {
        Hardware {
            controller: "nct6798".into(),
            channels: vec![1, 2, 3, 4],
        }
    }

    fn config() -> Config {
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(file.path(), config_json().to_string()).unwrap();
        Config::load(file.path()).unwrap()
    }

    fn config_json() -> serde_json::Value {
        serde_json::json!({
            "minimum_duty": 20, "start_duty": 30, "interval": 3, "average": 3,
            "profiles": {
                "power-saver": { "minimum_temperature": 60, "maximum_temperature": 90, "maximum_duty": 40 },
                "balanced": { "minimum_temperature": 50, "maximum_temperature": 90, "maximum_duty": 80 },
                "performance": { "minimum_temperature": 45, "maximum_temperature": 80, "maximum_duty": 100 }
            }
        })
    }

    #[test]
    fn renders_all_profiles_using_stable_device_paths_and_exact_caps() {
        let config = config();
        for (name, maximum) in [
            ("power-saver", 102),
            ("balanced", 204),
            ("performance", 255),
        ] {
            let contents = fancontrol_config(
                &hardware(),
                &config,
                name,
                Path::new("/sys/devices/controller/hwmon/hwmon8"),
                Path::new("/sys/devices/cpu/hwmon/hwmon2"),
            )
            .unwrap();
            assert!(contents.contains("/sys/devices/controller/hwmon/hwmon8/pwm4=/sys/devices/cpu/hwmon/hwmon2/temp1_input"));
            assert!(contents.contains(&format!(
                "MAXPWM=/sys/devices/controller/hwmon/hwmon8/pwm1={maximum}"
            )));
            assert!(contents.contains("MINPWM=/sys/devices/controller/hwmon/hwmon8/pwm1=51"));
            assert!(!contents.contains("pwm5"));
        }
        assert!(
            fancontrol_config(
                &hardware(),
                &config,
                "unknown",
                Path::new("/controller"),
                Path::new("/sensor")
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_invalid_ramps_and_hardware_arguments() {
        let mut value = config_json();
        value["start_duty"] = 50.into();
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(file.path(), value.to_string()).unwrap();
        assert!(Config::load(file.path()).is_err());
        let mut hardware = hardware();
        hardware.channels = vec![1, 1];
        assert!(hardware.validate().is_err());
    }

    #[test]
    fn full_speed_needs_only_hardware_arguments() {
        use clap::Parser;

        #[derive(Parser)]
        struct Cli {
            #[command(subcommand)]
            operation: Operation,
        }

        let cli = Cli::try_parse_from([
            "fan-control",
            "full-speed",
            "--controller",
            "nct6798",
            "--channels",
            "1,2,3,4",
        ])
        .unwrap();
        let Operation::FullSpeed { hardware } = cli.operation else {
            panic!("Expected full-speed operation");
        };
        hardware.validate().unwrap();
        assert_eq!(hardware.channels, [1, 2, 3, 4]);
    }

    #[test]
    fn fail_safe_attempts_remaining_fans_after_a_channel_write_fails() {
        let directory = tempfile::tempdir().unwrap();
        for n in [1, 3, 4] {
            fs::write(directory.path().join(format!("pwm{n}_enable")), "1").unwrap();
        }
        fs::create_dir(directory.path().join("pwm2_enable")).unwrap();
        assert!(full_speed(&hardware(), directory.path()).is_err());
        for n in [1, 3, 4] {
            assert_eq!(
                fs::read_to_string(directory.path().join(format!("pwm{n}_enable"))).unwrap(),
                "0\n"
            );
        }
    }
}
