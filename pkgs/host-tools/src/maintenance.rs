use crate::util;
use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    ffi::CString,
    fs,
    mem::MaybeUninit,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

const GIB: u64 = 1 << 30;
const DISK_WARNING: u64 = 100 * GIB;
const DISK_CRITICAL: u64 = 50 * GIB;
const DISK_RECOVERY_MARGIN: u64 = 5 * GIB;
const METADATA_WARNING_PERCENT: f64 = 90.0;
const METADATA_RECOVERY_PERCENT: f64 = 85.0;
const UNALLOCATED_WARNING: u64 = 8 * GIB;
const UNALLOCATED_RECOVERY: u64 = 12 * GIB;

pub fn unallocated(text: &str) -> Result<u64> {
    let re = Regex::new(r"(?m)^\s*Device unallocated:\s*(\d+)\s*$")?;
    re.captures(text)
        .context("Unexpected btrfs filesystem usage output")?[1]
        .parse()
        .context("invalid unallocated bytes")
}

fn available(path: &Path) -> Result<u64> {
    unallocated(&util::output(
        "btrfs",
        &["filesystem", "usage", "-b", util::path(path)?],
    )?)
}

#[derive(Debug, PartialEq)]
struct BtrfsUsage {
    unallocated: u64,
    metadata_size: u64,
    metadata_used: u64,
}

fn parse_btrfs_usage(text: &str) -> Result<BtrfsUsage> {
    let metadata = Regex::new(r"(?m)^Metadata,[^:]+:\s*Size:(\d+),\s*Used:(\d+)")?;
    let captures = metadata
        .captures(text)
        .context("Btrfs metadata usage missing from filesystem usage output")?;
    let usage = BtrfsUsage {
        unallocated: unallocated(text)?,
        metadata_size: captures[1].parse().context("invalid Btrfs metadata size")?,
        metadata_used: captures[2].parse().context("invalid Btrfs metadata used")?,
    };
    ensure!(usage.metadata_size > 0, "Btrfs reported zero metadata size");
    ensure!(
        usage.metadata_used <= usage.metadata_size,
        "Btrfs metadata used exceeds its allocated size"
    );
    Ok(usage)
}

fn btrfs_usage(path: &Path) -> Result<BtrfsUsage> {
    parse_btrfs_usage(&util::output(
        "btrfs",
        &["filesystem", "usage", "-b", util::path(path)?],
    )?)
}

fn statvfs(path: &Path) -> Result<libc::statvfs> {
    let name = CString::new(path.as_os_str().as_bytes())?;
    let mut stats = MaybeUninit::<libc::statvfs>::uninit();
    // statvfs initializes the structure only on success; the CString stays live.
    let status = unsafe { libc::statvfs(name.as_ptr(), stats.as_mut_ptr()) };
    if status != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(unsafe { stats.assume_init() })
}

fn available_bytes(stats: &libc::statvfs) -> Result<u64> {
    (stats.f_bavail as u64)
        .checked_mul(stats.f_frsize as u64)
        .context("filesystem available-byte count overflowed")
}

pub fn reclaim_space(
    mut read: impl FnMut() -> Result<u64>,
    mut balance: impl FnMut() -> Result<()>,
) -> Result<()> {
    let mut remaining = read()?;
    if remaining >= 8 * GIB {
        println!(
            "Btrfs: {:.2} GiB unallocated; no balance needed",
            remaining as f64 / GIB as f64
        );
        return Ok(());
    }
    for _ in 0..4 {
        balance()?;
        let next = read()?;
        println!(
            "Btrfs: {:.2} GiB unallocated after limited balance",
            next as f64 / GIB as f64
        );
        if next >= 12 * GIB {
            return Ok(());
        }
        ensure!(
            next > remaining,
            "Limited balance did not free device space; inspect Btrfs usage"
        );
        remaining = next;
    }
    bail!(
        "Btrfs still has only {:.2} GiB unallocated after four limited passes",
        remaining as f64 / GIB as f64
    );
}

pub fn pressure(percent: f64, remaining: u64) -> bool {
    percent >= 90.0 || remaining < 8 * GIB
}

fn used_percent(stats: &libc::statvfs) -> Result<f64> {
    ensure!(stats.f_blocks > 0, "filesystem reported zero total blocks");
    Ok(100.0 * (stats.f_blocks - stats.f_bavail) as f64 / stats.f_blocks as f64)
}

