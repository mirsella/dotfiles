//! Strict, first-suspend, untraced Q/M/H trials. Legacy plans retain their rules.
use super::*;
use std::{collections::BTreeMap, os::unix::fs::MetadataExt};

struct Plan {
    values: BTreeMap<String, String>,
    due: u64,
    wake: u64,
    minimum: u64,
    maximum: u64,
}

impl Plan {
    fn get(&self, key: &str) -> &str {
        &self.values[key]
    }

    fn parse(data: &str) -> Result<Self> {
        let required = [
            "schema_version",
            "trial_id",
            "profile",
            "boot",
            "generation",
            "kernel",
            "modules",
            "runtime",
            "due",
            "wake",
            "pm_async",
            "pm_trace",
            "recorder",
            "mem_sleep",
            "pm_test",
            "tpm",
            "ec",
            "logged_in_users",
            "ordinal",
            "daytime_start",
            "daytime_end",
        ];
        let mut values = BTreeMap::new();
        for line in data.lines() {
            let (key, value) = line.split_once('=').ok_or("malformed trial line")?;
            check(
                required.contains(&key) && !value.is_empty(),
                format!("unknown/empty trial field: {key}"),
            )?;
            check(
                values.insert(key.to_owned(), value.to_owned()).is_none(),
                format!("duplicate trial field: {key}"),
            )?;
        }
        for key in required {
            check(
                values.contains_key(key),
                format!("missing trial field: {key}"),
            )?;
        }
        for (key, expected) in [
            ("schema_version", "1"),
            ("pm_trace", "0"),
            ("recorder", "off"),
            ("mem_sleep", "deep"),
            ("pm_test", "none"),
            ("tpm", "initialized"),
            ("ec", "not-run"),
            ("ordinal", "1"),
        ] {
            check(
                values[key] == expected,
                format!("unsupported {key}: {}", values[key]),
            )?;
        }
        check(
            matches!(values["pm_async"].as_str(), "0" | "1"),
            "invalid pm_async",
        )?;
        LoginPolicy::from_plan(data)?;
        let id = &values["trial_id"];
        check(
            !id.is_empty()
                && id.len() <= 64
                && id
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'),
            "invalid trial ID",
        )?;
        let boot = &values["boot"];
        check(
            boot.len() == 36
                && boot.bytes().enumerate().all(|(i, c)| {
                    if [8, 13, 18, 23].contains(&i) {
                        c == b'-'
                    } else {
                        c.is_ascii_hexdigit()
                    }
                }),
            "invalid boot ID",
        )?;
        for key in ["generation", "kernel", "modules", "runtime"] {
            check(
                values[key].starts_with("/nix/store/")
                    && values[key]
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"/._+-".contains(&c)),
                format!("invalid store path: {key}"),
            )?;
        }
        let (minimum, maximum, length) = match values["profile"].as_str() {
            "Q" => (1800, 2400, 180),
            "M" => (10800, 11700, 180),
            "H" => (28800, 29700, 900),
            _ => return Err("only first-suspend Q/M/H profiles are implemented".into()),
        };
        let due: u64 = values["due"].parse()?;
        let wake: u64 = values["wake"].parse()?;
        check(
            wake.checked_sub(due) == Some(length),
            "profile sleep duration mismatch",
        )?;
        let start: u64 = values["daytime_start"].parse()?;
        let end: u64 = values["daytime_end"].parse()?;
        check(
            start <= due
                && wake.checked_add(180).is_some_and(|last| last <= end)
                && end.checked_sub(start).is_some_and(|length| length <= 43200),
            "attempt exceeds attended daytime window",
        )?;
        Ok(Self {
            values,
            due,
            wake,
            minimum,
            maximum,
        })
    }

    fn identity(&self) -> Result<()> {
        check(
            text("/proc/sys/kernel/random/boot_id")? == self.get("boot"),
            "trial boot changed",
        )?;
        for (path, expected) in [
            ("/run/booted-system", "generation"),
            ("/run/current-system", "generation"),
            ("/run/booted-system/kernel", "kernel"),
            ("/run/booted-system/kernel-modules", "modules"),
        ] {
            check(
                fs::canonicalize(path)? == Path::new(self.get(expected)),
                format!("trial identity changed: {path}"),
            )?;
        }
        check(
            env::current_exe()? == Path::new(self.get("runtime")),
            "trial runtime changed",
        )?;
        Ok(())
    }

    fn age(&self, awake: u64, epoch: u64, execute: bool) -> Result<()> {
        let age = if execute {
            check(
                (self.due..=self.due + 30).contains(&epoch),
                "outside 30-second submission window",
            )?;
            awake
        } else {
            check(epoch < self.due, "trial is no longer in the future")?;
            awake.checked_add(self.due - epoch).ok_or("age overflow")?
        };
        check(
            (self.minimum..=self.maximum).contains(&age),
            format!("awake age {age}s outside profile {}", self.get("profile")),
        )
    }

    fn save(&self, suffix: &str, contents: &str) -> Result<()> {
        save(&format!("{}-{suffix}.txt", self.get("trial_id")), contents)
    }

    fn preflight(&self, execute: bool) -> Result<Baseline> {
        self.identity()?;
        let before = preflight_pm(
            self.get("boot"),
            TpmState::Initialized,
            EcState::NotRun,
            self.get("pm_async"),
        )?;
        check(
            before.successes == 0 && before.markers == 0,
            "first-suspend trial has previous cycles/markers",
        )?;
        self.age(before.awake_seconds, now()?, execute)?;
        check(
            output(
                "systemctl",
                &[
                    "show",
                    "predator-pm-next.timer",
                    "-p",
                    "ActiveState",
                    "--value",
                ],
            )? == "inactive",
            "another study timer is active",
        )?;
        check(
            output(
                "nmcli",
                &["-g", "GENERAL.CON-UUID", "device", "show", "enp3s0f1"],
            )? == "c4e6863b-7f76-4a4d-9a48-9937af0b5a36",
            "declared LAN profile is not active",
        )?;
        check(
            !output("nmcli", &["-g", "IP4.DNS", "device", "show", "enp3s0f1"])?.is_empty(),
            "LAN DNS missing",
        )?;
        maintenance()?;
        Ok(before)
    }
}

