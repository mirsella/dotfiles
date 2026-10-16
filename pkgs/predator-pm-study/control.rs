//! Attended one-shot trial scheduling and collection. Run on the archive host.
use std::{
    env,
    error::Error,
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const PI: &str = "192.168.1.166";
const PREDATOR: &str = "192.168.1.19";
const LOG: &str = "/home/mirsella/dev/predator-pm-debug/netconsole-20261006.log";

fn check(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into().into())
    }
}
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
fn field<'a>(data: &'a str, key: &str) -> Result<&'a str> {
    let prefix = format!("{key}=");
    let mut matches = data.lines().filter_map(|line| line.strip_prefix(&prefix));
    let value = matches.next().ok_or_else(|| format!("missing {key}"))?;
    check(matches.next().is_none(), format!("duplicate {key}"))?;
    Ok(value)
}
fn epoch() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
fn write_new(path: &Path, data: &[u8]) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(data)?;
    file.sync_all()?;
    Ok(())
}
fn hash(path: &Path) -> Result<String> {
    let output = Command::new("sha256sum").arg(path).output()?;
    check(output.status.success(), "sha256sum failed")?;
    Ok(String::from_utf8(output.stdout)?
        .split_whitespace()
        .next()
        .ok_or("missing SHA256")?
        .into())
}

struct Trial {
    data: String,
    hash: String,
    id: String,
    boot: String,
    runtime: String,
    due: u64,
    wake: u64,
    out: PathBuf,
}
impl Trial {
    fn load(manifest: &Path, out: &Path) -> Result<Self> {
        let data = fs::read_to_string(manifest)?;
        let id = field(&data, "trial_id")?.to_string();
        check(
            !id.is_empty()
                && id.len() <= 64
                && id
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'),
            "invalid trial id",
        )?;
        let trial = Self {
            id,
            boot: field(&data, "boot")?.into(),
            runtime: field(&data, "runtime")?.into(),
            due: field(&data, "due")?.parse()?,
            wake: field(&data, "wake")?.parse()?,
            hash: hash(manifest)?,
            data,
            out: out.into(),
        };
        check(trial.data.ends_with('\n'), "manifest must end with newline")?;
        fs::create_dir_all(out)?;
        Ok(trial)
    }
    fn remote_plan(&self) -> String {
        format!("/var/lib/predator-pm-study/{}-plan.conf", self.id)
    }
    fn base(&self) -> String {
        format!("predator-pm-{}", self.id)
    }
    fn pi_result(&self, phase: &str) -> String {
        format!(
            "/home/mirsella/dev/predator-pm-debug/.agents-local/{}-{phase}.log",
            self.id
        )
    }

