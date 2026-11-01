use std::{
    env, fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const PKG: &str = "/nix/store/qar5vnhq2bk1vs3rr7g1z8kwa31bnwn4-predator-pm-study-0.1.0";
const BOOT: &str = "f9b45aa6-b820-4ab5-b18c-9324d8d53087";
const SSH: &str = "/home/mirsella/dev/dotfiles/.scratch/pm-resume-20261028/remote/ssh";
const ID: &str = "path-d3-20261101";
fn check(b: bool, s: &str) -> Result<()> {
    if b { Ok(()) } else { Err(s.into()) }
}
fn save(p: &Path, s: &[u8]) -> Result<()> {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(p)?;
    f.write_all(s)?;
    f.sync_all()?;
    Ok(())
}
fn hash(p: &Path) -> Result<String> {
    let o = Command::new("sha256sum").arg(p).output()?;
    check(o.status.success(), "hash failed")?;
    Ok(String::from_utf8(o.stdout)?
        .split_whitespace()
        .next()
        .ok_or("no digest")?
        .into())
}
fn remote(d: &Path, ip: &str, name: &str, script: &str) -> Result<Vec<u8>> {
    let boot = match ip {
        "192.168.1.19" => BOOT,
        "192.168.1.166" => "2142644e-84db-40ae-b1b9-9e8c71265cf7",
        _ => return Err("unknown host".into()),
    };
    let stamp = if name == "baseline" {
        ""
    } else {
        "date --iso-8601=ns"
    };
    let script = format!(
        r#"set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-11-01
h=$(TZ=Europe/Paris date +%H)
test "$h" -ge 8 && test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = {boot}
{stamp}
{script}
"#
    );
    save(&d.join(format!("{name}.sh")), script.as_bytes())?;
    let mut c = Command::new("timeout")
        .args([
            "150s",
            SSH,
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=7",
            "-l",
            "mirsella",
            ip,
            "sudo -n bash -s",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    c.stdin
        .take()
        .ok_or("no SSH stdin")?
        .write_all(script.as_bytes())?;
    let o = c.wait_with_output()?;
    save(&d.join(format!("{name}.out")), &o.stdout)?;
    save(&d.join(format!("{name}.stderr")), &o.stderr)?;
    save(
        &d.join(format!("{name}.status")),
        o.status.to_string().as_bytes(),
    )?;
    check(
        o.status.success(),
        &format!(
            "{name}: {} {}",
            o.status,
            String::from_utf8_lossy(&o.stderr)
        ),
    )?;
    Ok(o.stdout)
}
fn main() -> Result<()> {
    let a: Vec<_> = env::args().collect();
    check(
        a.len() >= 4 && a[3] == ID,
        "prepare|arm|collect DIRECTORY path-d3-20261101 [DUE BTIME]",
    )?;
    let d = Path::new(&a[2]);
    let id = ID;
    let unit = format!("predator-pm-path-{id}");
    let planpath = format!("/var/lib/predator-pm-wake-path/{id}-plan.conf");
    match a[1].as_str() {
        "prepare" => {
            check(a.len() == 6 && a[5] == "1793474520", "prepare history/args")?;
            let due: u64 = a[4].parse()?;
            check(
                (1793516400..=1793523600).contains(&due),
                "date/latest-entry guard",
            )?;
            fs::create_dir(d)?;
            let wake = due + 960;
            let fallback = due + 1200;
            let nonce = format!("{id}-{due}");
            let plan = include_str!("plan-template.conf")
                .replace("@DUE@", &due.to_string())
                .replace("@WAKE@", &wake.to_string())
                .replace("@FALLBACK@", &fallback.to_string());
            save(&d.join("plan.conf"), plan.as_bytes())?;
            let digest = hash(&d.join("plan.conf"))?;
            save(&d.join("plan.sha256"), digest.as_bytes())?;
            remote(
                d,
                "192.168.1.19",
                "stage",
                &format!(
                    r#"
test -z "$(systemctl list-timers --all --no-legend 'predator-pm-*' night-suspend.timer | awk '$1 != "-" {{print}}')"
systemctl list-timers --all --no-pager nextcloud-cron.timer
systemctl show nextcloud-cron.service -p ActiveState -p ExecMainStartTimestamp -p ExecMainExitTimestamp
systemctl list-jobs --no-legend
install -d -m0700 /var/lib/predator-pm-wake-path
umask 077
set -o noclobber
cat > {planpath} <<'PLAN'
{plan}PLAN
{PKG}/bin/predator-pm-wake-path check {planpath} {digest}
journalctl --sync
"#
                ),
            )?;
            remote(
                d,
                "192.168.1.166",
                "observer-proof",
                &format!(
                    r#"
test "$(timedatectl show -p NTPSynchronized --value)" = yes
timedatectl timesync-status
systemctl is-active predator-netconsole-20261006.service
test -z "$(systemctl list-jobs --no-legend)"
test -z "$(systemctl list-timers --all --no-legend 'predator-pm-*' | awk '$1 != "-" {{print}}')"
printf 'ca4666560dc9cd468f3b9b6c4c68a7dcb2675b561844ca5ef6a89fa9944722e8  /home/mirsella/wol_predator.py\n' | sha256sum -c -
live=/home/mirsella/dev/predator-pm-debug/netconsole-20261006.log
base=/home/mirsella/dev/predator-pm-debug/.agents-local/{id}
grep -F 'WAKEPATH READY id={id} nonce={nonce}' "$live"
test ! -e "$base-baseline.log"
test ! -e "$base-result.log"
cp --no-clobber "$live" "$base-baseline.log"
sha256sum "$base-baseline.log"
"#
                ),
            )?;
            remote(
                d,
                "192.168.1.166",
                "baseline",
                &format!(
                    "cat /home/mirsella/dev/predator-pm-debug/.agents-local/{id}-baseline.log"
                ),
            )?;
            check(
                fs::read_to_string(d.join("observer-proof.out"))?
                    .contains(&hash(&d.join("baseline.out"))?),
                "frozen baseline mismatch",
            )?;
            save(
                &d.join("prepared"),
                b"manifest checked; frozen Pi nonce verified; no timers armed\n",
            )?;
        }
        "arm" => {
            check(
                a.len() == 4 && d.join("prepared").exists(),
                "not prepared/invalid args",
            )?;
            let p = fs::read_to_string(d.join("plan.conf"))?;
            let get = |k: &str| -> Result<&str> {
                p.lines()
                    .find_map(|l| l.strip_prefix(&format!("{k}=")))
                    .ok_or_else(|| format!("missing {k}").into())
            };
            let due: u64 = get("due")?.parse()?;
            let wake: u64 = get("wake")?.parse()?;
            let fallback: u64 = get("rtc_fallback")?.parse()?;
            let digest = fs::read_to_string(d.join("plan.sha256"))?;
            check(hash(&d.join("plan.conf"))? == digest, "manifest changed")?;
            check(
                fs::read_to_string(d.join("owner-notified.txt"))?
                    .starts_with("Owner notified in current thread:"),
                "owner notice missing",
            )?;
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
            let reviewed = Command::new("jq").args(["-e", "--argjson", "now", &now.to_string(), r#".enabled == true and .scheduledTaskId == "scheduled-task:command:mcp:bc08a214-b602-4fa3-9d4e-6f808b4422d0:schedule-task:predator-wake-path-followup-20261101-1030" and .nextRunAt == "2026-11-01T09:30:00.000Z" and .boundThreadId == "309aecdf-080d-5358-9ce9-8360afbd7c4d" and .verifiedAtEpoch <= $now and $now - .verifiedAtEpoch < 120"#]).arg(d.join("review-schedule.json")).output()?;
            check(
                reviewed.status.success(),
                "independent followup receipt missing/stale/wrong",
            )?;
            let opts = "--timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=Restart=no --property=TimeoutStartSec=20s --property=RuntimeMaxSec=30s";
            let mut script = format!(
                r#"
test "$(date +%s)" -lt {}
test "$(timedatectl show -p NTPSynchronized --value)" = yes
systemctl is-active predator-netconsole-20261006.service
test -z "$(systemctl list-jobs --no-legend)"
printf 'ca4666560dc9cd468f3b9b6c4c68a7dcb2675b561844ca5ef6a89fa9944722e8  /home/mirsella/wol_predator.py\n' | sha256sum -c -
"#,
                due - 300
            );
            for (suffix, t) in [("wake", wake), ("wake-retry", wake + 30)] {
                script.push_str(&format!("systemd-run --unit={unit}-{suffix} --on-calendar=@{t} {opts} /usr/bin/python3 /home/mirsella/wol_predator.py\n"));
            }
            script.push_str(&format!("systemd-run --unit={unit}-probe --on-calendar=@{} {opts} /usr/bin/timeout 15s /usr/bin/ssh-keyscan -T 5 -t ed25519 192.168.1.19\nsystemd-run --unit={unit}-snapshot --on-calendar=@{} {opts} /usr/bin/cp --no-clobber /home/mirsella/dev/predator-pm-debug/netconsole-20261006.log /home/mirsella/dev/predator-pm-debug/.agents-local/{id}-result.log\nsystemctl list-timers --all --no-pager '{unit}*'\n", fallback+90, fallback+180));
            remote(d, "192.168.1.166", "observer-arm", &script)?;
            remote(
                d,
                "192.168.1.19",
                "primary-arm",
                &format!(
                    r#"
{PKG}/bin/predator-pm-wake-path check {planpath} {digest}
systemctl list-timers --all --no-pager nextcloud-cron.timer
test ! -e /var/lib/predator-pm-wake-path/{id}-requested.txt
systemd-run --unit={unit} --on-calendar=@{due} --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=Restart=no --property=TimeoutStartSec=30s --property=RuntimeMaxSec=2400s --property=TimeoutStopSec=180s --property=Environment=PATH=/run/current-system/sw/bin --property='ExecStopPost={PKG}/bin/predator-pm-wake-path restore {planpath} {digest}' {PKG}/bin/predator-pm-wake-path run {planpath} {digest}
systemctl show {unit}.service -p Type -p ActiveState -p RuntimeMaxUSec -p TimeoutStopUSec -p ExecStopPost -p Restart
systemctl list-timers --all --no-pager {unit}.timer
test "$(systemctl show {unit}.service -p ActiveState --value)" = inactive
test ! -e /var/lib/predator-pm-wake-path/{id}-requested.txt
"#
                ),
            )?;
            save(&d.join("armed"),b"one single-variable s2idle primary; Pi wake/retry/probe/snapshot; enabled 10:30 reviewer verified; no automatic retry\n")?;
        }
        "collect" => {
            check(a.len() == 4, "collect args")?;
            remote(
                d,
                "192.168.1.166",
                "observer-result",
                &format!(
                    r#"
sha256sum /home/mirsella/dev/predator-pm-debug/.agents-local/{id}-baseline.log /home/mirsella/dev/predator-pm-debug/.agents-local/{id}-result.log
cat /home/mirsella/dev/predator-pm-debug/.agents-local/{id}-result.log
journalctl -b --no-pager -o short-iso-precise -u {unit}-probe.service -u {unit}-snapshot.service -u {unit}-wake.service -u {unit}-wake-retry.service
systemctl list-timers --all --no-pager '{unit}*'
"#
                ),
            )?;
            remote(
                d,
                "192.168.1.19",
                "primary-result",
                &format!(
                    r#"
cat /proc/sys/kernel/random/boot_id
cat /proc/uptime
grep '^btime ' /proc/stat
systemctl show {unit}.service -p ActiveState -p MainPID -p Result -p ExecMainStatus
systemctl list-jobs --no-legend
cat /sys/power/suspend_stats/{{success,fail}} /sys/power/{{pm_test,pm_async,pm_trace,mem_sleep}} /sys/module/pm_stage/parameters/{{armed,markers,last_action,last_value,last_start}}
cat /sys/class/rtc/rtc0/wakealarm
for f in /var/lib/predator-pm-wake-path/{id}-*.txt; do printf '\nFILE %s\n' "$f"; stat -c '%U %a' "$f"; cat "$f"; done
sha256sum /var/lib/predator-pm-wake-path/{id}-*
journalctl -b --no-pager -o short-iso-precise -u {unit}.service
"#
                ),
            )?;
        }
        _ => return Err("unknown operation".into()),
    }
    Ok(())
}
