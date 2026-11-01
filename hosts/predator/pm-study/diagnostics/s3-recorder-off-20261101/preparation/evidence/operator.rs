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
const BIN: &str = "/nix/store/nbxvgd86xdy14jlvyn09h858gldgpqg4-predator-pm-study-0.1.0/bin/predator-pm-recorder-off";
const HASH: &str = "94b44bc3d6c5a4666372cee0cb47176966b2d684060bd26145fe856975d5d377";
const SOURCE: &str = "a7285638fc2cef87ff86e5cec7a617093265f075e6dd216c7a7923bad1d7c270";
const STATE: &str = "/var/lib/predator-pm-recorder-off";
const REVIEW_SHA: &str = "78a64967499872a4cd565d4c4788402b55432fb418c884b9c297d6a93af31896";
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
    need(!core && a[3] == "s3", "reviewed S3 only")?;
    let id = if core {
        "core-efi-20261101"
    } else {
        "off-s3-20261101"
    };
    let nonce = "off1101a";
    let unit = format!("predator-pm-recorder-off-{id}");
    let plan = format!("{STATE}/{id}-plan.conf");
    let logfile = "/home/mirsella/dev/predator-pm-debug/netconsole-20261006.log";
    let base = format!("/home/mirsella/dev/predator-pm-debug/.agents-local/{id}-baseline.log");
    let result = format!("/home/mirsella/dev/predator-pm-debug/.agents-local/{id}-result.log");
    match a[1].as_str() {
        "prepare" => {
            need(a.len() == 6, "prepare requires due and core SHA/none")?;
            let due: u64 = a[4].parse()?;
            need(
                (1793546100..=1793546400).contains(&due),
                "owner availability window",
            )?;
            need(
                if core {
                    a[5] == "none"
                } else {
                    a[5].len() == 64 && a[5].bytes().all(|b| b.is_ascii_hexdigit())
                },
                "core SHA",
            )?;
            fs::create_dir(&d)?;
            need(a[5] == REVIEW_SHA, "wrong independent core review")?;
            let manifest = format!(
                "schema=recorder-off-s3-20261101-1\nid={id}\nnonce={nonce}\nboot={BOOT}\nbtime=1793535079\nmode=s3\ndue={due}\nrtc={}\nreview=1793547000\nruntime={BIN}\nruntime_sha={HASH}\nsource_sha={SOURCE}\nrecorder=absent\nefi_writes=0\nsuccess_before=2\ncore_review_sha={REVIEW_SHA}\nprior_completion_sha=4e303475cf76fe2f972237f93ba15d18c5a77c6a52a9f6685e284eb1dbc96121\nmeter_scope=owner-confirmed-identical-laptop-and-peripherals\nhypothesis=late-EFI-recording-may-perturb-S3-power-entry\nprediction=lower-same-load-power-supports-instrumentation-sensitivity-not-unique-causality\nrollback=owned-RTC-alarm-clear-and-async1-after-inactive-sleep\nretention_sha=176cbbc62c46cd68c977a07d6c3f775d770ff0983176190646c17f9c18d37b63\nowner_ready=owner-confirmed-1615-1630-20261101\npm_async=0\npm_trace=0\npm_test=none\n",
                due + 300
            );
            save(&d, "plan.conf", &manifest)?;
            let sha = hash(&d.join("plan.conf"))?;
            save(&d, "manifest.sha256", &sha)?;
            remote(
                &d,
                "stage",
                false,
                &format!(
                    "date --iso-8601=ns\ntest -z \"$(systemctl list-jobs --no-legend)\"\ntest -z \"$(systemctl list-timers --all --no-legend | grep -E 'predator-pm-(recorder-off|boundary|path|wake|diagnostic|instrumented|s2idle)-' | grep -v '^-' || true)\"\ninstall -d -m700 {STATE}\numask 077\nset -o noclobber\ncat > {plan} <<'PLAN'\n{manifest}PLAN\n{BIN} check {plan} {sha}\njournalctl --sync"
                ),
            )?;
            let proof = remote(
                &d,
                "observer-proof",
                true,
                &format!(
                    "date --iso-8601=ns\ntest \"$(timedatectl show -p NTPSynchronized --value)\" = yes\ntimedatectl timesync-status\nsystemctl is-active predator-netconsole-20261006.service\ntest -z \"$(systemctl list-jobs --no-legend)\"\ngrep -F 'RECORDER_OFF READY id={id} nonce={nonce} boot={BOOT}' {logfile} | tail -1\ntest ! -e {base}; test ! -e {result}\ncp --no-clobber {logfile} {base}\nsha256sum {base}\nsha256sum /home/mirsella/wol_predator.py\n"
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
            let check=Command::new("jq").args(["-e","--argjson","review",&review.to_string(),".enabled == true and .readOnly == true and .deleteExactTaskAtStart == true and .reviewEpoch == $review and .boundThreadId == \"309aecdf-080d-5358-9ce9-8360afbd7c4d\" and .scheduledTaskId == \"scheduled-task:command:mcp:b1149009-6b28-4c5e-ae12-47335ef3ad69:schedule-task:predator-recorder-off-review-20261101-1630\" and ((now|floor)-.verifiedAtEpoch >= 0) and ((now|floor)-.verifiedAtEpoch < 120)"]).arg(d.join("review-schedule.json")).status()?;
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
                    "date --iso-8601=ns\n{BIN} check {plan} {sha}\ntest ! -e {STATE}/{id}-requested.txt\nsystemd-run --unit={unit} --on-calendar=@{due} --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=Restart=no --property=RuntimeMaxSec=900s --property=TimeoutStartSec=30s --property=TimeoutStopSec=180s --property='Environment=PATH=/run/current-system/sw/bin' --property='ExecStopPost={BIN} restore {plan} {sha}' {BIN} run {plan} {sha}\nsystemctl show {unit}.service -p ActiveState -p MainPID -p ExecStart -p ExecStopPost -p Restart -p RuntimeMaxUSec\nsystemctl list-timers --all --no-pager {unit}.timer\ntest \"$(systemctl show {unit}.service -p ActiveState --value)\" = inactive\ntest ! -e {STATE}/{id}-requested.txt"
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
                    "date --iso-8601=ns\ncat /proc/sys/kernel/random/boot_id; grep '^btime ' /proc/stat\nsystemctl show {unit}.service systemd-suspend.service -p ActiveState -p MainPID -p Result -p ExecMainStatus\nsystemctl list-jobs --no-pager\ncat /sys/power/suspend_stats/{{success,fail}} /sys/power/pm_async /sys/power/pm_trace /sys/power/pm_test /sys/power/mem_sleep /sys/class/rtc/rtc0/wakealarm\nstat -c '%U %a %s %n' {STATE}/{id}-*\nsha256sum {STATE}/{id}-*\nfor f in {STATE}/{id}-*.txt {STATE}/{id}-*.json; do test -f \"$f\" || continue; echo \"FILE $f\"; cat \"$f\"; done\njournalctl -u {unit}.service --no-pager -o short-iso-precise"
                ),
            )?;
        }
        _ => return Err("unknown operation".into()),
    }
    Ok(())
}