// Explicit cleanup also runs when the operation fails. Never rely on unwinding
// alone for restoring a sysfs setting, and report both failures if necessary.
fn with_pm<T>(path: &Path, selected: &str, action: impl FnOnce() -> Result<T>) -> Result<T> {
    let previous = text(path)?;
    check(previous == "1", "expected ordinary pm_async=1 before trial")?;
    let outcome = (|| {
        fs::write(path, selected)?;
        check(text(path)? == selected, "PM setting readback mismatch")?;
        action()
    })();
    let cleanup = (|| {
        fs::write(path, &previous)?;
        check(text(path)? == previous, "PM restoration readback mismatch")
    })();
    match (outcome, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (result, Err(error)) => Err(format!(
            "PM restoration failed: {error}; operation error={:?}",
            result.err()
        )
        .into()),
    }
}

fn captured(program: &str, args: &[&str]) -> String {
    match output(program, args) {
        Ok(value) => value,
        Err(error) => format!("CAPTURE_ERROR: {error}"),
    }
}

fn raw_return(plan: &Plan, returned: u64, result: &Result<()>) -> Result<()> {
    // Persist independently of clock restoration and the eventual verdict.
    plan.save("return", &format!("observed_epoch={returned}\nboot={}\nrequest_result={result:?}\nuptime={}\nsuccess={}\nfail={}\nmarkers={}\n",
        text("/proc/sys/kernel/random/boot_id")?, text("/proc/uptime")?, text("/sys/power/suspend_stats/success")?, text("/sys/power/suspend_stats/fail")?, text("/sys/module/pm_stage/parameters/markers")?))?;
    plan.save(
        "after-kernel",
        &captured(
            "journalctl",
            &["-k", "-b", "0", "--no-pager", "-o", "short-monotonic"],
        ),
    )?;
    plan.save(
        "after-service",
        &captured(
            "journalctl",
            &[
                "-b",
                "0",
                "-u",
                "systemd-suspend.service",
                "--no-pager",
                "-o",
                "short-iso-precise",
            ],
        ),
    )
}

