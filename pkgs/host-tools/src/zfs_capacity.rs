use crate::{monitoring, util};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    zfs: PathBuf,
    pools: Vec<String>,
    warning_percent: u8,
    critical_percent: u8,
    reminder_hours: u16,
    key_file: PathBuf,
    recipient: String,
}

impl Config {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.zfs.is_absolute(),
            "zfs program must be an absolute path"
        );
        ensure!(
            self.warning_percent >= 5
                && self.warning_percent < self.critical_percent
                && self.critical_percent < 100,
            "capacity thresholds must satisfy 5 <= warning < critical < 100"
        );
        ensure!(
            (1..=168).contains(&self.reminder_hours),
            "invalid reminder interval"
        );
        ensure!(!self.pools.is_empty(), "no pools configured");
        let mut names = BTreeSet::new();
        for pool in &self.pools {
            ensure!(
                pool.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
                    && pool
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"_-.:".contains(&c))
                    && names.insert(pool),
                "invalid or duplicate pool name: {pool}"
            );
        }
        ensure!(!self.recipient.trim().is_empty(), "empty email recipient");
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum Level {
    #[default]
    Normal,
    Warning,
    Critical,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Alert {
    level: Level,
    last_sent: Option<u64>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    pools: BTreeMap<String, Alert>,
}

#[derive(Clone, Copy, Debug)]
struct Usage {
    used: u64,
    available: u64,
}

impl Usage {
    fn parse(text: &str) -> Result<Self> {
        let mut values = BTreeMap::new();
        for line in text.lines() {
            let (property, value) = line.split_once('\t').context("invalid zfs capacity row")?;
            ensure!(
                matches!(property, "used" | "available"),
                "unexpected capacity property"
            );
            ensure!(
                values.insert(property, value.parse::<u64>()?).is_none(),
                "duplicate capacity property"
            );
        }
        let result = Self {
            used: *values.get("used").context("missing used capacity")?,
            available: *values
                .get("available")
                .context("missing available capacity")?,
        };
        ensure!(
            result.used != 0 || result.available != 0,
            "zero usable pool capacity"
        );
        Ok(result)
    }

    fn basis_points(self) -> u128 {
        u128::from(self.used) * 10_000 / (u128::from(self.used) + u128::from(self.available))
    }

    fn description(self, pool: &str) -> String {
        format!(
            "{pool}: {:.1}% used; {:.1} GB used, {:.1} GB available",
            self.basis_points() as f64 / 100.0,
            self.used as f64 / 1e9,
            self.available as f64 / 1e9,
        )
    }
}

fn level(usage: Usage, previous: Level, warning: u8, critical: u8) -> Level {
    let percent = usage.basis_points();
    if percent >= u128::from(critical) * 100
        || (previous == Level::Critical && percent >= u128::from(critical - 5) * 100)
    {
        Level::Critical
    } else if percent >= u128::from(warning) * 100
        || (previous != Level::Normal && percent >= u128::from(warning - 5) * 100)
    {
        Level::Warning
    } else {
        Level::Normal
    }
}

fn check(
    config: &Config,
    host: &str,
    pool: &str,
    usage: Usage,
    previous: Alert,
    now: u64,
    send: impl FnOnce(&str, &str) -> Result<()>,
) -> Result<Alert> {
    let next = level(
        usage,
        previous.level,
        config.warning_percent,
        config.critical_percent,
    );
    let mut result = Alert {
        level: next,
        ..previous
    };
    if next == Level::Normal {
        return Ok(result);
    }
    let due = previous.last_sent.is_none_or(|last| {
        if last > now {
            eprintln!(
                "Capacity notification clock moved backwards for {pool}; refreshing its reminder."
            );
            true
        } else {
            now - last >= u64::from(config.reminder_hours) * 3600
        }
    });
    if next > previous.level || due {
        let summary = format!("{host}: {pool} capacity {next:?}");
        let body = format!(
            "{}\n\nWarning threshold: {}%; critical threshold: {}%.\n\
             Usage includes all child datasets, snapshots and reservations. \
             Alert levels clear five percentage points below their threshold.\n\
             Free space or review retention before the pool fills. \
             This monitor never deletes data or changes retention.",
            usage.description(pool),
            config.warning_percent,
            config.critical_percent,
        );
        send(&summary, &body)?;
        result.last_sent = Some(now);
    }
    Ok(result)
}

fn check_pools(pools: &[String], mut check: impl FnMut(&str) -> Result<()>) -> Result<()> {
    let mut failures = Vec::new();
    for pool in pools {
        if let Err(error) = check(pool) {
            failures.push(format!("{pool}: {error:#}"));
        }
    }
    ensure!(
        failures.is_empty(),
        "capacity checks failed:\n{}",
        failures.join("\n")
    );
    Ok(())
}

pub fn run(config: &Path, test: bool) -> Result<()> {
    let config: Config = serde_json::from_str(&util::read(config)?)?;
    config.validate()?;
    let directory =
        PathBuf::from(env::var("STATE_DIRECTORY").context("STATE_DIRECTORY is required")?);
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(directory.join("lock"))?;
    lock.lock()?;
    let path = directory.join("state.json");
    let mut state: State = match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).context("invalid capacity alert state")?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => State::default(),
        Err(error) => return Err(error.into()),
    };
    let host = util::read("/proc/sys/kernel/hostname")?.trim().to_owned();
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let mut report = Vec::new();
    check_pools(&config.pools, |pool| {
        let raw = util::output(
            config.zfs.to_str().context("invalid zfs path")?,
            &["get", "-Hp", "-o", "property,value", "used,available", pool],
        )?;
        let usage = Usage::parse(&raw).with_context(|| format!("capacity of {pool}"))?;
        let description = usage.description(pool);
        println!("{description}");
        if test {
            report.push(description);
            return Ok(());
        }
        let previous = state.pools.get(pool).copied().unwrap_or_default();
        let next = check(
            &config,
            &host,
            pool,
            usage,
            previous,
            now,
            |subject, body| monitoring::mail(&config.key_file, &config.recipient, subject, body),
        )?;
        if state.pools.get(pool) != Some(&next) {
            let mut updated = state.clone();
            updated.pools.insert(pool.to_owned(), next);
            util::atomic_write(&path, &serde_json::to_vec(&updated)?)?;
            state = updated;
        }
        Ok(())
    })?;
    if test {
        monitoring::mail(
            &config.key_file,
            &config.recipient,
            &format!("{host}: storage capacity alert delivery test"),
            &format!(
                "Capacity alert delivery test; no threshold has been crossed.\n\n{}\n\n\
                Warning: {}%; critical: {}%. Checked every five minutes; \
                persistent alerts repeat every {} hours. Test did not change alert state.",
                report.join("\n"),
                config.warning_percent,
                config.critical_percent,
                config.reminder_hours
            ),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        Config {
            zfs: "/bin/zfs".into(),
            pools: vec!["backups".into()],
            warning_percent: 75,
            critical_percent: 80,
            reminder_hours: 24,
            key_file: "/key".into(),
            recipient: "owner@example.com".into(),
        }
    }
    fn usage(percent: u64) -> Usage {
        Usage {
            used: percent,
            available: 100 - percent,
        }
    }

    #[test]
    fn parses_exact_machine_properties_and_rejects_invalid_data() {
        assert_eq!(
            Usage::parse("available\t25\nused\t75\n")
                .unwrap()
                .basis_points(),
            7500
        );
        for bad in [
            "",
            "used\t0\navailable\t0",
            "used\t1\nused\t2\navailable\t3",
            "used\t1",
            "used\t1\navailable\tbad",
            "used\t1\navailable\t3\nquota\t4",
        ] {
            assert!(Usage::parse(bad).is_err(), "{bad}");
        }
        assert_eq!(
            Usage {
                used: u64::MAX,
                available: u64::MAX
            }
            .basis_points(),
            5000
        );
    }
    #[test]
    fn thresholds_and_hysteresis_are_explicit() {
        for (before, percent, after) in [
            (Level::Normal, 74, Level::Normal),
            (Level::Normal, 75, Level::Warning),
            (Level::Warning, 80, Level::Critical),
            (Level::Critical, 79, Level::Critical),
            (Level::Critical, 75, Level::Critical),
            (Level::Critical, 74, Level::Warning),
            (Level::Warning, 70, Level::Warning),
            (Level::Warning, 69, Level::Normal),
        ] {
            assert_eq!(level(usage(percent), before, 75, 80), after);
        }
    }
    #[test]
    fn sends_crossings_escalations_and_daily_reminders_not_every_poll() -> Result<()> {
        let c = config();
        let first = check(
            &c,
            "host",
            "backups",
            usage(75),
            Alert::default(),
            100,
            |subject, body| {
                assert!(subject.contains("Warning"));
                assert!(body.contains("snapshots"));
                Ok(())
            },
        )?;
        assert_eq!(first.last_sent, Some(100));
        let unchanged = check(&c, "host", "backups", usage(76), first, 200, |_, _| {
            panic!("duplicate warning")
        })?;
        assert_eq!(unchanged, first);
        let escalated = check(
            &c,
            "host",
            "backups",
            usage(80),
            unchanged,
            300,
            |subject, _| {
                assert!(subject.contains("Critical"));
                Ok(())
            },
        )?;
        let reminded = check(
            &c,
            "host",
            "backups",
            usage(81),
            escalated,
            300 + 86400,
            |_, _| Ok(()),
        )?;
        assert_eq!(reminded.last_sent, Some(86700));
        let clear = check(&c, "host", "backups", usage(69), reminded, 86800, |_, _| {
            panic!("recovery spam")
        })?;
        assert_eq!(clear.level, Level::Normal);
        check(
            &c,
            "host",
            "backups",
            usage(75),
            clear,
            86900,
            |_, _| Ok(()),
        )?;
        Ok(())
    }
    #[test]
    fn failed_delivery_does_not_acknowledge_an_alert() {
        let previous = Alert::default();
        assert!(
            check(
                &config(),
                "host",
                "backups",
                usage(80),
                previous,
                100,
                |_, _| anyhow::bail!("delivery failed")
            )
            .is_err()
        );
        assert_eq!(previous.last_sent, None);
    }
    #[test]
    fn unavailable_pool_does_not_suppress_healthy_pool_alerts() {
        let c = Config {
            pools: vec!["backups".into(), "fast".into()],
            ..config()
        };
        let mut state = State::default();
        let mut messages = Vec::new();
        let result = check_pools(&c.pools, |pool| {
            ensure!(pool != "backups", "pool unavailable");
            let next = check(
                &c,
                "host",
                pool,
                usage(80),
                state.pools.get(pool).copied().unwrap_or_default(),
                100,
                |subject, _| {
                    messages.push(subject.to_owned());
                    Ok(())
                },
            )?;
            state.pools.insert(pool.to_owned(), next);
            Ok(())
        });
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("backups: pool unavailable")
        );
        assert_eq!(messages, ["host: fast capacity Critical"]);
        assert!(!state.pools.contains_key("backups"));
        assert_eq!(state.pools["fast"].last_sent, Some(100));
    }
    #[test]
    fn rejects_bad_configuration_and_refreshes_after_clock_rollback() -> Result<()> {
        let mut c = config();
        c.validate()?;
        c.pools.push("backups".into());
        assert!(c.validate().is_err());
        c.pools = vec!["--all".into()];
        assert!(c.validate().is_err());
        c = config();
        c.warning_percent = 80;
        assert!(c.validate().is_err());
        let previous = Alert {
            level: Level::Warning,
            last_sent: Some(200),
        };
        let refreshed = check(
            &config(),
            "host",
            "backups",
            usage(76),
            previous,
            100,
            |_, _| Ok(()),
        )?;
        assert_eq!(refreshed.last_sent, Some(100));
        Ok(())
    }
}
