use std::{error::Error, fs, path::Path};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn invalid(message: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message)
}

fn read(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(path)?.trim().to_owned())
}

fn number(path: &Path) -> Result<f64> {
    read(path)?.parse().map_err(Into::into)
}

struct Battery {
    discharging: bool,
    watts: f64,
}

fn battery(sys_power_supply: &Path) -> Result<Option<Battery>> {
    for entry in fs::read_dir(sys_power_supply)? {
        let dir = entry?.path();
        if !read(&dir.join("type")).is_ok_and(|kind| kind == "Battery") {
            continue;
        }
        // BAT1 on the Framework exposes current_now but not power_now. Other
        // batteries do the opposite, so accept either.
        let watts = match number(&dir.join("power_now")) {
            Ok(microwatts) => microwatts.abs() / 1e6,
            Err(_) => {
                let volts = number(&dir.join("voltage_now"))? / 1e6;
                let amps = number(&dir.join("current_now"))? / 1e6;
                (volts * amps).abs()
            }
        };
        return Ok(Some(Battery {
            discharging: read(&dir.join("status"))? == "Discharging",
            watts,
        }));
    }
    Ok(None)
}

fn amdgpu_watts(sys_hwmon: &Path) -> Result<f64> {
    let mut found = false;
    for entry in fs::read_dir(sys_hwmon)? {
        let dir = entry?.path();
        if !read(&dir.join("name")).is_ok_and(|name| name == "amdgpu") {
            continue;
        }
        found = true;
        // power1_average is the SMU's averaged PPT, which matches the RAPL
        // package domain on APUs; power1_input is the fallback instant value.
        for attribute in ["power1_average", "power1_input"] {
            if let Ok(microwatts) = number(&dir.join(attribute)) {
                return Ok(microwatts / 1e6);
            }
        }
    }
    Err(if found {
        invalid("amdgpu hwmon has no power sensor")
    } else {
        invalid("no amdgpu hwmon")
    }
    .into())
}

fn round(watts: f64) -> f64 {
    (watts * 100.0).round() / 100.0
}

fn summary(sys_power_supply: &Path, sys_hwmon: &Path) -> Result<String> {
    // A desktop has no battery, so an amdgpu sensor there would not be an APU
    // package reading.
    let (package, total) = match battery(sys_power_supply)? {
        Some(battery) => (
            Some(amdgpu_watts(sys_hwmon)?),
            battery.discharging.then_some(battery.watts),
        ),
        None => (None, None),
    };
    Ok(serde_json::json!({
        "packageWatts": package.map(round),
        "totalWatts": total.map(round),
    })
    .to_string())
}

fn main() {
    match summary(
        Path::new("/sys/class/power_supply"),
        Path::new("/sys/class/hwmon"),
    ) {
        Ok(text) => println!("{text}"),
        Err(error) => {
            eprintln!("Power counters unavailable: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn write(path: &Path, text: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)?;
        Ok(())
    }

    struct Sysfs {
        root: tempfile::TempDir,
    }

    impl Sysfs {
        fn new() -> Result<Self> {
            let root = tempfile::tempdir()?;
            write(&root.path().join("hwmon/hwmon4/name"), "amdgpu\n")?;
            write(
                &root.path().join("hwmon/hwmon4/power1_average"),
                "29551000\n",
            )?;
            fs::create_dir_all(root.path().join("power_supply"))?;
            Ok(Self { root })
        }

        fn power_supply(&self) -> PathBuf {
            self.root.path().join("power_supply")
        }

        fn hwmon(&self) -> PathBuf {
            self.root.path().join("hwmon")
        }

        fn battery(&self, status: &str, voltage: &str, current: &str) -> Result<()> {
            let dir = self.power_supply().join("BAT1");
            write(&dir.join("type"), "Battery\n")?;
            write(&dir.join("status"), &format!("{status}\n"))?;
            write(&dir.join("voltage_now"), &format!("{voltage}\n"))?;
            write(&dir.join("current_now"), &format!("{current}\n"))?;
            Ok(())
        }
    }

    fn values(text: &str) -> serde_json::Value {
        serde_json::from_str(text).expect("summary should be valid JSON")
    }

    #[test]
    fn reports_package_power_and_discharge_total_on_battery() -> Result<()> {
        let sysfs = Sysfs::new()?;
        // 15.87 V at 3.0 A discharge = 47.61 W.
        sysfs.battery("Discharging", "15870000", "3000000")?;
        let output = values(&summary(&sysfs.power_supply(), &sysfs.hwmon())?);
        assert_eq!(output["packageWatts"], 29.55);
        assert_eq!(output["totalWatts"], 47.61);
        Ok(())
    }

    #[test]
    fn only_package_power_is_measurable_on_ac() -> Result<()> {
        let sysfs = Sysfs::new()?;
        sysfs.battery("Not charging", "15870000", "1000")?;
        let output = values(&summary(&sysfs.power_supply(), &sysfs.hwmon())?);
        assert_eq!(output["packageWatts"], 29.55);
        assert_eq!(output["totalWatts"], serde_json::Value::Null);
        Ok(())
    }

    #[test]
    fn a_machine_without_a_battery_reports_no_power() -> Result<()> {
        let sysfs = Sysfs::new()?;
        let output = values(&summary(&sysfs.power_supply(), &sysfs.hwmon())?);
        assert_eq!(output["packageWatts"], serde_json::Value::Null);
        assert_eq!(output["totalWatts"], serde_json::Value::Null);
        Ok(())
    }

    #[test]
    fn a_battery_without_a_gpu_power_sensor_is_an_error() -> Result<()> {
        let sysfs = Sysfs::new()?;
        sysfs.battery("Discharging", "15870000", "3000000")?;
        fs::remove_file(sysfs.hwmon().join("hwmon4/power1_average"))?;
        assert!(summary(&sysfs.power_supply(), &sysfs.hwmon()).is_err());
        Ok(())
    }
}