fn attempt(plan: &Plan, data: &str) -> Result<()> {
    plan.save("requested", data)?; // create_new is the duplicate-attempt guard.
    let mut ntp_stopped = false;
    let mut returned = None;
    let result = (|| {
        capture()?;
        plan.preflight(true)?;
        for disk in DISKS {
            let warmed = Command::new("timeout")
                .args([
                    "30s",
                    "dd",
                    &format!("if={disk}"),
                    "bs=4096",
                    "count=1",
                    "iflag=direct",
                    "status=none",
                ])
                .stdout(Stdio::null())
                .status()?;
            check(warmed.success(), format!("disk warm-up failed: {disk}"))?;
            marker(&format!("WARMED {disk}"))?;
        }
        plan.save(
            "before",
            &format!(
                "epoch={}\nuptime={}\n{}\n{}\n{}\n",
                now()?,
                text("/proc/uptime")?,
                output("hwclock", &["--show", "--utc"])?,
                output("zpool", &["status"])?,
                output(
                    "journalctl",
                    &["-k", "-b", "0", "--no-pager", "-o", "short-monotonic"]
                )?
            ),
        )?;
        output("rtcwake", &["--mode", "disable"])?;
        plan.preflight(true)?;
        ntp_stopped = true;
        output("systemctl", &["stop", "systemd-timesyncd.service"])?;
        marker(&format!("BEGIN trial={} boot={} due_epoch={} wake_epoch={} profile={} pm_async={} pm_trace=0 recorder=off TPM=initialized raw-EC=not-run inhibitors=enforced",
            plan.get("trial_id"), plan.get("boot"), plan.due, plan.wake, plan.get("profile"), plan.get("pm_async")))?;
        output("journalctl", &["--sync"])?;
        plan.identity()?;
        let uptime = text("/proc/uptime")?
            .split_whitespace()
            .next()
            .ok_or("missing uptime")?
            .parse::<f64>()? as u64;
        plan.age(uptime, now()?, true)?;
        check(
            text("/sys/power/pm_async")? == plan.get("pm_async"),
            "PM setting changed at entry",
        )?;
        let request = suspend_and_wait(LoginPolicy::from_plan(data)?);
        let epoch = now()?;
        returned = Some(epoch);
        raw_return(plan, epoch, &request)?;
        request
    })();
    let clock = if ntp_stopped { restore_clock() } else { Ok(()) };
    plan.save(
        "recovery",
        &format!(
            "request={result:?}\nclock={clock:?}\n{}\n{}\n{}\n",
            captured("timedatectl", &["timesync-status"]),
            captured("zpool", &["status"]),
            captured("findmnt", &["-rn", "-t", "zfs", "-o", "SOURCE,TARGET"])
        ),
    )?;
    if let Err(error) = result {
        return Err(format!("request/entry/return failed: {error}; clock={clock:?}").into());
    }
    check(
        text("/sys/power/suspend_stats/success")? == "1"
            && text("/sys/power/suspend_stats/fail")? == "0",
        "kernel did not complete one successful cycle",
    )?;
    check(
        completed_cycles(&text(format!(
            "{STATE}/{}-after-kernel.txt",
            plan.get("trial_id")
        ))?)?
            == 1,
        "kernel cycle incomplete",
    )?;
    check(
        text("/sys/module/pm_stage/parameters/markers")? == "0",
        "unarmed recorder wrote markers",
    )?;
    plan.identity()?;
    plan.save(
        "kernel-completed",
        &format!(
            "boot={}\nreturned_epoch={}\nclock={clock:?}\n",
            plan.get("boot"),
            returned.ok_or("no return time")?
        ),
    )?;
    clock.map_err(|error| format!("RECOVERY_FAILURE after complete kernel cycle: {error}"))?;
    check(
        (plan.wake.saturating_sub(2)..=plan.wake + 180)
            .contains(&returned.ok_or("missing return time")?),
        "EARLY_OR_LATE_WAKE: kernel returned outside planned WoL window",
    )?;
    check(
        output("zpool", &["status", "-x"])? == "all pools are healthy",
        "RECOVERY_FAILURE: unhealthy pool",
    )?;
    for (mount, dataset) in [
        ("/srv/data/fast", "fast/data"),
        ("/var/lib/nextcloud/data", "fast/ncdata"),
        ("/srv/backup", "tank/backup"),
        ("/srv/backup/recovery", "tank/backup/recovery"),
        ("/srv/data/archive", "tank/archive"),
    ] {
        check(
            output("findmnt", &["-n", "-o", "SOURCE", "-M", mount])? == dataset,
            format!("RECOVERY_FAILURE: missing {mount}"),
        )?;
    }
    let record = record_completion(plan.get("boot"), 1, 0, now()?)?;
    plan.save(
        "completed",
        &format!(
            "{record}returned_epoch={}\nphysical_sleep=AWAITING_OWNER_GRAPH\n",
            returned.unwrap()
        ),
    )?;
    marker(&format!("VERIFIED trial={} one timed deep cycle; fresh NTP and storage healthy; physical sleep evidence still required", plan.get("trial_id")))
}

fn load(path: &str, hash: &str) -> Result<(Plan, String)> {
    check(
        hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit()),
        "invalid manifest hash",
    )?;
    let meta = fs::symlink_metadata(path)?;
    check(
        meta.is_file() && meta.uid() == 0 && meta.mode() & 0o777 == 0o600,
        "trial manifest must be root-owned mode 0600",
    )?;
    let data = fs::read_to_string(path)?;
    check(
        output("sha256sum", &[path])?.split_whitespace().next() == Some(hash),
        "manifest hash changed",
    )?;
    let plan = Plan::parse(&data)?;
    plan.identity()?;
    Ok((plan, data))
}

