use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Command, Stdio},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const SSH: &str = ".scratch/pm-resume-20261028/remote/ssh";
const LABEL: &str = "awake-packet-20261101-0835";
fn cmd(host: &str) -> Command {
    let mut c = Command::new("timeout");
    c.args([
        "40s",
        SSH,
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=7",
        "-p",
        "22",
        "-l",
        "mirsella",
        host,
    ]);
    c
}
fn run(d: &Path, host: &str, name: &str, script: &str) -> Result<()> {
    fs::write(d.join(format!("{name}.sh")), script)?;
    let mut c = cmd(host)
        .arg("sudo -n bash -s")
        .stdin(Stdio::piped())
        .stdout(fs::File::create(d.join(format!("{name}.out")))?)
        .stderr(fs::File::create(d.join(format!("{name}.stderr")))?)
        .spawn()?;
    c.stdin
        .take()
        .ok_or("missing stdin")?
        .write_all(script.as_bytes())?;
    let status = c.wait()?;
    fs::write(d.join(format!("{name}.status")), format!("{status}\n"))?;
    if !status.success() {
        return Err(format!("{name} failed: {status}").into());
    }
    Ok(())
}
fn main() -> Result<()> {
    let d = Path::new(".scratch/pm-wake-path-20261101-0830");
    let guard = "set -euo pipefail\ntest \"$(TZ=Europe/Paris date +%F)\" = 2026-11-01\nh=$(TZ=Europe/Paris date +%H)\ntest \"$h\" -ge 8 && test \"$h\" -lt 23\ndate --iso-8601=ns\n";
    let awake = format!(
        "{guard}test \"$(cat /proc/sys/kernel/random/boot_id)\" = f9b45aa6-b820-4ab5-b18c-9324d8d53087\nsystemctl is-system-running\nsystemctl show systemd-suspend.service -p ActiveState -p MainPID\ntest \"$(systemctl show systemd-suspend.service -p ActiveState --value)\" = inactive\ntest \"$(systemctl show systemd-suspend.service -p MainPID --value)\" = 0\ntest -z \"$(systemctl list-jobs --no-legend --plain)\"\ntest \"$(cat /sys/power/suspend_stats/success)\" = 1\ntest \"$(cat /sys/power/suspend_stats/fail)\" = 0\ntest \"$(cat /sys/class/net/enp3s0f1/carrier)\" = 1\nprintf 'awake_guard=PASS label={LABEL}\\n'\ndate --iso-8601=ns\n"
    );
    run(d, "192.168.1.19", "packet-awake-before", &awake)?;
    let mut child=cmd("192.168.1.19").arg(format!("sudo -n timeout 30s /home/mirsella/dev/dotfiles/.scratch/pm-wake-path-20261101-0830/packet-capture {LABEL}")).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(fs::File::create(d.join("packet-receiver.stderr"))?).spawn()?;
    let result = (|| -> Result<()> {
        let mut reader = BufReader::new(child.stdout.take().ok_or("missing stdout")?);
        let mut log = String::new();
        reader.read_line(&mut log)?;
        fs::write(d.join("packet-receiver.out"), &log)?;
        if !log.starts_with(&format!("READY label={LABEL} interface=enp3s0f1 ")) {
            return Err("receiver did not become ready".into());
        }
        run(d, "192.168.1.19", "packet-awake-immediate", &awake)?;
        let send = format!(
            "{guard}test \"$(cat /proc/sys/kernel/random/boot_id)\" = 2142644e-84db-40ae-b1b9-9e8c71265cf7\ntest \"$(timedatectl show -p NTPSynchronized --value)\" = yes\nprintf 'ca4666560dc9cd468f3b9b6c4c68a7dcb2675b561844ca5ef6a89fa9944722e8  /home/mirsella/wol_predator.py\\n' | sha256sum -c -\nprintf 'AWAKE_ONLY_PACKET_PROOF label={LABEL} sends=1 payload=canonical102bytes\\n'\nip route get 192.168.1.255\ndate --iso-8601=ns\n/usr/bin/python3 /home/mirsella/wol_predator.py\ndate --iso-8601=ns\n"
        );
        run(d, "192.168.1.166", "packet-sender", &send)?;
        reader.read_to_string(&mut log)?;
        fs::write(d.join("packet-receiver.out"), &log)?;
        if !log.contains(&format!("PASS label={LABEL} ")) {
            return Err("receiver did not verify packet".into());
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    fs::write(d.join("packet-receiver.status"), format!("{status}\n"))?;
    result?;
    if !status.success() {
        return Err(format!("receiver failed: {status}").into());
    }
    run(d, "192.168.1.19", "packet-awake-after", &awake)?;
    println!("bounded_awake_packet_delivery=PASS label={LABEL} sends=1");
    Ok(())
}
