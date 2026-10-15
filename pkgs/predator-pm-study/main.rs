use std::{
    env,
    error::Error,
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
mod automatic;
const STATE: &str = "/var/lib/predator-pm-study";
fn not_held() -> Result<()> {
    check(
        !Path::new(&format!("{STATE}/HOLD")).exists(),
        "suspend study is on hold; wait for explicit scheduling permission",
    )
}
const KERNEL: &str = "/nix/store/b1slgijwvdxlci37m675ai43by6nf8ak-linux-6.18.54/bzImage";
const ROOT: &str = "/dev/disk/by-id/ata-HFS128G39TND-N210A_EI76N026711106D68-part2";
const DISKS: &[&str] = &[
    "/dev/disk/by-id/wwn-0x5000c500aa3cc143-part1",
    "/dev/disk/by-id/wwn-0x500003961228993f-part1",
    "/dev/disk/by-id/wwn-0x50014ee26a2aa7c6-part1",
];

fn check(value: bool, message: impl Into<String>) -> Result<()> {
    if !value {
        return Err(message.into().into());
    }
    Ok(())
}
fn text(path: impl AsRef<Path>) -> Result<String> {
    Ok(fs::read_to_string(path)?.trim().into())
}
fn output(program: &str, args: &[&str]) -> Result<String> {
    let result = Command::new(program).args(args).output()?;
    check(
        result.status.success(),
        format!(
            "{program} {args:?}: {} {}",
            result.status,
            String::from_utf8_lossy(&result.stderr)
        ),
    )?;
    Ok(String::from_utf8(result.stdout)?.trim().into())
}
fn now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
fn marker(message: &str) -> Result<()> {
    println!("{message}");
    writeln!(
        fs::OpenOptions::new().write(true).open("/dev/kmsg")?,
        "<6>pm-debug: tpm-ec-study-20261009 {message}"
    )?;
    Ok(())
}
fn save(name: &str, data: &str) -> Result<()> {
    fs::create_dir_all(STATE)?;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(format!("{STATE}/{name}"))?;
    file.write_all(data.as_bytes())?;
    Ok(())
}
fn preserve_rtc() -> Result<()> {
    let boot = text("/proc/sys/kernel/random/boot_id")?;
    let existing = format!("{STATE}/{boot}-boot-rtc.txt");
    if Path::new(&existing).exists() {
        check(
            fs::read_to_string(&existing)?.starts_with(&format!("boot={boot}\ncmdline=")),
            "existing boot RTC evidence has unexpected contents",
        )?;
        println!("Boot RTC evidence already preserved: {existing}");
        return Ok(());
    }
    save(
        &format!("{boot}-boot-rtc.txt"),
        &format!(
            "boot={boot}\ncmdline={}\n{}\n{}\n",
            text("/proc/cmdline")?,
            output("hwclock", &["--show", "--utc"])?,
            output(
                "journalctl",
                &["-k", "-b", "0", "-o", "short-monotonic", "--no-pager"]
            )?
        ),
    )
}
fn field<'a>(data: &'a str, key: &str) -> Result<&'a str> {
    let prefix = format!("{key}=");
    let mut values = data.lines().filter_map(|line| line.strip_prefix(&prefix));
    let value = values.next().ok_or_else(|| format!("missing {key}"))?;
    check(values.next().is_none(), format!("duplicate {key}"))?;
    Ok(value)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TpmState {
    Absent,
    Initialized,
}
impl TpmState {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "absent" => Ok(Self::Absent),
            "initialized" => Ok(Self::Initialized),
            _ => Err(format!("unknown TPM arm: {value}").into()),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Initialized => "initialized",
        }
    }
    fn from_plan(plan: &str) -> Result<Self> {
        // Original combined-arm plans predate this field and require TPM exclusion.
        if plan.lines().any(|line| line.starts_with("tpm=")) {
            Self::parse(field(plan, "tpm")?)
        } else {
            Ok(Self::Absent)
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EcState {
    NotRun,
    BootWrites,
}
impl EcState {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "not-run" => Ok(Self::NotRun),
            "boot-writes" => Ok(Self::BootWrites),
            _ => Err(format!("unknown EC arm: {value}").into()),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::NotRun => "not-run",
            Self::BootWrites => "boot-writes",
        }
    }
    fn from_plan(plan: &str) -> Result<Self> {
        if plan.lines().any(|line| line.starts_with("ec=")) {
            Self::parse(field(plan, "ec")?)
        } else {
            Ok(Self::NotRun)
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoginPolicy {
    Check,
    Allow,
}
impl LoginPolicy {
    fn name(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::Allow => "allow",
        }
    }
    fn from_plan(plan: &str) -> Result<Self> {
        if !plan
            .lines()
            .any(|line| line.starts_with("logged_in_users="))
        {
            return Ok(Self::Check);
        }
        match field(plan, "logged_in_users")? {
            "check" => Ok(Self::Check),
            "allow" => Ok(Self::Allow),
            value => Err(format!("unknown logged-in-user policy: {value}").into()),
        }
    }
    fn request(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Self::Check => ("systemctl", &["--check-inhibitors=yes", "suspend"]),
            // systemd's SD_LOGIND_ROOT_CHECK_INHIBITORS (0x01) enforces both
            // block and block-weak locks. Do not set SKIP_INHIBITORS (0x10).
            Self::Allow => (
                "busctl",
                &[
                    "call",
                    "org.freedesktop.login1",
                    "/org/freedesktop/login1",
                    "org.freedesktop.login1.Manager",
                    "SuspendWithFlags",
                    "t",
                    "1",
                ],
            ),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Recorder {
    Rtc,
    Off,
}
impl Recorder {
    fn name(self) -> &'static str {
        match self {
            Self::Rtc => "rtc",
            Self::Off => "off",
        }
    }
    fn from_plan(plan: &str) -> Result<Self> {
        if !plan.lines().any(|line| line.starts_with("recorder=")) {
            return Ok(Self::Rtc);
        }
        match field(plan, "recorder")? {
            "rtc" => Ok(Self::Rtc),
            "off" => Ok(Self::Off),
            value => Err(format!("unknown recorder mode: {value}").into()),
        }
    }
    fn validate_return(
        self,
        before: u64,
        after: u64,
        action: &str,
        value: &str,
        start: &str,
    ) -> Result<()> {
        match self {
            Self::Off => check(after == before, "disabled RTC recorder wrote phase markers"),
            Self::Rtc => check(
                after > before && action == "17" && value == "3" && start == "N",
                "RTC recording did not finish console resume",
            ),
        }
    }
}
fn ec_boot_writes(journal: &str) -> Result<()> {
    // systemd's UNIT_STARTING and UNIT_STARTED records prove the existing oneshot
    // completed once during initial boot, even after a later generation masks it.
    let mut starts = Vec::new();
    let mut finishes = Vec::new();
    for record in journal.split("\n\n") {
        if !record
            .lines()
            .any(|line| line == "UNIT=predator-kbd-backlight-off.service")
        {
            continue;
        }
        let target = if record
            .lines()
            .any(|line| line == "MESSAGE_ID=7d4958e842da4a758f6c1cdc7b36dcc5")
        {
            &mut starts
        } else if record
            .lines()
            .any(|line| line == "MESSAGE_ID=39f53479d3a045ac8e11786248231fbf")
        {
            &mut finishes
        } else {
            continue;
        };
        target.push(field(record, "__MONOTONIC_TIMESTAMP")?.parse::<u64>()?);
    }
    check(
        starts.len() == 1 && finishes.len() == 1,
        "expected exactly one successful EC boot service execution",
    )?;
    check(
        starts[0] > 0 && starts[0] <= finishes[0] && finishes[0] <= 300_000_000,
        "EC service did not complete during the first five boot minutes",
    )
}
fn completed_cycles(journal: &str) -> Result<u64> {
    let mut phase = 0;
    let mut cycles = 0;
    for line in journal.lines() {
        if line.contains("PM: suspend entry (deep)") {
            check(phase == 0, "nested or unfinished deep entry")?;
            phase = 1;
        } else if line.contains("Low-level resume complete") {
            check(phase == 1, "resume without matching deep entry")?;
            phase = 2;
        } else if line.contains("PM: suspend exit") {
            check(phase == 2, "exit without low-level resume")?;
            phase = 0;
            cycles += 1;
        }
    }
    check(phase == 0, "unfinished suspend in kernel history")?;
    Ok(cycles)
}
fn journal_cursor(journal: &str) -> Result<&str> {
    let mut cursors = journal
        .lines()
        .filter_map(|line| line.strip_prefix("-- cursor: "));
    let cursor = cursors.next().ok_or("missing journal cursor")?;
    check(
        !cursor.is_empty() && cursors.next().is_none(),
        "invalid journal cursor",
    )?;
    Ok(cursor)
}
fn record_completion(boot: &str, successes: u64, markers: u64, epoch: u64) -> Result<String> {
    let record =
        format!("boot={boot}\nsuccesses={successes}\nmarkers={markers}\ncompleted_epoch={epoch}\n");
    let temporary = format!("{STATE}/last-completed-{}.tmp", std::process::id());
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(record.as_bytes())?;
    file.sync_all()?;
    fs::rename(temporary, format!("{STATE}/last-completed.txt"))?;
    Ok(record)
}
struct Baseline {
    successes: u64,
    markers: u64,
    awake_seconds: u64,
}
fn baseline(boot: &str, journal: &str) -> Result<Baseline> {
    let successes: u64 = text("/sys/power/suspend_stats/success")?.parse()?;
    let markers: u64 = text("/sys/module/pm_stage/parameters/markers")?.parse()?;
    check(
        text("/sys/power/suspend_stats/fail")? == "0",
        "earlier suspend failure this boot",
    )?;
    check(
        completed_cycles(journal)? == successes,
        "kernel history and suspend counters disagree",
    )?;
    let awake_seconds = if successes == 0 {
        check(markers == 0, "phase recording without a completed suspend")?;
        text("/proc/uptime")?
            .split_whitespace()
            .next()
            .ok_or("missing uptime")?
            .parse::<f64>()? as u64
    } else {
        let path = format!("{STATE}/last-completed.txt");
        let record = if Path::new(&path).exists() {
            text(&path)?
        } else {
            // Import only the first verified cycle produced by the original runner.
            check(
                successes == 1,
                "no verification record for prior suspend cycles",
            )?;
            let phase = text(format!("{STATE}/{boot}-after-phase.txt"))?;
            for (key, expected) in [
                ("last_action", "17"),
                ("last_value", "3"),
                ("last_start", "N"),
            ] {
                check(
                    field(&phase, key)? == expected,
                    "legacy cycle did not finish console resume",
                )?;
            }
            check(
                field(&phase, "markers")?.parse::<u64>()? == markers && markers > 0,
                "legacy phase count changed",
            )?;
            let kernel = format!("{STATE}/{boot}-after-kernel.txt");
            check(
                completed_cycles(&text(&kernel)?)? == 1,
                "legacy kernel evidence incomplete",
            )?;
            let epoch = fs::metadata(kernel)?
                .modified()?
                .duration_since(UNIX_EPOCH)?
                .as_secs();
            check(
                epoch <= now()?,
                "legacy verification timestamp is in the future",
            )?;
            marker(
                "Imported the original verified cycle; its immutable phase/kernel evidence is retained",
            )?;
            record_completion(boot, successes, markers, epoch)?
        };
        check(
            field(&record, "boot")? == boot,
            "prior verification belongs to another boot",
        )?;
        check(
            field(&record, "successes")?.parse::<u64>()? == successes,
            "unrecorded suspend cycle",
        )?;
        check(
            field(&record, "markers")?.parse::<u64>()? == markers,
            "phase count changed since verified return",
        )?;
        now()?
            .checked_sub(field(&record, "completed_epoch")?.parse()?)
            .ok_or("clock precedes last verified return")?
    };
    Ok(Baseline {
        successes,
        markers,
        awake_seconds,
    })
}
fn capture() -> Result<()> {
    check(
        text("/sys/power/pm_trace")? == "0",
        "ordinary pm_trace must remain off",
    )?;
    if Path::new("/sys/module/pm_stage").exists() {
        check(
            text("/sys/module/pm_stage/parameters/armed")? == "N",
            "phase recorder already armed",
        )?;
    } else {
        output("modprobe", &["pm_stage", "armed=0"])?;
    }
    fs::write("/sys/power/pm_debug_messages", "1")?;
    fs::write("/sys/power/pm_print_times", "1")?;
    fs::write("/sys/module/kernel/parameters/initcall_debug", "Y")?;
    output("dmesg", &["-n", "8"])?;
    if !Path::new("/sys/module/netconsole").exists() {
        output(
            "modprobe",
            &[
                "netconsole",
                "netconsole=6665@192.168.1.19/enp3s0f1,16666@192.168.1.166/dc:a6:32:0d:3e:c8",
            ],
        )?;
    }
    let journal = output("journalctl", &["-k", "-b", "0", "-o", "cat", "--no-pager"])?;
    for expected in [
        "local port 6665",
        "local IPv4 address 192.168.1.19",
        "interface name 'enp3s0f1'",
        "remote port 16666",
        "remote IPv4 address 192.168.1.166",
        "remote ethernet address dc:a6:32:0d:3e:c8",
        "network logging started",
    ] {
        check(
            journal.contains(expected),
            format!("wrong netconsole target: missing {expected}"),
        )?;
    }
    marker(&format!(
        "CAPTURE CHECK boot={} active_generation={} nonce={}",
        text("/proc/sys/kernel/random/boot_id")?,
        fs::canonicalize("/run/current-system")?.display(),
        now()?
    ))
}
fn preflight(boot: &str, tpm: TpmState, ec_state: EcState) -> Result<Baseline> {
    check(
        text("/proc/sys/kernel/random/boot_id")? == boot,
        "boot changed",
    )?;
    check(
        fs::canonicalize("/run/booted-system/kernel")? == Path::new(KERNEL),
        "kernel image changed",
    )?;
    let excluded = text("/proc/cmdline")?.split_whitespace().any(|p| {
        p.strip_prefix("initcall_blacklist=")
            .is_some_and(|names| names.split(',').any(|name| name == "crb_acpi_driver_init"))
    });
    match tpm {
        TpmState::Absent => {
            check(excluded, "CRB initialization was not excluded at boot")?;
            check(
                !Path::new("/dev/tpm0").exists() && !Path::new("/dev/tpmrm0").exists(),
                "TPM initialized",
            )?;
        }
        TpmState::Initialized => {
            check(!excluded, "TPM-on arm booted with CRB excluded")?;
            check(
                Path::new("/dev/tpm0").exists() && Path::new("/dev/tpmrm0").exists(),
                "TPM-on arm has no TPM devices",
            )?;
            check(
                fs::canonicalize("/sys/bus/acpi/devices/MSFT0101:00/driver")?
                    == Path::new("/sys/bus/acpi/drivers/tpm_crb"),
                "TPM-on arm is not bound to CRB",
            )?;
        }
    }
    for name in ["acer_wmi", "acer_wmi_ph31751"] {
        check(
            !Path::new(&format!("/sys/module/{name}")).exists(),
            format!("Acer module loaded: {name}"),
        )?;
    }
    for (path, value) in [
        ("/sys/power/pm_trace", "0"),
        ("/sys/power/pm_async", "1"),
        ("/sys/power/pm_debug_messages", "1"),
        ("/sys/power/pm_print_times", "1"),
        ("/sys/module/kernel/parameters/initcall_debug", "Y"),
        ("/sys/module/pm_stage/parameters/armed", "N"),
        ("/sys/class/power_supply/ACAD/online", "1"),
        ("/sys/class/graphics/fb0/blank", "4"),
    ] {
        check(text(path)? == value, format!("unexpected {path}"))?;
    }
    check(
        text("/sys/power/mem_sleep")?.contains("[deep]"),
        "deep not selected",
    )?;
    check(
        text("/sys/power/pm_test")?.starts_with("[none]"),
        "pm_test is active",
    )?;
    check(
        text("/proc/acpi/button/lid/LID0/state")?.ends_with("closed"),
        "lid open",
    )?;
    check(
        output("ethtool", &["enp3s0f1"])?
            .lines()
            .any(|line| line.trim() == "Wake-on: g"),
        "magic-packet wake is not enabled",
    )?;
    let journal = output("journalctl", &["-b", "0", "-o", "cat", "--no-pager"])?;
    if tpm == TpmState::Absent {
        check(
            journal.contains("blacklisting initcall crb_acpi_driver_init"),
            "missing CRB exclusion boot evidence",
        )?;
    }
    let kernel = output("journalctl", &["-k", "-b", "0", "-o", "cat", "--no-pager"])?;
    let baseline = baseline(boot, &kernel)?;
    let ec = output(
        "journalctl",
        &[
            "-b",
            "0",
            "-u",
            "predator-kbd-backlight-off.service",
            "-o",
            "cat",
            "--no-pager",
        ],
    )?;
    match ec_state {
        EcState::NotRun => check(
            ec.is_empty()
                || ec == "-- No entries --"
                || (ec.contains("skipped") && !ec.contains("Starting")),
            "direct EC service ran this boot",
        )?,
        EcState::BootWrites => ec_boot_writes(&output(
            "journalctl",
            &[
                "-b",
                "0",
                "-u",
                "predator-kbd-backlight-off.service",
                "-o",
                "export",
                "--no-pager",
            ],
        )?)?,
    }
    check(
        output("zpool", &["list", "-H", "-o", "name"])? == "fast\ntank",
        "expected fast and tank",
    )?;
    check(
        output("zpool", &["status", "-x"])? == "all pools are healthy",
        "unhealthy pools",
    )?;
    check(
        !output("zpool", &["status"])?.contains("in progress"),
        "ZFS maintenance in progress",
    )?;
    for (mount, dataset) in [
        ("/srv/data/fast", "fast/data"),
        ("/var/lib/nextcloud/data", "fast/ncdata"),
        ("/srv/backup", "tank/backup"),
        ("/srv/backup/recovery", "tank/backup/recovery"),
        ("/srv/data/archive", "tank/archive"),
    ] {
        check(
            output("findmnt", &["-n", "-o", "SOURCE", "-M", mount])? == dataset,
            format!("wrong mount: {mount}"),
        )?;
    }
    check(
        output(
            "systemctl",
            &[
                "show",
                "night-suspend.timer",
                "-p",
                "ActiveState",
                "--value",
            ],
        )? == "inactive",
        "ordinary nightly timer active",
    )?;
    check(
        output("timedatectl", &["show", "-p", "NTPSynchronized", "--value"])? == "yes",
        "clock unsynchronized",
    )?;
    marker(&format!(
        "PREFLIGHT boot={boot} uptime={} awake_since_verified_return={}s prior_successes={} prior_markers={} active_generation={} TPM={} raw-EC={} deep=1 fast=tank=healthy",
        text("/proc/uptime")?,
        baseline.awake_seconds,
        baseline.successes,
        baseline.markers,
        fs::canonicalize("/run/current-system")?.display(),
        tpm.name(),
        ec_state.name()
    ))?;
    Ok(baseline)
}
fn maintenance() -> Result<()> {
    let active = output(
        "systemctl",
        &[
            "list-units",
            "--no-pager",
            "--no-legend",
            "--plain",
            "--state=active,activating,reloading,deactivating",
            "db-backup.service",
            "recovery-backup.service",
            "hermes-backup.service",
            "sanoid.service",
            "syncoid-*.service",
            "zfs-scrub*.service",
            "zpool-trim*.service",
            "nixos-upgrade.service",
            "nextcloud-setup.service",
            "nextcloud-cron.service",
        ],
    )?;
    check(active.is_empty(), format!("active maintenance: {active}"))?;
    check(
        Command::new("pgrep")
            .args(["-G", "nixbld"])
            .status()?
            .code()
            == Some(1),
        "Nix builders active or pgrep failed",
    )?;
    Ok(())
}
fn restore_clock() -> Result<()> {
    // Capture before restarting: a fast NTP reply may precede journalctl's spawn.
    let prior = output(
        "journalctl",
        &[
            "-b",
            "0",
            "-n",
            "1",
            "--show-cursor",
            "-o",
            "cat",
            "--no-pager",
        ],
    )?;
    let cursor = journal_cursor(&prior)?;
    let mut journal = Command::new("timeout")
        .args([
            "90s",
            "journalctl",
            "-b",
            "0",
            "-u",
            "systemd-timesyncd.service",
            "--follow",
            "--lines=all",
            "--after-cursor",
            cursor,
            "-o",
            "cat",
            "--no-pager",
        ])
        .stdout(Stdio::piped())
        .spawn()?;
    output("systemctl", &["restart", "systemd-timesyncd.service"])?;
    let mut fresh = false;
    for line in BufReader::new(journal.stdout.take().ok_or("missing NTP journal")?).lines() {
        let line = line?;
        println!("NTP: {line}");
        if line.contains("Initial clock synchronization to") {
            fresh = true;
            break;
        }
    }
    let _ = journal.kill();
    journal.wait()?;
    check(fresh, "no fresh NTP synchronization after resume")?;
    check(
        output("timedatectl", &["show", "-p", "NTPSynchronized", "--value"])? == "yes",
        "NTP did not synchronize",
    )?;
    output("hwclock", &["--systohc", "--utc"])?;
    Ok(())
}
fn clock_handoff() -> Result<()> {
    check(
        output(
            "systemctl",
            &[
                "show",
                "systemd-networkd.service",
                "-p",
                "ActiveState",
                "--value",
            ],
        )? == "inactive",
        "networkd still owns runtime network state",
    )?;
    check(
        output(
            "systemctl",
            &[
                "show",
                "NetworkManager.service",
                "-p",
                "ActiveState",
                "--value",
            ],
        )? == "active"
            && output(
                "nmcli",
                &["-g", "GENERAL.STATE", "device", "show", "enp3s0f1"],
            )?
            .starts_with("100")
            && !output(
                "nmcli",
                &["-g", "IP4.GATEWAY", "device", "show", "enp3s0f1"],
            )?
            .is_empty(),
        "NetworkManager has not completed the LAN handoff",
    )?;
    check(
        !Path::new("/sys/module/pm_stage").exists()
            || text("/sys/module/pm_stage/parameters/armed")? == "N",
        "refusing clock repair while RTC recording is armed",
    )?;
    let state = Path::new("/run/systemd/netif/state");
    match fs::symlink_metadata(state) {
        Ok(meta) => {
            check(meta.is_file(), "unexpected networkd runtime state file")?;
            fs::remove_file(state)?;
            println!(
                "Removed inactive initrd networkd's stale online-state cache after verified NetworkManager handoff"
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            println!("No stale networkd online-state cache remains");
        }
        Err(error) => return Err(error.into()),
    }
    restore_clock()
}
fn sleep_length(due: u64, wake: u64) -> Result<u64> {
    let seconds = wake.checked_sub(due).ok_or("wake precedes suspend")?;
    check(
        matches!(seconds, 600 | 900 | 1800),
        "sleep must be ten, fifteen or thirty minutes",
    )?;
    Ok(seconds)
}
fn attempt(
    boot: &str,
    due: u64,
    wake: u64,
    tpm: TpmState,
    ec: EcState,
    min_awake: u64,
    recorder: Recorder,
    login_policy: LoginPolicy,
) -> Result<()> {
    let trial = format!("{boot}-{due}");
    not_held()?;
    save(
        &format!("{trial}-requested.txt"),
        &format!(
            "boot={boot}\ndue={due}\nwake={wake}\ntpm={}\nraw_ec={}\nrecorder={}\nlogged_in_users={}\nmin_awake_seconds={min_awake}\nrequested_epoch={}\ncmdline={}\nbooted_generation={}\nactive_generation={}\n",
            tpm.name(),
            ec.name(),
            recorder.name(),
            login_policy.name(),
            now()?,
            text("/proc/cmdline")?,
            fs::canonicalize("/run/booted-system")?.display(),
            fs::canonicalize("/run/current-system")?.display()
        ),
    )?;
    capture()?;
    let before = preflight(boot, tpm, ec)?;
    check(
        (due..=due + 300).contains(&now()?),
        "outside scheduled attempt window",
    )?;
    sleep_length(due, wake)?;
    check(
        min_awake >= 21600 && before.awake_seconds >= min_awake,
        format!("requires at least {min_awake}s continuously awake since boot/verified return"),
    )?;
    maintenance()?;
    for disk in DISKS {
        let warmed = Command::new("timeout")
            .args([
                "30s",
                "dd",
                &format!("if={disk}"),
                "bs=4096",
                "count=1",
                "iflag=direct",
                "status=none",
            ])
            .stdout(Stdio::null())
            .status()?;
        check(warmed.success(), format!("disk warm-up failed: {disk}"))?;
        marker(&format!("WARMED {disk}"))?;
    }
    save(
        &format!("{trial}-before.txt"),
        &format!(
            "due={due}\nwake={wake}\nprior_successes={}\nprior_markers={}\nawake_seconds={}\n{}\n{}\n{}\n",
            before.successes,
            before.markers,
            before.awake_seconds,
            output("hwclock", &["--show", "--utc"])?,
            output(
                "journalctl",
                &["-k", "-b", "0", "--no-pager", "-o", "short-monotonic"]
            )?,
            output("zpool", &["status"])?
        ),
    )?;
    output("rtcwake", &["--mode", "disable"])?;
    output("systemctl", &["stop", "systemd-timesyncd.service"])?;
    marker(&format!(
        "BEGIN boot={boot} due_epoch={due} wake_epoch={wake} TPM={} raw-EC={} wake=Pi-WoL RTC-alarm=off recorder={} logged_in_users={} inhibitors=enforced pm_trace=0 pm_async=1 manual-quiet-period-bypass=1",
        tpm.name(),
        ec.name(),
        recorder.name(),
        login_policy.name()
    ))?;
    output("journalctl", &["--sync"])?;
    if recorder == Recorder::Rtc {
        fs::write("/sys/module/pm_stage/parameters/armed", "Y")?;
    }
    let result = suspend_and_wait(login_policy);
    fs::write("/sys/module/pm_stage/parameters/armed", "N")?;
    let mut phases = String::new();
    for name in ["last_action", "last_value", "last_start", "markers"] {
        phases += &format!(
            "{name}={}\n",
            text(format!("/sys/module/pm_stage/parameters/{name}"))?
        );
    }
    save(
        &format!("{trial}-after-phase.txt"),
        &format!("{phases}{}\n", output("hwclock", &["--show", "--utc"])?),
    )?;
    restore_clock()?;
    result?;
    check(
        text("/sys/power/suspend_stats/success")?.parse::<u64>()? == before.successes + 1
            && text("/sys/power/suspend_stats/fail")? == "0",
        "no complete successful suspend",
    )?;
    let journal = output(
        "journalctl",
        &["-k", "-b", "0", "--no-pager", "-o", "short-monotonic"],
    )?;
    check(
        completed_cycles(&journal)? == before.successes + 1,
        "new cycle lacks ordered deep/resume/exit markers",
    )?;
    let markers = text("/sys/module/pm_stage/parameters/markers")?.parse::<u64>()?;
    recorder.validate_return(
        before.markers,
        markers,
        &text("/sys/module/pm_stage/parameters/last_action")?,
        &text("/sys/module/pm_stage/parameters/last_value")?,
        &text("/sys/module/pm_stage/parameters/last_start")?,
    )?;
    save(&format!("{trial}-after-kernel.txt"), &journal)?;
    check(
        (wake.saturating_sub(10)..=wake + 180).contains(&now()?),
        "return outside planned WoL window",
    )?;
    check(
        output("zpool", &["status", "-x"])? == "all pools are healthy",
        "pool unhealthy after return",
    )?;
    let record = record_completion(boot, before.successes + 1, markers, now()?)?;
    save(&format!("{trial}-completed.txt"), &record)?;
    marker(&format!(
        "VERIFIED boot={boot} recorder={} one complete deep cycle; clocks restored; pools healthy; sustained low-power observation still required",
        recorder.name()
    ))
}
fn suspend_and_wait(login_policy: LoginPolicy) -> Result<()> {
    let prior = output(
        "journalctl",
        &[
            "-b",
            "0",
            "-n",
            "1",
            "--show-cursor",
            "-o",
            "cat",
            "--no-pager",
        ],
    )?;
    let cursor = journal_cursor(&prior)?;
    let (program, args) = login_policy.request();
    marker(&format!(
        "SUSPEND REQUEST logged_in_users={} command={program} {args:?}",
        login_policy.name()
    ))?;
    output(program, args)?;
    let mut child = Command::new("timeout")
        .args([
            "2400s",
            "journalctl",
            "-b",
            "0",
            "-u",
            "systemd-suspend.service",
            "--follow",
            "--lines=all",
            "--after-cursor",
            cursor,
            "-o",
            "cat",
            "--no-pager",
        ])
        .stdout(Stdio::piped())
        .spawn()?;
    let mut returned = false;
    for line in BufReader::new(child.stdout.take().ok_or("no journal output")?).lines() {
        let line = line?;
        println!("{line}");
        if line.contains("System returned from sleep operation") {
            returned = true;
            break;
        }
        if line.contains("Failed to put system to sleep")
            || line.contains("Failed to start System Suspend")
        {
            break;
        }
    }
    let _ = child.kill();
    child.wait()?;
    check(returned, "suspend did not return successfully")
}
fn unlock_root() -> Result<()> {
    let mut secret = Vec::new();
    std::io::stdin().read_to_end(&mut secret)?;
    check(!secret.is_empty(), "empty unlock credential")?;
    let mut queries = Vec::new();
    for entry in fs::read_dir("/run/systemd/ask-password")? {
        let path = entry?.path();
        if !path
            .file_name()
            .ok_or("missing filename")?
            .to_string_lossy()
            .starts_with("ask.")
        {
            continue;
        }
        queries.push(text(&path)?);
    }
    let socket = root_socket(queries.iter().map(String::as_str))?;
    let mut child = Command::new("systemd-reply-password")
        .args(["1", &socket])
        .stdin(Stdio::piped())
        .spawn()?;
    let mut input = child.stdin.take().ok_or("missing password input")?;
    input.write_all(&secret)?;
    input.write_all(b"\n")?;
    drop(input);
    check(child.wait()?.success(), "root password handoff failed")?;
    Ok(())
}
fn root_socket<'a>(queries: impl IntoIterator<Item = &'a str>) -> Result<String> {
    let mut socket = None;
    for query in queries {
        if query
            .lines()
            .any(|line| line == format!("Id=cryptsetup:{ROOT}"))
            || query.contains("crypt-root")
        {
            check(socket.is_none(), "multiple root password queries")?;
            socket = Some(
                query
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("Socket=")
                            .filter(|value| !value.is_empty())
                            .map(String::from)
                    })
                    .ok_or("root query missing socket")?,
            );
        }
    }
    socket.ok_or_else(|| "no root unlock query".into())
}
fn planned(path: &str, execute: bool) -> Result<()> {
    let plan = text(path)?;
    let tpm = TpmState::from_plan(&plan)?;
    let ec = EcState::from_plan(&plan)?;
    let recorder = Recorder::from_plan(&plan)?;
    let login_policy = LoginPolicy::from_plan(&plan)?;
    let min_awake = if plan
        .lines()
        .any(|line| line.starts_with("min_awake_seconds="))
    {
        field(&plan, "min_awake_seconds")?.parse::<u64>()?
    } else {
        21600
    };
    check(min_awake >= 21600, "plan awake minimum is below six hours")?;
    let due = field(&plan, "due")?.parse::<u64>()?;
    let wake = field(&plan, "wake")?.parse::<u64>()?;
    sleep_length(due, wake)?;
    let boot = text("/proc/sys/kernel/random/boot_id")?;
    if plan.lines().any(|line| line.starts_with("boot=")) {
        check(
            field(&plan, "boot")? == boot,
            "boot-pinned plan belongs to another boot",
        )?;
    }
    if !execute {
        capture()?;
        preflight(&boot, tpm, ec)?;
        return marker(&format!(
            "PLAN READY boot={boot} due={due} wake={wake} recorder={} logged_in_users={}; reboot conditions and awake duration will be checked at execution",
            recorder.name(),
            login_policy.name()
        ));
    }
    let result = attempt(&boot, due, wake, tpm, ec, min_awake, recorder, login_policy);
    if let Err(error) = &result {
        save(
            &format!("{boot}-{due}-error.txt"),
            &format!(
                "boot={boot}\ndue={due}\nobserved_epoch={}\nerror={error}\n",
                now()?
            ),
        )?;
        marker(&format!("PLAN ERROR boot={boot} due={due}: {error}"))?;
    }
    result
}
fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("capture") => capture(),
        Some("preserve-rtc") => preserve_rtc(),
        Some("clock-handoff") => clock_handoff(),
        Some("check") => preflight(args.get(2).ok_or("missing boot ID")?, TpmState::Absent, EcState::NotRun).map(|_| ()),
        Some("check-arm") => preflight(
            args.get(2).ok_or("missing boot ID")?,
            TpmState::parse(args.get(3).ok_or("missing TPM arm")?)?,
            args.get(4).map(|value| EcState::parse(value)).transpose()?.unwrap_or(EcState::NotRun),
        ).map(|_| ()),
        Some("check-plan") => planned(args.get(2).ok_or("missing plan file")?, false),
        Some("attempt-plan") => planned(args.get(2).ok_or("missing plan file")?, true),
        Some("attempt") => attempt(
            args.get(2).ok_or("missing boot ID")?,
            args.get(3).ok_or("missing due time")?.parse()?,
            args.get(4).ok_or("missing wake time")?.parse()?,
            TpmState::Absent,
            EcState::NotRun, 21600, Recorder::Rtc, LoginPolicy::Check,
        ),
        Some("unlock-root") => unlock_root(),
        Some("unlock-fast") => automatic::unlock_fast(),
        Some("auto-prepare") => automatic::prepare(args.get(2).ok_or("missing previous boot ID")?),
        Some("auto-arm") => automatic::arm(args.get(2).ok_or("missing boot ID")?),
        Some("assist-next-boot") => automatic::assist(args.get(2).ok_or("missing assistant request")?),
        Some("telegram") => automatic::telegram(args.get(2).ok_or("missing notification text")?),
        _ => Err(
            "usage: predator-pm-study capture|preserve-rtc|check BOOT|check-arm BOOT absent|initialized [not-run|boot-writes]|attempt BOOT DUE WAKE|check-plan FILE|attempt-plan FILE|unlock-root|unlock-fast|auto-prepare PREVIOUS_BOOT|auto-arm BOOT|assist-next-boot REQUEST|telegram TEXT".into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logged_in_users_require_explicit_unambiguous_permission() {
        assert_eq!(
            LoginPolicy::from_plan("due=100\n").unwrap(),
            LoginPolicy::Check
        );
        assert_eq!(
            LoginPolicy::from_plan("logged_in_users=check\n").unwrap(),
            LoginPolicy::Check
        );
        assert_eq!(
            LoginPolicy::from_plan("logged_in_users=allow\n").unwrap(),
            LoginPolicy::Allow
        );
        assert!(LoginPolicy::from_plan("logged_in_users=\n").is_err());
        assert!(LoginPolicy::from_plan("logged_in_users=unknown\n").is_err());
        assert!(LoginPolicy::from_plan("logged_in_users=allow\nlogged_in_users=check\n").is_err());
    }

    #[test]
    fn allowing_logins_still_enforces_logind_inhibitors() {
        assert_eq!(
            LoginPolicy::Check.request(),
            ("systemctl", &["--check-inhibitors=yes", "suspend"][..])
        );
        assert_eq!(
            LoginPolicy::Allow.request(),
            (
                "busctl",
                &[
                    "call",
                    "org.freedesktop.login1",
                    "/org/freedesktop/login1",
                    "org.freedesktop.login1.Manager",
                    "SuspendWithFlags",
                    "t",
                    "1"
                ][..]
            )
        );
    }

    #[test]
    fn recorder_mode_is_explicit_and_legacy_plans_keep_rtc() {
        assert_eq!(Recorder::from_plan("due=100\n").unwrap(), Recorder::Rtc);
        assert_eq!(
            Recorder::from_plan("recorder=off\n").unwrap(),
            Recorder::Off
        );
        assert_eq!(
            Recorder::from_plan("recorder=rtc\n").unwrap(),
            Recorder::Rtc
        );
        assert!(Recorder::from_plan("recorder=unknown\n").is_err());
        assert!(Recorder::from_plan("recorder=off\nrecorder=rtc\n").is_err());
    }
    #[test]
    fn return_verification_distinguishes_disabled_and_completed_recording() {
        assert!(Recorder::Off.validate_return(0, 0, "0", "0", "N").is_ok());
        assert!(
            Recorder::Off
                .validate_return(0, 60, "17", "3", "N")
                .is_err()
        );
        assert!(Recorder::Rtc.validate_return(0, 60, "17", "3", "N").is_ok());
        assert!(Recorder::Rtc.validate_return(0, 0, "17", "3", "N").is_err());
        assert!(
            Recorder::Rtc
                .validate_return(0, 60, "10", "3", "Y")
                .is_err()
        );
    }

    #[test]
    fn sleep_interval_accepts_explicit_ten_fifteen_and_legacy_thirty_minutes() {
        assert_eq!(sleep_length(1000, 1600).unwrap(), 600);
        assert_eq!(sleep_length(1000, 1900).unwrap(), 900);
        assert_eq!(sleep_length(1000, 2800).unwrap(), 1800);
        for wake in [999, 1000, 1599, 1601, 1899, 1901, 2799, 2801] {
            assert!(sleep_length(1000, wake).is_err());
        }
        assert!(sleep_length(u64::MAX, 600).is_err());
    }

    #[test]
    fn ec_arm_requires_one_completed_initial_boot_execution() {
        let record = |id, time| {
            format!(
                "UNIT=predator-kbd-backlight-off.service\nMESSAGE_ID={id}\n__MONOTONIC_TIMESTAMP={time}\n\n"
            )
        };
        let start = record("7d4958e842da4a758f6c1cdc7b36dcc5", 20_000_000);
        let finish = record("39f53479d3a045ac8e11786248231fbf", 21_000_000);
        let success = format!("{start}{finish}");
        assert!(ec_boot_writes(&success).is_ok());
        assert!(ec_boot_writes(&start).is_err());
        assert!(ec_boot_writes(&format!("{success}{success}")).is_err());
        assert!(
            ec_boot_writes(&format!(
                "{start}{}",
                record("39f53479d3a045ac8e11786248231fbf", 301_000_000)
            ))
            .is_err()
        );
        assert_eq!(EcState::from_plan("due=1\n").unwrap(), EcState::NotRun);
        assert_eq!(
            EcState::from_plan("ec=boot-writes\n").unwrap(),
            EcState::BootWrites
        );
        assert!(EcState::from_plan("ec=unknown\n").is_err());
        assert!(EcState::from_plan("ec=not-run\nec=boot-writes\n").is_err());
    }

    #[test]
    fn tpm_arm_is_explicit_and_never_accepts_unknown_or_duplicate_values() {
        assert_eq!(TpmState::from_plan("due=1\n").unwrap(), TpmState::Absent);
        assert_eq!(
            TpmState::from_plan("tpm=initialized\n").unwrap(),
            TpmState::Initialized
        );
        assert!(TpmState::from_plan("tpm=unknown\n").is_err());
        assert!(TpmState::from_plan("tpm=absent\ntpm=initialized\n").is_err());
    }

    #[test]
    fn password_handoff_selects_root_and_not_other_volumes() {
        let root =
            format!("[Ask]\nId=cryptsetup:{ROOT}\nSocket=/run/systemd/ask-password/root.sock\n");
        let fast = "[Ask]\nId=cryptsetup:/dev/fast\nSocket=/run/systemd/ask-password/fast.sock\n";
        assert_eq!(
            root_socket([fast, &root]).unwrap(),
            "/run/systemd/ask-password/root.sock"
        );
        assert!(root_socket([fast]).is_err());
    }

    #[test]
    fn ambiguous_or_incomplete_root_queries_refuse_password_handoff() {
        let root = "[Ask]\nMessage=Unlock crypt-root\nSocket=/run/systemd/ask-password/root.sock\n";
        assert!(root_socket([root, root]).is_err());
        assert!(root_socket(["Message=Unlock crypt-root\n", root]).is_err());
        assert!(root_socket(["Message=Unlock crypt-root\nSocket=\n"]).is_err());
    }

    #[test]
    fn repeated_cycles_require_every_new_resume_and_exit() {
        let cycle = "PM: suspend entry (deep)\nLow-level resume complete\nPM: suspend exit\n";
        assert_eq!(completed_cycles(&cycle.repeat(2)).unwrap(), 2);
        assert!(completed_cycles(&format!("{cycle}PM: suspend entry (deep)\n")).is_err());
        assert!(completed_cycles("PM: suspend entry (deep)\nPM: suspend exit\n").is_err());
        assert!(completed_cycles("Low-level resume complete\nPM: suspend exit\n").is_err());
    }

    #[test]
    fn cursor_selection_excludes_prior_return_text_and_rejects_ambiguity() {
        assert_eq!(
            journal_cursor("System returned from sleep operation\n-- cursor: s=abc;i=42\n")
                .unwrap(),
            "s=abc;i=42"
        );
        assert!(journal_cursor("System returned from sleep operation\n").is_err());
        assert!(journal_cursor("-- cursor: a\n-- cursor: b\n").is_err());
        assert!(field("due=1\ndue=2\n", "due").is_err());
    }
}
