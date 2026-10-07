use crate::{activity, monitoring::storage_event, util};
use anyhow::{Context, Result, ensure};
use chrono::{Local, Timelike};
use serde_json::Value;
use std::process::Command;

const POOLS: &[&str] = &["fast", "tank"];
const MAINTENANCE: &[&str] = &[
    "db-backup.service",
    "recovery-backup.service",
    "sanoid.service",
    "syncoid-*.service",
    "zfs-scrub.service",
    "zpool-trim.service",
    "nixos-upgrade.service",
    "nextcloud-setup.service",
    "nextcloud-cron.service",
];

pub fn pool_refusal(pool: &Value) -> Result<Option<String>> {
    let name = pool["name"].as_str().context("missing pool name")?;
    if pool["scan_stats"]["state"] == "SCANNING" {
        return Ok(Some(format!("ZFS {name} scan in progress")));
    }
    let devices = pool["vdevs"].as_object().context("missing pool vdevs")?;
    let mut offline = std::collections::BTreeSet::new();
    for state in std::iter::once(&pool["state"]).chain(devices.values().map(|v| &v["state"])) {
        let state = state.as_str().context("missing pool/vdev state")?;
        if state != "ONLINE" {
            offline.insert(state);
        }
    }
    if !offline.is_empty() {
        return Ok(Some(format!(
            "ZFS {name} vdevs not online: {}",
            offline.into_iter().collect::<Vec<_>>().join(", ")
        )));
    }
    let mut checksum = false;
    for device in devices.values() {
        if device["read_errors"]
            .as_u64()
            .context("missing read errors")?
            > 0
            || device["write_errors"]
                .as_u64()
                .context("missing write errors")?
                > 0
        {
            return Ok(Some(format!("ZFS {name} has read or write errors")));
        }
        checksum |= device["checksum_errors"]
            .as_u64()
            .context("missing checksum errors")?
            > 0;
    }
    if pool["error_count"]
        .as_u64()
        .context("missing pool error count")?
        > 0
    {
        return Ok(Some(format!("ZFS {name} has permanent data errors")));
    }
    if !checksum {
        return Ok(Some(format!("ZFS {name} requires attention")));
    }
    Ok(None)
}

fn pool_problem(now: i64) -> Result<Option<String>> {
    let imported = util::output("zpool", &["list", "-H", "-o", "name"])?;
    let missing: Vec<_> = POOLS
        .iter()
        .filter(|name| !imported.split_whitespace().any(|s| s == **name))
        .copied()
        .collect();
    if !missing.is_empty() {
        return Ok(Some(format!("missing ZFS pools: {}", missing.join(", "))));
    }
    let data = util::json(
        "zpool",
        &["status", "-xj", "--json-int", "--json-flat-vdevs"],
    )?;
    let pools = data["pools"]
        .as_object()
        .context("missing ZFS pools JSON")?;
    for name in POOLS {
        if let Some(pool) = pools.get(*name) {
            if let Some(reason) = pool_refusal(pool)? {
                return Ok(Some(reason));
            }
            let journal = util::output(
                "journalctl",
                &["-k", "--since", &format!("@{}", now - 1800), "--no-pager"],
            )?;
            if journal.lines().any(storage_event) {
                return Ok(Some(format!("recent storage faults, not clearing {name}")));
            }
            util::run("zpool", &["clear", name])?;
            if util::output("zpool", &["status", "-x", name])?.trim()
                != format!("pool '{name}' is healthy")
            {
                return Ok(Some(format!(
                    "ZFS {name} still unhealthy after zpool clear"
                )));
            }
            println!("night-suspend: cleared corrected errors on {name}");
        }
    }
    Ok(None)
}

fn blocker(now: i64) -> Result<Option<String>> {
    if let Some(reason) = activity::blocker(now as f64)? {
        return Ok(Some(reason));
    }
    let mut args = vec![
        "list-units",
        "--no-pager",
        "--no-legend",
        "--plain",
        "--state=active,activating,reloading,deactivating",
    ];
    args.extend(MAINTENANCE);
    let busy = util::output("systemctl", &args)?;
    if !busy.trim().is_empty() {
        return Ok(Some(format!("maintenance: {}", busy.trim())));
    }
    let workers = Command::new("pgrep").args(["-G", "nixbld"]).output()?;
    if workers.status.success() {
        return Ok(Some("Nix build workers".into()));
    }
    ensure!(
        workers.status.code() == Some(1),
        "pgrep failed: {}",
        String::from_utf8_lossy(&workers.stderr)
    );
    pool_problem(now)
}

pub fn run(activity_only: bool) -> Result<()> {
    let now = Local::now();
    if activity_only {
        println!(
            "{}",
            activity::blocker(now.timestamp() as f64)?.unwrap_or_else(|| {
                "no authenticated user activity or open app proxy connections".into()
            })
        );
        return Ok(());
    }
    if now.hour() >= 7 {
        println!("night-suspend: outside the overnight window");
        return Ok(());
    }
    if let Some(reason) = blocker(now.timestamp())? {
        println!("night-suspend: blocked: {reason}");
        return Ok(());
    }
    let disks = util::json("lsblk", &["-Jdp", "-o", "NAME,TRAN"])?;
    for disk in disks["blockdevices"]
        .as_array()
        .context("missing blockdevices")?
    {
        if disk["tran"] == "usb" {
            let name = disk["name"].as_str().context("missing disk name")?;
            util::run(
                "timeout",
                &[
                    "--kill-after=5",
                    "90",
                    "dd",
                    &format!("if={name}"),
                    "of=/dev/null",
                    "bs=512",
                    "count=1",
                    "iflag=direct",
                ],
            )?;
            println!("night-suspend: warmed {name}");
        }
    }
    let alarm = now
        .with_hour(7)
        .and_then(|t| t.with_minute(0))
        .and_then(|t| t.with_second(0))
        .and_then(|t| t.with_nanosecond(0))
        .context("invalid local wake time")?;
    util::run(
        "rtcwake",
        &["--mode=no", "--time", &alarm.timestamp().to_string()],
    )?;
    println!("night-suspend: RTC alarm set for 07:00, requesting suspend");
    util::run("systemctl", &["--check-inhibitors=yes", "suspend"])
}
