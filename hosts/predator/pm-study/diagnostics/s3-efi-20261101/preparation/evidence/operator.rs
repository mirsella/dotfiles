use std::{
    env, fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const SSH: &str = "/home/mirsella/dev/dotfiles/.scratch/pm-resume-20261028/remote/ssh";
const BOOT: &str = "39d7579b-b86d-4d30-b910-131943e550c8";
const BIN: &str = "/nix/store/5142bjnrl7jnyh2b9zb8ac8ki40xmpzm-predator-pm-study-0.1.0/bin/predator-pm-boundary-trial";
const HASH: &str = "12b551f868bb0e0eae8c012bc9e41a0fbbc6e4dc975bf651e3a538e1651ed886";
const SOURCE: &str = "4884bb9d16b073785e441a17f807a6ce130f53e6b43a0a0cbdb9e41cd0a19f7f";
fn need(v: bool, s: &str) -> Result<()> {
    if v { Ok(()) } else { Err(s.into()) }
}
fn save(d: &Path, n: &str, s: &str) -> Result<()> {
    let mut f = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(d.join(n))?;
    f.write_all(s.as_bytes())?;
    f.sync_all()?;
    Ok(())
}
fn hash(p: &Path) -> Result<String> {
    let v = Command::new("sha256sum").arg(p).output()?;
    need(v.status.success(), "hash")?;
    Ok(String::from_utf8(v.stdout)?
        .split_whitespace()
        .next()
        .ok_or("hash missing")?
        .into())
}
fn remote(d: &Path, n: &str, pi: bool, s: &str) -> Result<String> {
    let host = if pi { "192.168.1.166" } else { "192.168.1.19" };
    let boot = if pi {
        "2142644e-84db-40ae-b1b9-9e8c71265cf7"
    } else {
        BOOT
    };
    let body = format!(
        "set -euo pipefail\nexport TZ=Europe/Paris\ntest \"$(date +%F)\" = 2026-11-01\nh=$(date +%H); test \"$h\" -ge 8; test \"$h\" -lt 23\ntest \"$(cat /proc/sys/kernel/random/boot_id)\" = {boot}\n{s}\n"
    );
    save(d, &format!("{n}.sh"), &body)?;
    let mut child = Command::new("timeout")
        .arg("150s")
        .arg(SSH)
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=7",
            "-l",
            "mirsella",
            host,
            "sudo -n bash -s",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("stdin")?
        .write_all(body.as_bytes())?;
    let v = child.wait_with_output()?;
    let stdout = String::from_utf8(v.stdout)?;
    save(d, &format!("{n}.out"), &stdout)?;
    save(
        d,
        &format!("{n}.stderr"),
        &String::from_utf8_lossy(&v.stderr),
    )?;
    save(
        d,
        &format!("{n}.status"),
        &v.status.code().unwrap_or(-1).to_string(),
    )?;
    need(
        v.status.success(),
        &format!("{n} failed; retained receipts"),
    )?;
    Ok(stdout)
}
fn main() -> Result<()> {
    let a: Vec<_> = env::args().collect();
    need(
        a.len() >= 4,
        "prepare|arm|collect DIR core|s3 [DUE CORE_SHA]",
    )?;
    let d = PathBuf::from(&a[2]);
    let core = a[3] == "core";
    need(core || a[3] == "s3", "mode")?;
    let id = if core {
        "core-efi-20261101"
    } else {
        "s3-efi-20261101"
    };
    let nonce = if core { "core1101a" } else { "deep1101a" };
    let unit = format!("predator-pm-boundary-{id}");
    let plan = format!("/var/lib/predator-pm-efi-s3/{id}-plan.conf");
    let logfile = "/home/mirsella/dev/predator-pm-debug/netconsole-20261006.log";
    let base = format!("/home/mirsella/dev/predator-pm-debug/.agents-local/{id}-baseline.log");
    let result = format!("/home/mirsella/dev/predator-pm-debug/.agents-local/{id}-result.log");
    match a[1].as_str() {
        "prepare" => {
            need(a.len() == 6, "prepare requires due and core SHA/none")?;
            let due: u64 = a[4].parse()?;
            need((1793535600..=1793566800).contains(&due), "due")?;
            need(
                if core {
                    a[5] == "none"
                } else {
                    a[5].len() == 64 && a[5].bytes().all(|b| b.is_ascii_hexdigit())
                },
                "core SHA",
            )?;
            fs::create_dir(&d)?;
            let manifest = format!(
                "schema=efi-s3-20261101-1\nid={id}\nnonce={nonce}\nboot={BOOT}\nbtime=1793535078\nmode={}\ndue={due}\nrtc={}\nreview={}\nruntime={BIN}\nruntime_sha={HASH}\nsource_sha={SOURCE}\nmodule=/nix/store/ymy989dq8bxys9j2jkanpc3d80qmvliy-predator-pm-boundary-stage-20261101/lib/modules/6.18.58/extra/pm_boundary_stage.ko.xz\nmodule_sha=90535c76374d5e2c29ba03780949b8917084c01fb6a610c0427a32e3471627c0\nsuccess_before={}\ncore_completion_sha={}\nretention_sha=176cbbc62c46cd68c977a07d6c3f775d770ff0983176190646c17f9c18d37b63\nowner_ready=owner-confirmed-ready-20261101\npm_async=0\npm_trace=0\npm_test={}\n",
                a[3],
                if core { 0 } else { due + 300 },
                due + if core { 900 } else { 600 },
                u8::from(!core),
                a[5],
                if core { "core" } else { "none" }
            );
            save(&d, "plan.conf", &manifest)?;
            let sha = hash(&d.join("plan.conf"))?;
            save(&d, "manifest.sha256", &sha)?;
            remote(
                &d,
                "stage",
                false,
                &format!(
                    "date --iso-8601=ns\ntest -z \"$(systemctl list-jobs --no-legend)\"\ntest -z \"$(systemctl list-timers --all --no-legend | grep -E 'predator-pm-(boundary|path|wake|diagnostic|instrumented|s2idle)-' | grep -v '^-' || true)\"\ninstall -d -m700 /var/lib/predator-pm-efi-s3\numask 077\nset -o noclobber\ncat > {plan} <<'PLAN'\n{manifest}PLAN\n{BIN} check {plan} {sha}\njournalctl --sync"
                ),
            )?;
            let proof = remote(
                &d,
                "observer-proof",
                true,
                &format!(
                    "date --iso-8601=ns\ntest \"$(timedatectl show -p NTPSynchronized --value)\" = yes\ntimedatectl timesync-status\nsystemctl is-active predator-netconsole-20261006.service\ntest -z \"$(systemctl list-jobs --no-legend)\"\ngrep -F 'BOUNDARY READY id={id} nonce={nonce} boot={BOOT}' {logfile} | tail -1\ntest ! -e {base}; test ! -e {result}\ncp --no-clobber {logfile} {base}\nsha256sum {base}\nsha256sum /home/mirsella/wol_predator.py\n"
                ),
            )?;
            let expected = proof
                .lines()
                .find_map(|l| l.strip_suffix(&format!("  {base}")))
                .ok_or("baseline digest")?;
            let bytes = remote(&d, "baseline", true, &format!("cat {base}"))?;
            save(&d, "frozen-pi-baseline.log", &bytes)?;
            need(
                hash(&d.join("frozen-pi-baseline.log"))? == expected,
                "independent baseline hash",
            )?;
            save(
                &d,
                "prepared.txt",
                "full check, nonce and independent baseline verified; unarmed",
            )?;
        }
        "arm" => {
            need(d.join("prepared.txt").exists(), "not prepared")?;
            let manifest = fs::read_to_string(d.join("plan.conf"))?;
            let get = |k: &str| -> Result<String> {
                Ok(manifest
                    .lines()
                    .find_map(|l| l.strip_prefix(&format!("{k}=")))
                    .ok_or("field")?
                    .into())
            };
            let due: u64 = get("due")?.parse()?;
            let review: u64 = get("review")?.parse()?;
            let sha = hash(&d.join("plan.conf"))?;
            need(
                sha == fs::read_to_string(d.join("manifest.sha256"))?,
                "manifest changed",
            )?;
            need(
                fs::read_to_string(d.join("owner-notified.txt"))?
                    .starts_with("Owner notified in current thread:"),
                "owner notification missing",
            )?;
            let check=Command::new("jq").args(["-e","--argjson","review",&review.to_string(),".enabled == true and .readOnly == true and .deleteExactTaskAtStart == true and .reviewEpoch == $review and .boundThreadId == \"309aecdf-080d-5358-9ce9-8360afbd7c4d\" and .scheduledTaskId != null and ((now|floor)-.verifiedAtEpoch >= 0) and ((now|floor)-.verifiedAtEpoch < 120)"]).arg(d.join("review-schedule.json")).status()?;
            need(
                check.success(),
                "fresh scheduled read-only reviewer missing",
            )?;
            let probe = due + if core { 180 } else { 390 };
            let freeze = due + if core { 240 } else { 480 };
            remote(
                &d,
                "observer-arm",
                true,
                &format!(
                    "date --iso-8601=ns\ntest $(date +%s) -lt $(({due}-300))\ntest \"$(timedatectl show -p NTPSynchronized --value)\" = yes\nsystemctl is-active predator-netconsole-20261006.service\ntest -z \"$(systemctl list-jobs --no-legend)\"\nsystemd-run --unit={unit}-probe --on-calendar=@{probe} --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=Restart=no --property=RuntimeMaxSec=30s --property=TimeoutStartSec=20s /bin/bash -c 'date --iso-8601=ns; timeout 15s ssh-keyscan -T 7 -t ed25519 192.168.1.19; date --iso-8601=ns'\nsystemd-run --unit={unit}-snapshot --on-calendar=@{freeze} --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=Restart=no --property=RuntimeMaxSec=30s --property=TimeoutStartSec=20s /bin/cp --no-clobber {logfile} {result}\nsystemctl list-timers --all --no-pager '{unit}-*'"
                ),
            )?;
            remote(
                &d,
                "primary-arm",
                false,
                &format!(
                    "date --iso-8601=ns\n{BIN} check {plan} {sha}\ntest ! -e /var/lib/predator-pm-efi-s3/{id}-requested.txt\nsystemd-run --unit={unit} --on-calendar=@{due} --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=Restart=no --property=RuntimeMaxSec=900s --property=TimeoutStartSec=30s --property=TimeoutStopSec=180s --property='Environment=PATH=/run/current-system/sw/bin' --property='ExecStopPost={BIN} restore {plan} {sha}' {BIN} run {plan} {sha}\nsystemctl show {unit}.service -p ActiveState -p MainPID -p ExecStart -p ExecStopPost -p Restart -p RuntimeMaxUSec\nsystemctl list-timers --all --no-pager {unit}.timer\ntest \"$(systemctl show {unit}.service -p ActiveState --value)\" = inactive\ntest ! -e /var/lib/predator-pm-efi-s3/{id}-requested.txt"
                ),
            )?;
            save(
                &d,
                "armed.txt",
                &format!("id={id}\ndue={due}\nreview={review}\nno_WoL=true\n"),
            )?;
        }
        "collect" => {
            remote(
                &d,
                "observer-result",
                true,
                &format!(
                    "date --iso-8601=ns\nsha256sum {base} {result}\ncat {result}\njournalctl -u {unit}-probe.service -u {unit}-snapshot.service --no-pager -o short-iso-precise\nsystemctl list-timers --all --no-pager '{unit}-*'"
                ),
            )?;
            remote(
                &d,
                "primary-result",
                false,
                &format!(
                    "date --iso-8601=ns\ncat /proc/sys/kernel/random/boot_id; grep '^btime ' /proc/stat\nsystemctl show {unit}.service systemd-suspend.service -p ActiveState -p MainPID -p Result -p ExecMainStatus\nsystemctl list-jobs --no-pager\ncat /sys/power/suspend_stats/{{success,fail}} /sys/power/pm_async /sys/power/pm_trace /sys/power/pm_test /sys/power/mem_sleep /sys/class/rtc/rtc0/wakealarm\nstat -c '%U %a %s %n' /var/lib/predator-pm-efi-s3/{id}-*\nsha256sum /var/lib/predator-pm-efi-s3/{id}-*\nfor f in /var/lib/predator-pm-efi-s3/{id}-*.txt /var/lib/predator-pm-efi-s3/{id}-*.json; do test -f \"$f\" || continue; echo \"FILE $f\"; cat \"$f\"; done\njournalctl -u {unit}.service --no-pager -o short-iso-precise"
                ),
            )?;
        }
        _ => return Err("unknown operation".into()),
    }
    Ok(())
}
