use std::{
    env, fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    process::{Command, Stdio},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const PKG: &str = "/nix/store/glb3dymq7hxaym5dg6602m1h3isjf17n-predator-pm-study-0.1.0";
const BOOT: &str = "ae29f675-9167-4975-a884-01885468bc38";
const SSH: &str = "/home/mirsella/dev/dotfiles/.scratch/pm-resume-20261028/remote/ssh";
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
fn remote(d: &Path, ip: &str, n: &str, s: &str) -> Result<Vec<u8>> {
    save(&d.join(format!("{n}.sh")), s.as_bytes())?;
    let mut c = Command::new("timeout")
        .args([
            "150s",
            SSH,
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=7",
            "-p",
            "22",
            "-l",
            "mirsella",
            ip,
            "sudo -n bash -s",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    c.stdin.take().unwrap().write_all(s.as_bytes())?;
    let o = c.wait_with_output()?;
    save(&d.join(format!("{n}.out")), &o.stdout)?;
    save(&d.join(format!("{n}.stderr")), &o.stderr)?;
    save(
        &d.join(format!("{n}.status")),
        o.status.to_string().as_bytes(),
    )?;
    check(
        o.status.success(),
        &format!("{n}: {} {}", o.status, String::from_utf8_lossy(&o.stderr)),
    )?;
    Ok(o.stdout)
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
fn main() -> Result<()> {
    let a: Vec<_> = env::args().collect();
    check(a.len() >= 4, "prepare|arm|collect DIRECTORY ID [DUE BTIME]")?;
    let d = Path::new(&a[2]);
    let id = &a[3];
    check(
        !id.is_empty()
            && id.len() <= 80
            && id
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'),
        "bad id",
    )?;
    let unit = format!("predator-pm-s2idle-{id}");
    let planpath = format!("/var/lib/predator-pm-s2idle/{id}-plan.conf");
    match a[1].as_str() {
        "prepare" => {
            check(a.len() == 6, "prepare args")?;
            fs::create_dir(d)?;
            let due: u64 = a[4].parse()?;
            let btime: u64 = a[5].parse()?;
            let wake = due.checked_add(960).ok_or("wake overflow")?;
            let nonce = format!("{id}-{due}");
            let plan = format!(
                "schema=rtc-s2idle-control-1\nid={id}\nboot={BOOT}\nruntime={PKG}/bin/predator-pm-s2idle\nruntime_sha=27c4152e2577d3569027fa479a412cb08e63f41a63d99755b45a6e17eaad5a33\nsource_sha=618ecaee583e6b2a2a1489649418731ddbfcad3c65f2e9d109772f4c6db602df\nhandoff_sha=1e7247c6e7e90a22fa41d022afb9e45001bf1328e69ec1440641f1c919518ee4\ndue={due}\nwake={wake}\nwindow_end=1793484000\nbtime={btime}\nhistorical_btime=1793440484\nsuccess_before=0\nmarkers_before=0\nkind=s2idle-control\nrecovery_sha=b752bc4051bce5ed70ae71de0360ac566b078e8609dc0a6e8edd8e0b5f807f55\npm_async=0\npm_trace=0\npm_test=none\nmem_sleep=s2idle\nrecorder=rtc-unsync-zerooffset\ntpm=initialized\nec=not-run\nmeasurement=dated-power-graph-required\nreset_provenance=owner-forced-reboot-method-off-duration-unspecified\nnonce={nonce}\n"
            );
            save(&d.join("plan.conf"), plan.as_bytes())?;
            let digest = hash(&d.join("plan.conf"))?;
            save(&d.join("plan.sha256"), digest.as_bytes())?;
            remote(
                d,
                "192.168.1.19",
                "stage",
                &format!(
                    "set -euo pipefail\ndate --iso-8601=seconds\ntest \"$(cat /proc/sys/kernel/random/boot_id)\" = {BOOT}\ntest -z \"$(systemctl list-timers --all --no-legend 'predator-pm-*' night-suspend.timer | awk '$1 != \"-\" {{print}}')\"\nsystemctl list-timers --all --no-pager nextcloud-cron.timer\nsystemctl show nextcloud-cron.service -p ActiveState -p ExecMainStartTimestamp -p ExecMainExitTimestamp\nsystemctl list-jobs --no-legend\ninstall -d -m0700 /var/lib/predator-pm-s2idle\numask 077\nset -o noclobber\ncat > {planpath} <<'PLAN'\n{plan}PLAN\n{PKG}/bin/predator-pm-s2idle check {planpath} {digest}\njournalctl --sync\n"
                ),
            )?;
            remote(
                d,
                "192.168.1.166",
                "observer-proof",
                &format!(
                    "set -euo pipefail\ndate --iso-8601=seconds\ncat /proc/sys/kernel/random/boot_id\ntest \"$(timedatectl show -p NTPSynchronized --value)\" = yes\ntimedatectl timesync-status\nsystemctl is-active predator-netconsole-20261006.service\ntest -z \"$(systemctl list-jobs --no-legend)\"\ntest -z \"$(systemctl list-timers --all --no-legend 'predator-pm-*' | awk '$1 != \"-\" {{print}}')\"\nprintf 'ca4666560dc9cd468f3b9b6c4c68a7dcb2675b561844ca5ef6a89fa9944722e8  /home/mirsella/wol_predator.py\\n' | sha256sum -c -\nlive=/home/mirsella/dev/predator-pm-debug/netconsole-20261006.log\nbase=/home/mirsella/dev/predator-pm-debug/.agents-local/{id}\ngrep -F 'S2IDLE READY id={id} nonce={nonce}' \"$live\"\ntest ! -e \"$base-baseline.log\"\ntest ! -e \"$base-result.log\"\ncp --no-clobber \"$live\" \"$base-baseline.log\"\nsha256sum \"$base-baseline.log\"\n"
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
            let digest = fs::read_to_string(d.join("plan.sha256"))?;
            check(
                d.join("review-schedule.json").exists()
                    && fs::read_to_string(d.join("owner-notified.txt"))?
                        .contains("Telegram notification delivered"),
                "review/notice not recorded",
            )?;
            let opts = "--timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=TimeoutStartSec=20s --property=RuntimeMaxSec=30s";
            let mut s = format!(
                "set -euo pipefail\ndate --iso-8601=seconds\ntest \"$(date +%s)\" -lt {}\ntest \"$(timedatectl show -p NTPSynchronized --value)\" = yes\nsystemctl is-active predator-netconsole-20261006.service\nprintf 'ca4666560dc9cd468f3b9b6c4c68a7dcb2675b561844ca5ef6a89fa9944722e8  /home/mirsella/wol_predator.py\\n' | sha256sum -c -\n",
                due - 300
            );
            for (suffix, t) in [("wake", wake), ("wake-retry", wake + 30)] {
                s.push_str(&format!("systemd-run --unit={unit}-{suffix} --on-calendar=@{t} {opts} /usr/bin/python3 /home/mirsella/wol_predator.py\n"));
            }
            s.push_str(&format!("systemd-run --unit={unit}-probe --on-calendar=@{} {opts} /usr/bin/timeout 15s /usr/bin/ssh-keyscan -T 5 -t ed25519 192.168.1.19\nsystemd-run --unit={unit}-snapshot --on-calendar=@{} {opts} /usr/bin/cp --no-clobber /home/mirsella/dev/predator-pm-debug/netconsole-20261006.log /home/mirsella/dev/predator-pm-debug/.agents-local/{id}-result.log\nsystemctl list-timers --all --no-pager '{unit}*'\n",wake+90,wake+180));
            remote(d, "192.168.1.166", "observer-arm", &s)?;
            remote(
                d,
                "192.168.1.19",
                "primary-arm",
                &format!(
                    "set -euo pipefail\ndate --iso-8601=seconds\n{PKG}/bin/predator-pm-s2idle check {planpath} {digest}\nsystemctl list-timers --all --no-pager nextcloud-cron.timer\ntest ! -e /var/lib/predator-pm-s2idle/{id}-requested.txt\nsystemd-run --unit={unit} --on-calendar=@{due} --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=TimeoutStartSec=30s --property=RuntimeMaxSec=2400s --property=TimeoutStopSec=180s --property=Environment=PATH=/run/current-system/sw/bin --property='ExecStopPost={PKG}/bin/predator-pm-s2idle restore {planpath} {digest}' {PKG}/bin/predator-pm-s2idle run {planpath} {digest}\nsystemctl show {unit}.service -p Type -p ActiveState -p RuntimeMaxUSec -p TimeoutStopUSec -p ExecStopPost\nsystemctl list-timers --all --no-pager {unit}.timer\ntest \"$(systemctl show {unit}.service -p ActiveState --value)\" = inactive\ntest ! -e /var/lib/predator-pm-s2idle/{id}-requested.txt\n"
                ),
            )?;
            save(&d.join("armed"),b"one s2idle primary; Pi wake/retry/probe/snapshot; independent dated review recorded\n")?;
        }
        "collect" => {
            check(a.len() == 4, "collect args")?;
            remote(
                d,
                "192.168.1.166",
                "observer-result",
                &format!(
                    "set -euo pipefail\ndate --iso-8601=seconds\nsha256sum /home/mirsella/dev/predator-pm-debug/.agents-local/{id}-baseline.log /home/mirsella/dev/predator-pm-debug/.agents-local/{id}-result.log\ncat /home/mirsella/dev/predator-pm-debug/.agents-local/{id}-result.log\njournalctl -b --no-pager -o short-iso-precise -u {unit}-probe.service -u {unit}-snapshot.service -u {unit}-wake.service -u {unit}-wake-retry.service\nsystemctl list-timers --all --no-pager '{unit}*'\n"
                ),
            )?;
            remote(
                d,
                "192.168.1.19",
                "primary-result",
                &format!(
                    "set -euo pipefail\ndate --iso-8601=seconds\ncat /proc/sys/kernel/random/boot_id\ncat /proc/uptime\ngrep '^btime ' /proc/stat\nsystemctl show {unit}.service -p ActiveState -p Result -p ExecMainStatus\nsystemctl list-jobs --no-legend\ncat /sys/power/suspend_stats/{{success,fail}} /sys/power/{{pm_test,pm_async,pm_trace,mem_sleep}} /sys/module/pm_stage/parameters/{{armed,markers,last_action,last_value,last_start}}\nfor f in /var/lib/predator-pm-s2idle/{id}-*.txt; do printf '\\nFILE %s\\n' \"$f\"; stat -c '%U %a' \"$f\"; cat \"$f\"; done\nsha256sum /var/lib/predator-pm-s2idle/{id}-*\njournalctl -b --no-pager -o short-monotonic -u {unit}.service\n"
                ),
            )?;
        }
        _ => return Err("unknown operation".into()),
    }
    Ok(())
}