    fn remote(&self, name: &str, host: &str, script: &str) -> Result<String> {
        let dest = self.out.join(format!("{name}.txt"));
        check(
            !dest.exists() && !self.out.join(format!("{name}.meta")).exists(),
            format!("capture already exists: {name}; use a new capture directory"),
        )?;
        let mut child = Command::new("timeout")
            .args([
                "150s",
                "ssh",
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=5",
                "-o",
                "ServerAliveInterval=10",
                "-o",
                "ServerAliveCountMax=2",
                "-p",
                "22",
                "-l",
                "mirsella",
                host,
                "sudo",
                "-n",
                "bash",
                "-s",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .ok_or("missing SSH stdin")?
            .write_all(script.as_bytes())?;
        let result = child.wait_with_output()?;
        write_new(&dest, &result.stdout)?;
        write_new(
            &self.out.join(format!("{name}.meta")),
            format!(
                "host={host}\nstatus={}\nscript:\n{script}\nstderr:\n{}",
                result.status,
                String::from_utf8_lossy(&result.stderr)
            )
            .as_bytes(),
        )?;
        check(
            result.status.success(),
            format!("{name} failed: {}; preserved {dest:?}", result.status),
        )?;
        Ok(String::from_utf8(result.stdout)?)
    }

    fn check(&self, name: &str) -> Result<String> {
        self.remote(
            name,
            PREDATOR,
            &format!(
                "set -eu\n{} check-trial {} {}\n",
                quote(&self.runtime),
                quote(&self.remote_plan()),
                quote(&self.hash)
            ),
        )
    }

    fn stage(&self) -> Result<()> {
        check(
            self.data.lines().all(|line| line != "PREDATOR_TRIAL_EOF"),
            "invalid heredoc terminator",
        )?;
        self.remote("stage", PREDATOR, &format!("set -eu\numask 077\nset -o noclobber\ntest \"$(cat /proc/sys/kernel/random/boot_id)\" = {}\ncat > {} <<'PREDATOR_TRIAL_EOF'\n{}PREDATOR_TRIAL_EOF\nsha256sum {}\n", quote(&self.boot), quote(&self.remote_plan()), self.data, quote(&self.remote_plan())))?;
        self.check("stage-check")?;
        Ok(())
    }

    fn snapshot(&self, phase: &str) -> Result<()> {
        let remote = self.pi_result(phase);
        self.remote(&format!("{phase}-pi-netconsole"), PI, &format!("set -eu\numask 077\nmkdir -p /home/mirsella/dev/predator-pm-debug/.agents-local\nif ! test -e {p}; then (set -o noclobber; cat {log} > {p}); fi\ncat {p}\n", p=quote(&remote), log=quote(LOG)))?;
        let remote_hash = self.remote(
            &format!("{phase}-pi-hash"),
            PI,
            &format!("set -eu\nsha256sum {}\n", quote(&remote)),
        )?;
        check(
            remote_hash.split_whitespace().next()
                == Some(hash(&self.out.join(format!("{phase}-pi-netconsole.txt")))?.as_str()),
            "fixed Pi snapshot hash mismatch",
        )
    }

    fn timer(unit: &str, when: u64, command: &str) -> String {
        format!("systemd-run --unit={u} --on-calendar=@{when} --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=no --property=Type=oneshot {command}\ntest \"$(systemctl show {u}.timer -p SubState --value)\" = waiting\nsystemctl cat {u}.timer {u}.service\n", u=quote(unit))
    }

    fn observer(&self) -> Result<()> {
        let readiness = self.check("arm-check")?;
        let nonce = readiness
            .lines()
            .filter(|line| line.starts_with("CAPTURE CHECK "))
            .last()
            .and_then(|line| {
                line.split_whitespace()
                    .find_map(|word| word.strip_prefix("nonce="))
            })
            .ok_or("missing readiness nonce")?;
        self.remote("observer-ready", PI, &format!("set -eu\ntest \"$(timedatectl show -p NTPSynchronized --value)\" = yes\nsystemctl is-active predator-netconsole-20261006.service\ntest \"$(sha256sum /home/mirsella/wol_predator.py | cut -d' ' -f1)\" = 53a767e2eb6275439bba73272b6e65db5991e670d6c933da00b92fc2b2e048b6\ngrep -F {} {}\ncat /proc/sys/kernel/random/boot_id\ndate --iso-8601=seconds\n", quote(&format!("nonce={nonce}")), quote(LOG)))?;
        self.snapshot("baseline")?;
        let mut script = String::from("set -eu\n");
        // Check all names before creating any units, so rollback owns every name.
        for kind in ["wake", "retry", "probe", "snapshot"] {
            script += &format!(
                "test \"$(systemctl show {}.timer -p LoadState --value)\" = not-found\n",
                quote(&format!("{}-{kind}", self.base()))
            );
        }
        for (kind, when, command) in [
            ("wake", self.wake, "/usr/bin/python3 /home/mirsella/wol_predator.py".to_string()),
            ("retry", self.wake+30, "/usr/bin/python3 /home/mirsella/wol_predator.py".to_string()),
            ("probe", self.wake+90, "--property=TimeoutStartSec=20s /usr/bin/timeout 15s /usr/bin/ssh-keyscan -T 5 -t ed25519 192.168.1.19".into()),
            ("snapshot", self.wake+180, format!("/usr/bin/cp --no-clobber {} {}", quote(LOG), quote(&self.pi_result("result")))),
        ] {
            script += &Self::timer(&format!("{}-{kind}", self.base()), when, &command);
        }
        self.remote("armed-observer", PI, &script)?;
        Ok(())
    }

    fn cancel(&self, name: &str) -> Result<()> {
        // Stop only pending timers. Never kill an executing suspend service.
        check(
            epoch()? + 30 < self.due,
            "too close to/after entry for automatic cancellation; inspect live state",
        )?;
        let primary = self.remote(&format!("{name}-primary"), PREDATOR, &format!("set -eu\ntest \"$(date +%s)\" -lt {}\ntest \"$(systemctl show {u}.service -p ActiveState --value)\" = inactive\nif test \"$(systemctl show {u}.timer -p LoadState --value)\" != not-found; then systemctl stop {u}.timer; fi\ntest \"$(systemctl show {u}.timer -p ActiveState --value)\" = inactive\nsystemctl show {u}.timer -p ActiveState -p SubState\n", self.due-30, u=quote(&self.base())));
        // Preserve wake coverage unless primary cancellation was confirmed.
        primary?;
        let mut script = String::from("set -eu\n");
        for kind in ["wake", "retry", "probe", "snapshot"] {
            script += &format!("if test \"$(systemctl show {u}.timer -p LoadState --value)\" != not-found; then systemctl stop {u}.timer; fi\n", u=quote(&format!("{}-{kind}", self.base())));
        }
        let observer = self.remote(&format!("{name}-observer"), PI, &script);
        observer?;
        Ok(())
    }

    fn arm(&self) -> Result<()> {
        check(
            epoch()? + 300 < self.due,
            "need at least five minutes before primary due",
        )?;
        // Refuse existing owned units before modifying any of them.
        self.remote(
            "unused-primary",
            PREDATOR,
            &format!(
                "set -eu\ntest \"$(systemctl show {}.timer -p LoadState --value)\" = not-found\n",
                quote(&self.base())
            ),
        )?;
        // Refuse previously armed observer names before enabling rollback.
        let mut unused = String::from("set -eu\n");
        for kind in ["wake", "retry", "probe", "snapshot"] {
            unused += &format!(
                "test \"$(systemctl show {}.timer -p LoadState --value)\" = not-found\n",
                quote(&format!("{}-{kind}", self.base()))
            );
        }
        self.remote("unused-observer", PI, &unused)?;
        let result: Result<()> = (|| {
            self.observer()?;
            let command = format!("--property=TimeoutStartSec=12min --property=Environment=PATH=/run/current-system/sw/bin {} {} attempt-trial {} {}",
                quote(&format!("--property=ExecStopPost={} restore-trial {} {}", self.runtime, self.remote_plan(), self.hash)), quote(&self.runtime), quote(&self.remote_plan()), quote(&self.hash));
            let script = format!(
                "set -eu\ntest \"$(date +%s)\" -lt {}\n{} check-trial {} {}\n{}",
                self.due - 60,
                quote(&self.runtime),
                quote(&self.remote_plan()),
                quote(&self.hash),
                Self::timer(&self.base(), self.due, &command)
            );
            self.remote("armed-primary", PREDATOR, &script)?;
            Ok(())
        })();
        if let Err(error) = result {
            let cancellation = self.cancel("arm-rollback");
            return Err(format!("arming failed: {error}; cancellation={cancellation:?}").into());
        }
        println!(
            "ARMED trial={} boot={} due={} wake={} retry={} probe={} snapshot={} manifest={}",
            self.id,
            self.boot,
            self.due,
            self.wake,
            self.wake + 30,
            self.wake + 90,
            self.wake + 180,
            self.hash
        );
        Ok(())
    }

    fn collect(&self) -> Result<()> {
        check(
            epoch()? >= self.wake + 180,
            "result collection is before observation deadline",
        )?;
        let mut issues = Vec::new();
        let mut observer =
            String::from("set -eu\ndate --iso-8601=seconds\ncat /proc/sys/kernel/random/boot_id\n");
        for kind in ["wake", "retry", "probe", "snapshot"] {
            observer += &format!("systemctl show {u}.service -p Result -p ExecMainStatus -p ExecMainStartTimestamp -p ExecMainExitTimestamp\njournalctl -u {u}.service --no-pager -o short-iso-precise\n", u=quote(&format!("{}-{kind}", self.base())));
        }
        if let Err(error) = self.remote("result-observer", PI, &observer) {
            issues.push(error.to_string());
        }
        if let Err(error) = self.snapshot("result") {
            issues.push(error.to_string());
        }
        let script = format!("set -eu\ndate --iso-8601=seconds\ncat /proc/sys/kernel/random/boot_id /proc/uptime\nreadlink -f /run/booted-system /run/current-system\nfor file in /var/lib/predator-pm-study/{}-*.txt; do if test -f \"$file\"; then printf '\\nARTIFACT=%s\\n' \"$file\"; cat \"$file\"; fi; done\njournalctl --boot={} -u {}.service -u systemd-suspend.service --no-pager -o short-iso-precise\njournalctl --boot={} -k --no-pager -o short-monotonic\ntimedatectl status\ntimedatectl timesync-status\nzpool status\nfindmnt -rn -t zfs -o SOURCE,TARGET\ncat /sys/power/pm_async /sys/power/suspend_stats/success /sys/power/suspend_stats/fail\n", self.id, quote(&self.boot), quote(&self.base()), quote(&self.boot));
        if let Err(error) = self.remote("result-predator", PREDATOR, &script) {
            issues.push(error.to_string());
        }
        write_new(&self.out.join("collection.txt"), format!("trial={}\nepoch={}\nissues={issues:?}\nclassification=requires_power_graph_and_boot_bound_review\n", self.id, epoch()?).as_bytes())?;
        let mut hashes = String::new();
        let mut paths: Vec<_> = fs::read_dir(&self.out)?
            .map(|entry| entry.map(|e| e.path()))
            .collect::<std::io::Result<_>>()?;
        paths.sort();
        for path in paths {
            if path.is_file() {
                hashes += &format!(
                    "{}  {}\n",
                    hash(&path)?,
                    path.file_name()
                        .ok_or("missing filename")?
                        .to_string_lossy()
                );
            }
        }
        write_new(&self.out.join("SHA256SUMS"), hashes.as_bytes())?;
        check(
            issues.is_empty(),
            format!("partial collection; retained all available evidence: {issues:?}"),
        )
    }
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args().collect();
    check(
        args.len() == 4,
        "usage: predator-pm-control stage|check|arm|cancel|collect MANIFEST CAPTURE_DIRECTORY",
    )?;
    let trial = Trial::load(Path::new(&args[2]), Path::new(&args[3]))?;
    match args[1].as_str() {
        "stage" => trial.stage(),
        "check" => trial.check("check").map(|value| println!("{value}")),
        "arm" => trial.arm(),
        "cancel" => trial.cancel("cancel"),
        "collect" => trial.collect(),
        _ => Err("unknown control action".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quoting_preserves_literal_shell_metacharacters() {
        let value = "a'b\n$(exit 9)";
        let output = Command::new("sh")
            .args(["-c", &format!("printf %s {}", quote(value))])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), value);
    }
}
