use std::{
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    process::Command,
};
type R<T> = Result<T, Box<dyn std::error::Error>>;
const UNIT: &str = "predator-pm-boundary-core-efi-20261101.service";
fn out(c: &str, a: &[&str]) -> R<String> {
    let r = Command::new("timeout").arg("20").arg(c).args(a).output()?;
    if !r.status.success() {
        return Err(format!("{c} {a:?}: {}", String::from_utf8_lossy(&r.stderr)).into());
    }
    Ok(String::from_utf8(r.stdout)?)
}
fn same(p: &str, v: &str) -> R<()> {
    if fs::read_to_string(p)?.trim() != v {
        return Err(format!("{p} mismatch").into());
    }
    Ok(())
}
fn save(p: &str, v: &str) -> R<()> {
    let mut f = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(p)?;
    f.write_all(v.as_bytes())?;
    f.sync_all()?;
    Ok(())
}
fn main() -> R<()> {
    let now = out("date", &["+%s"])?.trim().parse::<u64>()?;
    assert!((1793516400..1793570400).contains(&now));
    same(
        "/proc/sys/kernel/random/boot_id",
        "39d7579b-b86d-4d30-b910-131943e550c8",
    )?;
    assert_eq!(
        fs::canonicalize("/run/current-system")?.to_str(),
        Some(
            "/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363"
        )
    );
    for (p, v) in [
        ("/sys/power/suspend_stats/success", "1"),
        ("/sys/power/suspend_stats/fail", "0"),
        ("/sys/power/pm_async", "1"),
        ("/sys/power/pm_trace", "0"),
        ("/sys/class/rtc/rtc0/wakealarm", ""),
        ("/sys/module/pm_stage/parameters/armed", "N"),
    ] {
        same(p, v)?;
    }
    assert!(fs::read_to_string("/sys/power/pm_test")?.contains("[none]"));
    assert!(!std::path::Path::new("/sys/module/pm_boundary_stage").exists());
    assert_eq!(
        out(
            "systemctl",
            &[
                "show",
                "systemd-suspend.service",
                "-p",
                "ActiveState",
                "--value"
            ]
        )?
        .trim(),
        "inactive"
    );
    assert_eq!(
        out(
            "systemctl",
            &[
                "show",
                "systemd-suspend.service",
                "-p",
                "MainPID",
                "--value"
            ]
        )?
        .trim(),
        "0"
    );
    assert!(
        out("systemctl", &["list-jobs", "--no-legend"])?
            .trim()
            .is_empty()
    );
    let failed = out(
        "systemctl",
        &["list-units", "--failed", "--no-legend", "--plain"],
    )?;
    assert_eq!(failed.lines().count(), 1);
    assert_eq!(failed.split_whitespace().next(), Some(UNIT));
    assert_eq!(
        out("systemctl", &["show", UNIT, "-p", "MainPID", "--value"])?.trim(),
        "0"
    );
    for (name, sha) in [
        (
            "restored.txt",
            "33c76ad264bf7b7f766935fb0e1122cb56c9618977aba88645baba6cdc84864e",
        ),
        (
            "outcome.txt",
            "ed6c49b6b4c269651901180e12f4c4a7dff6ca29a1e1d235af6b641f09ac2b4e",
        ),
        (
            "stage-status.txt",
            "8ec9a59873daefb342530ce89044b651a2b3e280d817e260d7d0c7b49af7a48e",
        ),
    ] {
        assert_eq!(
            out(
                "sha256sum",
                &[&format!(
                    "/var/lib/predator-pm-efi-s3/core-efi-20261101-{name}"
                )]
            )?
            .split_whitespace()
            .next(),
            Some(sha)
        );
    }
    let dir = "/var/lib/predator-pm-boundary/core-control-state-review-20261101";
    fs::DirBuilder::new().mode(0o700).create(dir)?;
    save(
        &format!("{dir}/before-unit.txt"),
        &out(
            "systemctl",
            &[
                "show",
                UNIT,
                "-p",
                "ActiveState",
                "-p",
                "Result",
                "-p",
                "MainPID",
                "-p",
                "ExecMainStatus",
            ],
        )?,
    )?;
    save(
        &format!("{dir}/original-journal.jsonl"),
        &out(
            "journalctl",
            &[
                "-b",
                "39d7579bb86d4d30b910131943e550c8",
                "-u",
                UNIT,
                "--no-pager",
                "-o",
                "json",
            ],
        )?,
    )?;
    save(
        &format!("{dir}/intent.txt"),
        &format!(
            "epoch={now}\naction=clear-only-completed-owned-unit-failure-status\noriginal_failure=post-cleanup-exact-btime-guard\ncleanup_repeated=false\npower_transition=false\n"
        ),
    )?;
    out("systemctl", &["reset-failed", UNIT])?;
    assert_eq!(out("systemctl", &["is-system-running"])?.trim(), "running");
    assert!(
        out("systemctl", &["list-jobs", "--no-legend"])?
            .trim()
            .is_empty()
    );
    save(
        &format!("{dir}/completed.txt"),
        "owned-failed-unit-state-cleared; original-files-and-failure-journal-preserved; no-PM-cleanup-repeated\n",
    )?;
    println!("owned_unit_state_repair=PASS");
    Ok(())
}
