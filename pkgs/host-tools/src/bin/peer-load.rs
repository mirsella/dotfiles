use anyhow::{Context, Result, bail, ensure};
use std::{collections::BTreeMap, fs, process::Command, thread};

// Linux prints one cpuN line per online logical CPU. This counts the whole
// host, not just the CPUs available to the SSH process.
const SAMPLE: &str = "cat /proc/loadavg && grep -c '^cpu[0-9]' /proc/stat";
const HOSTS: [&str; 3] = ["main", "laptop", "predator"];

fn normalized_load(loadavg: &str, cpus: usize) -> Result<f64> {
    let load: f64 = loadavg
        .split_whitespace()
        .next()
        .context("missing one-minute load")?
        .parse()
        .context("invalid one-minute load")?;
    ensure!(load.is_finite() && load >= 0.0, "invalid one-minute load");
    ensure!(cpus > 0, "online CPU count is zero");
    Ok(load * 100.0 / cpus as f64)
}

fn parse_remote_sample(sample: &str) -> Result<f64> {
    let mut lines = sample.lines();
    let loadavg = lines.next().context("missing load average")?;
    let cpus = lines
        .next()
        .context("missing online CPU count")?
        .trim()
        .parse()
        .context("invalid online CPU count")?;
    ensure!(lines.next().is_none(), "unexpected extra output");
    normalized_load(loadavg, cpus)
}

fn sample_local() -> Result<f64> {
    let loadavg = fs::read_to_string("/proc/loadavg").context("read /proc/loadavg")?;
    let stat = fs::read_to_string("/proc/stat").context("read /proc/stat")?;
    let cpus = stat
        .lines()
        .filter(|line| matches!(line.as_bytes(), [b'c', b'p', b'u', b'0'..=b'9', ..]))
        .count();
    normalized_load(&loadavg, cpus)
}

fn sample_remote(host: &str) -> Result<f64> {
    let output = Command::new("timeout")
        .args([
            "10s",
            "ssh",
            "-nT",
            "-oBatchMode=yes",
            "-oConnectTimeout=4",
            "-oStrictHostKeyChecking=yes",
            host,
            &format!("/bin/sh -c \"{SAMPLE}\""),
        ])
        .output()
        .with_context(|| format!("could not sample {host}"))?;
    ensure!(
        output.status.success(),
        "{host} load unavailable ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    std::str::from_utf8(&output.stdout)
        .context("load sample is not UTF-8")
        .and_then(parse_remote_sample)
        .with_context(|| format!("invalid load sample from {host}"))
}

fn report_all(local_host: &str, json: bool) -> Result<()> {
    ensure!(
        HOSTS.contains(&local_host),
        "unknown local host: {local_host}"
    );
    let loads = thread::scope(|scope| {
        let samples = HOSTS.map(|host| {
            scope.spawn(move || {
                if host == local_host {
                    sample_local().with_context(|| format!("could not sample {host} locally"))
                } else {
                    sample_remote(host)
                }
            })
        });
        samples.map(
            |sample| match sample.join().expect("load sampler panicked") {
                Ok(load) => Some(load),
                Err(error) => {
                    eprintln!("{error:#}");
                    None
                }
            },
        )
    });
    let by_host = HOSTS.into_iter().zip(loads);
    if json {
        println!(
            "{}",
            serde_json::to_string(&by_host.collect::<BTreeMap<_, _>>())?
        );
    } else {
        for (host, load) in by_host {
            match load {
                Some(load) => println!("{host} load: {load:.0}%"),
                None => println!("{host} load: unavailable"),
            }
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [flag, local_host] if flag == "--all" || flag == "--json" => {
            report_all(local_host, flag == "--json")
        }
        [target] if target == "local" || HOSTS.contains(&target.as_str()) => {
            let load = if target == "local" {
                sample_local()?
            } else {
                sample_remote(target)?
            };
            println!("{load:.0}%");
            Ok(())
        }
        _ => bail!("usage: peer-load [main|laptop|predator|--all|--json]"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_one_minute_load_per_cpu_and_can_exceed_capacity() {
        assert_eq!(
            parse_remote_sample("4.00 9.00 7.00 3/100 123\n8\n").unwrap(),
            50.0
        );
        assert_eq!(
            parse_remote_sample("4.00 9.00 7.00 3/100 123\n2\n").unwrap(),
            200.0
        );
    }

    #[test]
    fn rejects_missing_or_invalid_samples() {
        for sample in ["", "4.00\n", "4.00\n0\n", "NaN\n8\n", "4.00\n8\nbanner\n"] {
            assert!(parse_remote_sample(sample).is_err(), "{sample:?}");
        }
    }
}