pub fn btrfs(path: &Path, notify: bool, reclaim: bool) -> Result<()> {
    if reclaim {
        return reclaim_space(
            || available(path),
            || {
                util::run(
                    "btrfs",
                    &["balance", "start", "-dusage=75,limit=5", util::path(path)?],
                )
            },
        );
    }
    let remaining = available(path)?;
    let stats = statvfs(path)?;
    let percent = used_percent(&stats)?;
    println!(
        "Btrfs {}: filesystem {percent:.1}% full, {:.2} GiB device-unallocated",
        path.display(),
        remaining as f64 / GIB as f64
    );
    if pressure(percent, remaining) {
        let message = format!(
            "Btrfs {}: filesystem {percent:.1}% full; {:.2} GiB device-unallocated. Inspect filesystem usage and the limited-balance service.",
            path.display(),
            remaining as f64 / GIB as f64
        );
        if notify {
            util::run(
                "notify-send",
                &["--urgency=critical", "Filesystem space pressure", &message],
            )
            .with_context(|| format!("{message} Desktop notification failed"))?;
        }
        bail!("{message}");
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum DiskLevel {
    #[default]
    Clear,
    Warning,
    Critical,
}

#[derive(Default, Deserialize, Serialize)]
struct DiskAlertState {
    disk: DiskLevel,
    metadata_alerted: bool,
}

struct DiskSnapshot {
    available: u64,
    metadata_percent: f64,
    unallocated: u64,
}

fn disk_level(available: u64, previous: DiskLevel) -> DiskLevel {
    match previous {
        DiskLevel::Clear if available <= DISK_CRITICAL => DiskLevel::Critical,
        DiskLevel::Clear if available <= DISK_WARNING => DiskLevel::Warning,
        DiskLevel::Clear => DiskLevel::Clear,
        DiskLevel::Warning if available <= DISK_CRITICAL => DiskLevel::Critical,
        DiskLevel::Warning if available <= DISK_WARNING + DISK_RECOVERY_MARGIN => {
            DiskLevel::Warning
        }
        DiskLevel::Warning => DiskLevel::Clear,
        DiskLevel::Critical if available <= DISK_CRITICAL + DISK_RECOVERY_MARGIN => {
            DiskLevel::Critical
        }
        DiskLevel::Critical if available <= DISK_WARNING + DISK_RECOVERY_MARGIN => {
            DiskLevel::Warning
        }
        DiskLevel::Critical => DiskLevel::Clear,
    }
}

fn metadata_alert(snapshot: &DiskSnapshot, previous: bool) -> bool {
    if previous {
        snapshot.metadata_percent >= METADATA_RECOVERY_PERCENT
            || snapshot.unallocated <= UNALLOCATED_RECOVERY
    } else {
        snapshot.metadata_percent >= METADATA_WARNING_PERCENT
            || snapshot.unallocated <= UNALLOCATED_WARNING
    }
}

fn load_alert_state(path: &Path) -> Result<DiskAlertState> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .with_context(|| format!("parse disk alert state {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(DiskAlertState::default()),
        Err(error) => {
            Err(error).with_context(|| format!("read disk alert state {}", path.display()))
        }
    }
}

fn reconcile_disk_alerts(
    state_path: &Path,
    state: &mut DiskAlertState,
    snapshot: &DiskSnapshot,
    notify: impl Fn(&str, &str, bool) -> Result<()>,
) -> Result<()> {
    let next_disk = disk_level(snapshot.available, state.disk);
    if next_disk != state.disk {
        let escalate = match (state.disk, next_disk) {
            (DiskLevel::Clear, DiskLevel::Warning) => Some((
                "Disk space below 100 GiB",
                format!(
                    "{:.1} GiB available. Free up space before the 50 GiB critical threshold.",
                    snapshot.available as f64 / GIB as f64
                ),
                false,
            )),
            (_, DiskLevel::Critical) => Some((
                "Disk space critically low",
                format!(
                    "Only {:.1} GiB available (below 50 GiB).",
                    snapshot.available as f64 / GIB as f64
                ),
                true,
            )),
            // Recovery and critical-to-warning de-escalation only rearm future alerts.
            _ => None,
        };
        if let Some((summary, body, critical)) = escalate {
            notify(summary, &body, critical)?;
        }
        state.disk = next_disk;
        util::atomic_write(state_path, serde_json::to_string(state)?.as_bytes())?;
    }

    let next_metadata = metadata_alert(snapshot, state.metadata_alerted);
    if next_metadata != state.metadata_alerted {
        if next_metadata {
            notify(
                "Btrfs metadata/allocation pressure",
                &format!(
                    "Metadata is {:.1}% used; {:.1} GiB is unallocated. Btrfs may be unable to allocate new chunks. Inspect `btrfs filesystem usage` and the limited-balance service.",
                    snapshot.metadata_percent,
                    snapshot.unallocated as f64 / GIB as f64
                ),
                true,
            )?;
        }
        state.metadata_alerted = next_metadata;
        util::atomic_write(state_path, serde_json::to_string(state)?.as_bytes())?;
    }
    Ok(())
}

pub fn disk_alerts(path: &Path, test: bool) -> Result<()> {
    if test {
        return crate::monitoring::storage_alert(
            "Storage alert test",
            "Desktop storage notifications are working.",
            false,
        );
    }
    let stats = statvfs(path)?;
    let usage = btrfs_usage(path)?;
    let snapshot = DiskSnapshot {
        available: available_bytes(&stats)?,
        metadata_percent: 100.0 * usage.metadata_used as f64 / usage.metadata_size as f64,
        unallocated: usage.unallocated,
    };
    println!(
        "{}: {:.1} GiB available; Btrfs metadata {:.1}% used; {:.1} GiB unallocated",
        path.display(),
        snapshot.available as f64 / GIB as f64,
        snapshot.metadata_percent,
        snapshot.unallocated as f64 / GIB as f64
    );
    let state_path = PathBuf::from(std::env::var("STATE_DIRECTORY")?).join("alerts.json");
    let mut state = load_alert_state(&state_path)?;
    reconcile_disk_alerts(
        &state_path,
        &mut state,
        &snapshot,
        |summary, body, critical| crate::monitoring::storage_alert(summary, body, critical),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn btrfs_pressure_uses_available_blocks() -> Result<()> {
        // statvfs contains only integer fields, all of which permit zero.
        let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
        stats.f_blocks = 100;
        stats.f_bfree = 20;
        stats.f_bavail = 10;
        assert_eq!(used_percent(&stats)?, 90.0);
        assert!(pressure(used_percent(&stats)?, 8 * GIB));

        stats.f_bavail = 11;
        assert!(!pressure(used_percent(&stats)?, 8 * GIB));
        stats.f_blocks = 0;
        assert!(used_percent(&stats).is_err());
        stats.f_frsize = 4096;
        stats.f_bavail = 3;
        assert_eq!(available_bytes(&stats)?, 3 * 4096);
        Ok(())
    }

    #[test]
    fn parses_raw_btrfs_metadata_and_unallocated_space() -> Result<()> {
        let usage = parse_btrfs_usage(
            "Overall:\n    Device unallocated: 3222364160\n\
             Data,single: Size:956724936704, Used:729083785216 (76.21%)\n\
             Metadata,DUP: Size:19574095872, Used:13279969280 (67.84%)\n",
        )?;
        assert_eq!(
            usage,
            BtrfsUsage {
                unallocated: 3_222_364_160,
                metadata_size: 19_574_095_872,
                metadata_used: 13_279_969_280,
            }
        );
        assert!(parse_btrfs_usage("Device unallocated: 1\n").is_err());
        Ok(())
    }

    #[test]
    fn disk_alerts_cross_once_and_rearm_after_recovery() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let state_path = directory.path().join("alerts.json");
        let mut state = DiskAlertState::default();
        let mut snapshot = DiskSnapshot {
            available: 99 * GIB,
            metadata_percent: 50.0,
            unallocated: 20 * GIB,
        };
        let notifications = std::cell::RefCell::new(Vec::new());
        let check = |state: &mut DiskAlertState, snapshot: &DiskSnapshot| {
            reconcile_disk_alerts(&state_path, state, snapshot, |summary, _, critical| {
                notifications
                    .borrow_mut()
                    .push((summary.to_owned(), critical));
                Ok(())
            })
        };

        check(&mut state, &snapshot)?;
        check(&mut state, &snapshot)?;
        assert_eq!(notifications.borrow().len(), 1);
        snapshot.available = 49 * GIB;
        check(&mut state, &snapshot)?;
        assert_eq!(notifications.borrow().len(), 2);
        snapshot.available = 60 * GIB;
        check(&mut state, &snapshot)?;
        assert_eq!(state.disk, DiskLevel::Warning);
        assert_eq!(notifications.borrow().len(), 2);
        snapshot.available = 106 * GIB;
        check(&mut state, &snapshot)?;
        assert_eq!(state.disk, DiskLevel::Clear);
        snapshot.available = 100 * GIB;
        check(&mut state, &snapshot)?;
        assert_eq!(notifications.borrow().len(), 3);
        Ok(())
    }

    #[test]
    fn metadata_alert_has_recovery_hysteresis() {
        let mut snapshot = DiskSnapshot {
            available: 200 * GIB,
            metadata_percent: 90.0,
            unallocated: 20 * GIB,
        };
        assert!(metadata_alert(&snapshot, false));
        snapshot.metadata_percent = 86.0;
        assert!(metadata_alert(&snapshot, true));
        snapshot.metadata_percent = 84.0;
        assert!(!metadata_alert(&snapshot, true));
        snapshot.unallocated = 8 * GIB;
        assert!(metadata_alert(&snapshot, false));
        snapshot.unallocated = 10 * GIB;
        assert!(metadata_alert(&snapshot, true));
        snapshot.unallocated = 13 * GIB;
        assert!(!metadata_alert(&snapshot, true));
    }
}
