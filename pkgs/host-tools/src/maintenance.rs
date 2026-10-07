use crate::util;
use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use std::{ffi::CString, mem::MaybeUninit, os::unix::ffi::OsStrExt, path::Path};

const GIB: u64 = 1 << 30;

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
    let name = CString::new(path.as_os_str().as_bytes())?;
    let mut stats = MaybeUninit::<libc::statvfs>::uninit();
    // statvfs initializes the structure only on success; the CString stays live.
    let status = unsafe { libc::statvfs(name.as_ptr(), stats.as_mut_ptr()) };
    if status != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let stats = unsafe { stats.assume_init() };
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
        Ok(())
    }
}
