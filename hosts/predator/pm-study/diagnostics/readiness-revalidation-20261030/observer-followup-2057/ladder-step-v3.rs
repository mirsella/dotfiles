use std::{
    collections::BTreeMap,
    env, fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    process::{Command, Stdio},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const BOOT: &str = "f8eb6e44-85ae-4e56-bc00-587876699945";
const COMPACT: &str = "f8eb6e4485ae4e56bc00587876699945";
const PKG: &str = "/nix/store/gqgwph5fdwycdl019dl2gwg6x32mvhan-predator-pm-study-0.1.0";
const SSH: &str = "/home/mirsella/dev/dotfiles/.scratch/pm-resume-20261028/remote/ssh";
fn check(ok: bool, why: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(why.into()) }
}
fn write(path: &std::path::Path, data: &[u8]) -> Result<()> {
    let mut f = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(data)?;
    f.sync_all()?;
    Ok(())
}
fn remote(dir: &std::path::Path, ip: &str, name: &str, script: &str) -> Result<Vec<u8>> {
    write(&dir.join(format!("{name}.sh")), script.as_bytes())?;
    let mut child = Command::new("timeout")
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
    child.stdin.take().unwrap().write_all(script.as_bytes())?;
    let out = child.wait_with_output()?;
    write(&dir.join(format!("{name}.out")), &out.stdout)?;
    write(&dir.join(format!("{name}.stderr")), &out.stderr)?;
    write(
        &dir.join(format!("{name}.status")),
        out.status.to_string().as_bytes(),
    )?;
    check(
        out.status.success(),
        &format!("{name} failed: {}", out.status),
    )?;
    Ok(out.stdout)
}
fn hash(path: &std::path::Path) -> Result<String> {
    let out = Command::new("sha256sum").arg(path).output()?;
    check(out.status.success(), "hash failed")?;
    Ok(String::from_utf8(out.stdout)?
        .split_whitespace()
        .next()
        .unwrap()
        .into())
}
fn fields(text: &str) -> Result<BTreeMap<String, String>> {
    text.lines()
        .map(|l| {
            l.split_once('=')
                .map(|(k, v)| (k.into(), v.into()))
                .ok_or_else(|| "bad field".into())
        })
        .collect()
}
fn cycles(text: &str) -> Result<u64> {
    let mut phase = 0;
    let mut count = 0;
    for l in text.lines() {
        check(
            !l.contains("Low-level resume complete") && !l.contains("Timekeeping suspended for"),
            "unexpected real sleep",
        )?;
        if l.contains("PM: suspend entry (") {
            check(
                phase == 0 && l.contains("PM: suspend entry (deep)"),
                "bad entry",
            )?;
            phase = 1;
        } else if l.contains("PM: suspend debug: Waiting for") {
            check(
                phase == 1 && l.contains("Waiting for 5 second(s)."),
                "bad checkpoint",
            )?;
            phase = 2;
        } else if l.contains("PM: suspend exit") {
            check(phase == 2, "bad exit")?;
            phase = 0;
            count += 1;
        }
    }
    check(phase == 0, "unfinished cycle")?;
    Ok(count)
}
fn main() -> Result<()> {
    let a: Vec<String> = env::args().collect();
    check(
        a.len() >= 4,
        "usage mode directory id [level ordinal previous kind sha due]",
    )?;
    let dir = PathBuf::from(&a[2]);
    let id = &a[3];
    check(
        id.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'),
        "bad id",
    )?;
    if a[1] == "prepare" {
        check(a.len() == 10, "prepare arguments")?;
        fs::create_dir(&dir)?;
        let level = &a[4];
        let ordinal: u64 = a[5].parse()?;
        let previous = &a[6];
        let kind = &a[7];
        let sha = &a[8];
        let due: u64 = a[9].parse()?;
        check(
            ["devices", "platform", "processors", "core"].contains(&level.as_str())
                && (2..=9).contains(&ordinal),
            "unsupported ladder step",
        )?;
        check(
            previous
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
                && ["completed", "revalidated"].contains(&kind.as_str())
                && sha.len() == 64
                && sha.bytes().all(|c| c.is_ascii_hexdigit()),
            "invalid predecessor",
        )?;
        let plan = format!(
            "schema=pm-test-2\nid={id}\nboot={BOOT}\ngeneration=/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363\nkernel=/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage\nmodules=/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules\nruntime={PKG}/bin/predator-pm-diagnostic\nbtime=1793381192\ndue={due}\nwindow_start=1793379600\nwindow_end=1793397600\nlevel={level}\nordinal={ordinal}\nprevious_id={previous}\nprevious_sha={sha}\nprevious_kind={kind}\npm_async=0\npm_trace=0\nrecorder=off\nmem_sleep=deep\ntest_delay=5\ntpm=initialized\nec=not-run\nlogged_in_users=allow-inhibitors-enforced\n"
        );
        write(&dir.join("plan.conf"), plan.as_bytes())?;
        let digest = hash(&dir.join("plan.conf"))?;
        write(&dir.join("plan.sha256"), digest.as_bytes())?;
        remote(
            &dir,
            "192.168.1.19",
            "stage",
            &format!(
                "set -euo pipefail\ndate --iso-8601=seconds\ntest \"$(cat /proc/sys/kernel/random/boot_id)\" = {BOOT}\ntest -z \"$(systemctl list-timers --all --no-legend 'predator-pm-*' | awk '$1 != \"-\" {{print}}')\"\nsystemctl list-timers --all --no-pager nextcloud-cron.timer\numask 077\nset -o noclobber\ncat > /var/lib/predator-pm-diagnostic/{id}-plan.conf <<'PLAN'\n{plan}PLAN\n{PKG}/bin/predator-pm-diagnostic check /var/lib/predator-pm-diagnostic/{id}-plan.conf {digest}\nprintf '<6>DIAG CAPTURE id={id} boot={BOOT} due={due}\\n' > /dev/kmsg\njournalctl --sync\n"
            ),
        )?;
        let name = format!("predator-pm-diagnostic-{id}");
        remote(
            &dir,
            "192.168.1.166",
            "observer-arm",
            &format!(
                "set -euo pipefail\ndate --iso-8601=seconds\nsystemctl is-active predator-netconsole-20261006.service\ntest \"$(timedatectl show -p NTPSynchronized --value)\" = yes\ntest -z \"$(systemctl list-timers --all --no-legend 'predator-pm-*' | awk '$1 != \"-\" {{print}}')\"\nlive=/home/mirsella/dev/predator-pm-debug/netconsole-20261006.log\nbase=/home/mirsella/dev/predator-pm-debug/.agents-local/{id}\ngrep -F 'DIAG CAPTURE id={id} boot={BOOT} due={due}' \"$live\"\ntest ! -e \"$base-baseline.log\"\ntest ! -e \"$base-result.log\"\ncp --no-clobber \"$live\" \"$base-baseline.log\"\nstat -c '%s' \"$base-baseline.log\"\nsha256sum \"$base-baseline.log\"\nsystemd-run --unit={name}-probe --on-calendar=@{} --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=oneshot --property=TimeoutStartSec=20s /usr/bin/timeout 15s /usr/bin/ssh-keyscan -T 5 -t ed25519 192.168.1.19\nsystemd-run --unit={name}-snapshot --on-calendar=@{} --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=oneshot --property=TimeoutStartSec=30s /usr/bin/cp --no-clobber \"$live\" \"$base-result.log\"\nsystemd-run --unit={name}-review --on-calendar=@{} --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=oneshot --property=TimeoutStartSec=10s /usr/bin/printf 'DIAG REVIEW READY {id}\\n'\nsystemctl list-timers --all --no-pager '{name}*'\n",
                due + 120,
                due + 240,
                due + 300
            ),
        )?;
        remote(
            &dir,
            "192.168.1.166",
            "baseline",
            &format!("cat /home/mirsella/dev/predator-pm-debug/.agents-local/{id}-baseline.log"),
        )?;
        let baseline_hash = hash(&dir.join("baseline.out"))?;
        check(
            fs::read_to_string(dir.join("observer-arm.out"))?.contains(&baseline_hash),
            "baseline hash mismatch",
        )?;
        remote(
            &dir,
            "192.168.1.19",
            "primary-arm",
            &format!(
                "set -euo pipefail\ndate --iso-8601=seconds\n{PKG}/bin/predator-pm-diagnostic check /var/lib/predator-pm-diagnostic/{id}-plan.conf {digest}\nsystemd-run --unit={name} --on-calendar=@{due} --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=TimeoutStartSec=30s --property=RuntimeMaxSec=180s --property=TimeoutStopSec=30s --property='ExecStopPost={PKG}/bin/predator-pm-diagnostic restore /var/lib/predator-pm-diagnostic/{id}-plan.conf {digest}' --setenv=PATH=/run/current-system/sw/bin {PKG}/bin/predator-pm-diagnostic run /var/lib/predator-pm-diagnostic/{id}-plan.conf {digest}\nsystemctl cat {name}.timer {name}.service\nsystemctl show {name}.timer -p ActiveState -p SubState -p NextElapseUSecRealtime\nsystemctl show {name}.service -p ActiveState -p MainPID -p Type -p RuntimeMaxUSec\ntest ! -e /var/lib/predator-pm-diagnostic/{id}-requested.txt\ncat /sys/power/suspend_stats/success /sys/power/suspend_stats/fail /sys/power/pm_test /sys/power/pm_async\n"
            ),
        )?;
        write(&dir.join("review-wait.sh"),format!("set -euo pipefail\ntest \"$(date +%s)\" -lt {}\ncoproc WATCH {{ exec stdbuf -oL journalctl -f -n 0 --no-pager -o cat; }}\npid=$WATCH_PID\nprintf 'follower_pid=%s\\n' \"$pid\"\ntrap 'kill \"$pid\" 2>/dev/null || true; wait \"$pid\" 2>/dev/null || true; printf \"follower_reaped=%s\\n\" \"$pid\"' EXIT\ntrap 'exit 124' TERM INT\nfound=0\nwhile IFS= read -r line <&\"${{WATCH[0]}}\"; do if [[ \"$line\" == 'DIAG REVIEW READY {id}' ]]; then printf '%s\\n' \"$line\"; found=1; break; fi; done\ntest \"$found\" = 1\ndate --iso-8601=seconds\n", due+295).as_bytes())?;
        println!(
            "ARMED {id} level={level} ordinal={ordinal} due={due} review={} manifest={digest}",
            due + 300
        );
    } else if a[1] == "collect" {
        let plan = fields(&fs::read_to_string(dir.join("plan.conf"))?)?;
        let ordinal: u64 = plan["ordinal"].parse()?;
        let due: u64 = plan["due"].parse()?;
        let read = dir.join("review");
        fs::create_dir(&read)?;
        remote(
            &read,
            "192.168.1.166",
            "observer",
            &format!(
                "set -euo pipefail\ndate --iso-8601=seconds\ncat /proc/sys/kernel/random/boot_id\nsystemctl is-active predator-netconsole-20261006.service\ntimedatectl show -p NTPSynchronized\njournalctl -u predator-pm-diagnostic-{id}-probe.service -u predator-pm-diagnostic-{id}-snapshot.service -u predator-pm-diagnostic-{id}-review.service --no-pager -o short-iso-precise\nstat -c '%s' /home/mirsella/dev/predator-pm-debug/.agents-local/{id}-result.log\nsha256sum /home/mirsella/dev/predator-pm-debug/.agents-local/{id}-result.log\nsystemctl list-timers --all --no-pager 'predator-pm-*'\n"
            ),
        )?;
        remote(
            &read,
            "192.168.1.166",
            "frozen-pi",
            &format!("cat /home/mirsella/dev/predator-pm-debug/.agents-local/{id}-result.log"),
        )?;
        check(
            fs::read_to_string(read.join("observer.out"))?
                .contains(&hash(&read.join("frozen-pi.out"))?),
            "result Pi hash mismatch",
        )?;
        remote(
            &read,
            "192.168.1.19",
            "state",
            &format!(
                "set -euo pipefail\ndate --iso-8601=seconds\ncat /proc/sys/kernel/random/boot_id /proc/uptime\ntest \"$(cat /proc/sys/kernel/random/boot_id)\" = {BOOT}\nreadlink -f /run/current-system /run/booted-system /run/booted-system/kernel /run/booted-system/kernel-modules\nsha256sum {PKG}/bin/predator-pm-diagnostic /var/lib/predator-pm-diagnostic/{id}-* /var/lib/predator-pm-study/last-completed.txt\nstat -c '%n %u %a %s' /var/lib/predator-pm-diagnostic/{id}-*\nfor f in /sys/power/suspend_stats/{{success,fail}} /sys/power/{{pm_test,pm_async,pm_trace}} /sys/module/pm_stage/parameters/{{armed,markers}}; do printf '%s=' \"$f\"; cat \"$f\"; done\nsystemctl show systemd-suspend.service predator-pm-diagnostic-{id}.service -p Id -p ActiveState -p MainPID -p Result -p ExecMainStatus\nsystemctl list-jobs --no-pager\nsystemctl --failed --no-pager\nnmcli -g GENERAL.STATE,GENERAL.CON-UUID,IP4.DNS device show enp3s0f1\ntimedatectl timesync-status\nzpool status\nfor m in /srv/data/fast /var/lib/nextcloud/data /srv/backup /srv/backup/recovery /srv/data/archive; do findmnt -n -o SOURCE,TARGET -M \"$m\"; done\nsystemctl is-active nginx postgresql redis-nextcloud phpfpm-nextcloud podman.socket podman-immich_server podman-immich_postgres podman-immich_redis podman-immich_ml hermes-agent hermes-backend systemd-timesyncd\npodman ps --format '{{{{.Names}}}} {{{{.Status}}}}'\nsystemctl list-timers --all --no-pager 'predator-pm-*' nextcloud-cron.timer night-suspend.timer\njournalctl -b {COMPACT} -u predator-pm-diagnostic-{id}.service -u systemd-suspend.service --no-pager -o short-iso-precise\n"
            ),
        )?;
        remote(
            &read,
            "192.168.1.19",
            "artifacts",
            &format!(
                "set -euo pipefail\ncd /var/lib\ntar -cf - predator-pm-diagnostic/{id}-* predator-pm-study/last-completed.txt\n"
            ),
        )?;
        let artifacts = read.join("artifacts");
        fs::create_dir(&artifacts)?;
        check(
            Command::new("tar")
                .args(["-xf"])
                .arg(read.join("artifacts.out"))
                .arg("-C")
                .arg(&artifacts)
                .status()?
                .success(),
            "artifact extraction",
        )?;
        remote(
            &read,
            "192.168.1.19",
            "kernel-export",
            &format!("journalctl -k -b {COMPACT} --no-pager -o export"),
        )?;
        let state = fs::read_to_string(read.join("state.out"))?;
        for expected in [
            format!("/sys/power/suspend_stats/success={ordinal}"),
            "/sys/power/suspend_stats/fail=0".into(),
            "/sys/power/pm_async=1".into(),
            "/sys/power/pm_test=[none]".into(),
            "/sys/module/pm_stage/parameters/armed=N".into(),
            "/sys/module/pm_stage/parameters/markers=0".into(),
            "No jobs running.".into(),
            "0 loaded units listed.".into(),
        ] {
            check(
                state.contains(&expected),
                &format!("current state mismatch: {expected}"),
            )?;
        }
        let export = fs::read_to_string(read.join("kernel-export.out"))?;
        let mut messages = String::new();
        let mut records = 0;
        for block in export.split("\n\n").filter(|b| !b.trim().is_empty()) {
            check(
                block.lines().any(|l| l == format!("_BOOT_ID={COMPACT}")),
                "wrong journal boot",
            )?;
            records += 1;
            if let Some(m) = block.lines().find_map(|l| l.strip_prefix("MESSAGE=")) {
                messages.push_str(m);
                messages.push('\n');
            }
        }
        check(cycles(&messages)? == ordinal, "boot diagnostic count")?;
        let file = |s: &str| artifacts.join(format!("predator-pm-diagnostic/{id}-{s}.txt"));
        let completed = fields(&fs::read_to_string(file("completed"))?)?;
        for (k, v) in [
            ("id", id.as_str()),
            ("boot", BOOT),
            ("runtime", plan["runtime"].as_str()),
            ("level", plan["level"].as_str()),
            ("ordinal", plan["ordinal"].as_str()),
            ("result", "DIAGNOSTIC_RETURN_VERIFIED"),
            ("real_s3", "false"),
        ] {
            check(
                completed.get(k).map(String::as_str) == Some(v),
                "completion mismatch",
            )?;
        }
        let end: u64 = completed["completed_epoch"].parse()?;
        check(end >= due && end <= due + 180, "completion timing")?;
        let observed = fs::read_to_string(file("observed-journal"))?;
        check(
            cycles(&observed)? == 1
                && observed.contains(&format!(
                    "DIAG BEGIN id={id} boot={BOOT} level={}",
                    plan["level"]
                ))
                && observed.contains("System returned from sleep operation")
                && observed.contains("Finished System Suspend."),
            "current return evidence",
        )?;
        let returned = fields(&fs::read_to_string(file("return"))?)?;
        for k in ["request_and_return", "stop"] {
            check(
                returned.get(k).map(String::as_str) == Some("Ok(())"),
                "return/reader failure",
            )?;
        }
        check(
            returned["reader_joined"] == "true" && returned["reaped"].starts_with("Ok("),
            "reader cleanup",
        )?;
        let outcome = fields(&fs::read_to_string(file("outcome"))?)?;
        check(
            outcome["operation"] == "Ok(())" && outcome["cleanup"] == "Ok(())",
            "operation cleanup",
        )?;
        let network = fs::read_to_string(file("network-readiness"))?;
        check(
            network.contains("readiness=Ok(())") && network.contains("reader_joined=true"),
            "network readiness",
        )?;
        check(
            hash(&artifacts.join("predator-pm-study/last-completed.txt"))?
                == "280573317fc03a7338ca233c367593ff2fac0163eb8cfe70fabc375f4648fd08",
            "ordinary anchor changed",
        )?;
        let completed_sha = hash(&file("completed"))?;
        write(&read.join("independent-verification.txt"),format!("boot={BOOT}\nvalidated_kernel_records={records}\ndiagnostic_cycles={ordinal}\ncurrent_cycle_verified=true\nresult=DIAGNOSTIC_RETURN_VERIFIED\ncompleted_sha={completed_sha}\nordinary_last_completed=unchanged\n").as_bytes())?;
        write(&dir.join("completed.sha256"), completed_sha.as_bytes())?;
        println!("VERIFIED {id} ordinal={ordinal} completed={end} SHA={completed_sha}");
    } else {
        return Err("unknown mode".into());
    }
    Ok(())
}