pub fn restore(path: &str, hash: &str) -> Result<()> {
    let (plan, _) = load(path, hash)?;
    check(
        Path::new(&format!("{STATE}/{}-requested.txt", plan.get("trial_id"))).exists(),
        "no trial execution to clean up",
    )?;
    let current = text("/sys/power/pm_async")?;
    check(
        current == "1" || current == plan.get("pm_async"),
        "unexpected PM value during trial cleanup",
    )?;
    fs::write("/sys/power/pm_async", "1")?;
    check(
        text("/sys/power/pm_async")? == "1",
        "trial cleanup PM readback failed",
    )?;
    check(
        text("/sys/module/pm_stage/parameters/armed")? == "N",
        "unexpected armed recorder during cleanup",
    )?;
    output("systemctl", &["start", "systemd-timesyncd.service"])?;
    marker(&format!(
        "TRIAL CLEANUP id={} pm_async=1 timesyncd=started",
        plan.get("trial_id")
    ))
}

pub fn run(path: &str, hash: &str, execute: bool) -> Result<()> {
    let (plan, data) = load(path, hash)?;
    not_held()?;
    let result = with_pm(
        Path::new("/sys/power/pm_async"),
        plan.get("pm_async"),
        || {
            if execute {
                attempt(&plan, &data)
            } else {
                capture()?;
                plan.preflight(false)?;
                marker(&format!(
                    "TRIAL READY id={} profile={} pm_async={} due={} wake={} manifest={hash}",
                    plan.get("trial_id"),
                    plan.get("profile"),
                    plan.get("pm_async"),
                    plan.due,
                    plan.wake
                ))
            }
        },
    );
    if execute {
        // Do not overwrite earlier errors on an accidental duplicate invocation.
        let suffix = if Path::new(&format!("{STATE}/{}-result.txt", plan.get("trial_id"))).exists()
        {
            format!("repeat-error-{}", now()?)
        } else {
            "result".into()
        };
        plan.save(
            &suffix,
            &format!(
                "epoch={}\nresult={result:?}\npm_async={}\n",
                now()?,
                text("/sys/power/pm_async")?
            ),
        )?;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> String {
        "schema_version=1\ntrial_id=serial-m-test\nprofile=M\nboot=214d0240-3825-4946-8492-c30bf530eba0\ngeneration=/nix/store/g-system\nkernel=/nix/store/k/bzImage\nmodules=/nix/store/m\nruntime=/nix/store/r/bin/predator-pm-study\ndue=20000\nwake=20180\npm_async=0\npm_trace=0\nrecorder=off\nmem_sleep=deep\npm_test=none\ntpm=initialized\nec=not-run\nlogged_in_users=allow\nordinal=1\ndaytime_start=19900\ndaytime_end=21000".into()
    }
    #[test]
    fn strict_manifest_rejects_silent_defaults_and_conflicting_modes() {
        assert!(Plan::parse(&manifest()).is_ok());
        for change in [
            manifest() + "\nunknown=1",
            manifest() + "\npm_async=1",
            manifest().replace("recorder=off", "recorder=rtc"),
            manifest().replace("ordinal=1", "ordinal=2"),
            manifest().replace("profile=M", "profile=L"),
            manifest().replace("wake=20180", "wake=20900"),
            manifest().replace("daytime_end=21000", "daytime_end=20200"),
            manifest().replace("pm_async=0\n", ""),
        ] {
            assert!(Plan::parse(&change).is_err(), "accepted {change}");
        }
    }
    #[test]
    fn age_guard_distinguishes_prepare_from_entry_and_rejects_missed_windows() {
        let plan = Plan::parse(&manifest()).unwrap();
        assert!(plan.age(7100, 16000, false).is_ok());
        assert!(plan.age(11100, 20000, true).is_ok());
        assert!(plan.age(11100, 20031, true).is_err());
        assert!(plan.age(11100, 19999, true).is_err());
        assert!(plan.age(100, 20000, true).is_err());
        assert!(plan.age(12000, 20000, true).is_err());
    }
    #[test]
    fn pm_setting_is_restored_after_failure() {
        let path = env::temp_dir().join(format!("predator-pm-setting-{}", std::process::id()));
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap()
            .write_all(b"1")
            .unwrap();
        let failed: Result<()> = with_pm(&path, "0", || {
            assert_eq!(text(&path).unwrap(), "0");
            Err("simulated abort".into())
        });
        assert!(failed.is_err());
        assert_eq!(text(&path).unwrap(), "1");
        with_pm(&path, "0", || Ok(())).unwrap();
        assert_eq!(text(&path).unwrap(), "1");
        fs::remove_file(path).unwrap();
    }
}
