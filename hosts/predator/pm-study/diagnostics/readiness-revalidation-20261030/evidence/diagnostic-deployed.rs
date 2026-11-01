//! Explicit, serial pm_test diagnostics. These never create real-S3 completion
//! records. One immutable manifest runs one phase; an operator reviews each
//! result before staging another phase on the same boot.
use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, BufRead, BufReader, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const STATE: &str = "/var/lib/predator-pm-diagnostic";
const LEVELS: [&str; 5] = ["freezer", "devices", "platform", "processors", "core"];
const DELAY: &str = "PM: suspend debug: Waiting for 5 second(s).";
const LAN_UUID: &str = "c4e6863b-7f76-4a4d-9a48-9937af0b5a36";

fn require(ok: bool, reason: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(reason.into().into())
    }
}
fn text(path: impl AsRef<Path>) -> Result<String> {
    Ok(fs::read_to_string(path)?.trim().to_owned())
}
fn now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
fn output(program: &str, args: &[&str]) -> Result<String> {
    let value = Command::new(program)
        .args(args)
        .env("TZ", "Europe/Paris")
        .output()?;
    require(
        value.status.success(),
        format!(
            "{program} {args:?}: {} {}",
            value.status,
            String::from_utf8_lossy(&value.stderr)
        ),
    )?;
    Ok(String::from_utf8(value.stdout)?.trim().to_owned())
}
fn marker(message: &str) -> Result<()> {
    println!("{message}");
    writeln!(
        fs::OpenOptions::new().write(true).open("/dev/kmsg")?,
        "<6>pm-diagnostic: {message}"
    )?;
    Ok(())
}
fn fields(data: &str) -> Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    for line in data.lines() {
        let (key, value) = line.split_once('=').ok_or("malformed diagnostic field")?;
        require(
            !value.is_empty() && values.insert(key.into(), value.into()).is_none(),
            "empty/duplicate diagnostic field",
        )?;
    }
    Ok(values)
}
fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}
fn digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())
}
fn hash(path: &Path) -> Result<String> {
    Ok(
        output("sha256sum", &[path.to_str().ok_or("non-UTF8 path")?])?
            .split_whitespace()
            .next()
            .ok_or("missing hash")?
            .into(),
    )
}
fn root_file(path: &Path) -> Result<String> {
    let meta = fs::symlink_metadata(path)?;
    require(
        meta.is_file() && meta.uid() == 0 && meta.mode() & 0o777 == 0o600,
        "diagnostic file must be root-owned regular mode0600",
    )?;
    text(path)
}

struct Plan {
    values: BTreeMap<String, String>,
    due: u64,
    btime: u64,
    ordinal: u64,
    end: u64,
}
impl Plan {
    fn get(&self, key: &str) -> &str {
        &self.values[key]
    }
    fn parse(data: &str) -> Result<Self> {
        let values = fields(data)?;
        let keys = [
            "schema",
            "id",
            "boot",
            "generation",
            "kernel",
            "modules",
            "runtime",
            "btime",
            "due",
            "window_start",
            "window_end",
            "level",
            "ordinal",
            "previous_id",
            "previous_sha",
            "pm_async",
            "pm_trace",
            "recorder",
            "mem_sleep",
            "test_delay",
            "tpm",
            "ec",
            "logged_in_users",
        ];
        let version2 = values.get("schema").map(String::as_str) == Some("pm-test-2");
        require(
            values.len() == keys.len() + usize::from(version2)
                && keys.iter().all(|k| values.contains_key(*k))
                && (!version2 || values.contains_key("previous_kind")),
            "unknown/missing diagnostic keys",
        )?;
        require(
            version2 || values["schema"] == "pm-test-1",
            "unsupported diagnostic schema",
        )?;
        for (key, expected) in [
            ("pm_async", "0"),
            ("pm_trace", "0"),
            ("recorder", "off"),
            ("mem_sleep", "deep"),
            ("test_delay", "5"),
            ("tpm", "initialized"),
            ("ec", "not-run"),
            ("logged_in_users", "allow-inhibitors-enforced"),
        ] {
            require(
                values[key] == expected,
                format!("unsupported diagnostic {key}"),
            )?;
        }
        require(
            id(&values["id"]) && LEVELS.contains(&values["level"].as_str()),
            "invalid diagnostic ID/level",
        )?;
        let boot = &values["boot"];
        require(
            boot.len() == 36
                && boot.bytes().enumerate().all(|(i, c)| {
                    if [8, 13, 18, 23].contains(&i) {
                        c == b'-'
                    } else {
                        c.is_ascii_hexdigit()
                    }
                }),
            "invalid boot",
        )?;
        for key in ["generation", "kernel", "modules", "runtime"] {
            require(
                values[key].starts_with("/nix/store/")
                    && values[key]
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"/._+-".contains(&c)),
                "invalid store path",
            )?;
        }
        let due: u64 = values["due"].parse()?;
        let btime: u64 = values["btime"].parse()?;
        let start: u64 = values["window_start"].parse()?;
        let end: u64 = values["window_end"].parse()?;
        require(
            btime < due
                && start <= due
                && due.checked_add(600).is_some_and(|v| v <= end)
                && end.checked_sub(start).is_some_and(|v| v <= 43200),
            "invalid diagnostic time window",
        )?;
        let ordinal = values["ordinal"].parse()?;
        if version2 {
            require(
                if ordinal == 1 {
                    values["previous_kind"] == "none"
                } else {
                    matches!(
                        values["previous_kind"].as_str(),
                        "completed" | "revalidated"
                    )
                },
                "invalid explicit predecessor kind",
            )?;
        }
        require(
            (1..=20).contains(&ordinal),
            "diagnostic ordinal out of range",
        )?;
        if ordinal == 1 {
            require(
                values["level"] == "freezer"
                    && values["previous_id"] == "none"
                    && values["previous_sha"] == "none",
                "first diagnostic must be freezer without predecessor",
            )?;
        } else {
            require(
                id(&values["previous_id"])
                    && values["previous_id"] != values["id"]
                    && digest(&values["previous_sha"]),
                "invalid predecessor",
            )?;
        }
        Ok(Self {
            values,
            due,
            btime,
            ordinal,
            end,
        })
    }
    fn load(path: &str, expected: &str) -> Result<(Self, String)> {
        require(
            digest(expected) && hash(Path::new(path))? == expected,
            "diagnostic manifest hash mismatch",
        )?;
        let raw = root_file(Path::new(path))?;
        Ok((Self::parse(&raw)?, raw))
    }
    fn save(&self, suffix: &str, contents: &str) -> Result<()> {
        fs::create_dir_all(STATE)?;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(format!("{STATE}/{}-{suffix}.txt", self.get("id")))?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        Ok(())
    }
    fn identity(&self) -> Result<()> {
        require(
            text("/proc/sys/kernel/random/boot_id")? == self.get("boot"),
            "diagnostic boot changed",
        )?;
        for (path, key) in [
            ("/run/booted-system", "generation"),
            ("/run/current-system", "generation"),
            ("/run/booted-system/kernel", "kernel"),
            ("/run/booted-system/kernel-modules", "modules"),
        ] {
            require(
                fs::canonicalize(path)? == Path::new(self.get(key)),
                format!("identity changed: {path}"),
            )?;
        }
        require(
            env::current_exe()? == Path::new(self.get("runtime")),
            "diagnostic runtime changed",
        )?;
        require(
            text("/proc/stat")?
                .lines()
                .find_map(|l| l.strip_prefix("btime "))
                .ok_or("missing btime")?
                .parse::<u64>()?
                == self.btime,
            "btime changed",
        )
    }
    fn predecessor(&self) -> Result<()> {
        if self.ordinal == 1 {
            return Ok(());
        }
        let kind = if self.get("schema") == "pm-test-2" {
            self.get("previous_kind")
        } else {
            "completed"
        };
        let path = format!("{STATE}/{}-{kind}.txt", self.get("previous_id"));
        require(
            hash(Path::new(&path))? == self.get("previous_sha"),
            "predecessor hash changed",
        )?;
        let prior = fields(&root_file(Path::new(&path))?)?;
        self.validate_predecessor_record(&prior, kind)
    }
    fn validate_predecessor_record(
        &self,
        prior: &BTreeMap<String, String>,
        kind: &str,
    ) -> Result<()> {
        for key in ["boot", "generation", "kernel", "modules", "runtime"] {
            require(
                prior.get(key).map(String::as_str) == Some(self.get(key)),
                format!("predecessor {key} mismatch"),
            )?;
        }
        let (expected_result, time_key) = if kind == "revalidated" {
            require(
                prior.get("original_completion").map(String::as_str) == Some("absent"),
                "revalidation must preserve original missing completion",
            )?;
            ("DIAGNOSTIC_RETURN_REVALIDATED", "validated_epoch")
        } else {
            ("DIAGNOSTIC_RETURN_VERIFIED", "completed_epoch")
        };
        require(
            prior.get("result").map(String::as_str) == Some(expected_result)
                && prior.get("real_s3").map(String::as_str) == Some("false")
                && prior.get("id").map(String::as_str) == Some(self.get("previous_id")),
            "predecessor was not verified",
        )?;
        require(
            prior
                .get("ordinal")
                .ok_or("missing prior ordinal")?
                .parse::<u64>()?
                .checked_add(1)
                == Some(self.ordinal),
            "predecessor ordinal mismatch",
        )?;
        require(
            prior
                .get(time_key)
                .ok_or("missing prior time")?
                .parse::<u64>()?
                < self.due,
            "predecessor chronology mismatch",
        )?;
        let previous = LEVELS
            .iter()
            .position(|v| Some(*v) == prior.get("level").map(String::as_str))
            .ok_or("invalid prior level")?;
        let current = LEVELS.iter().position(|v| *v == self.get("level")).unwrap();
        require(
            current == previous || current == previous + 1,
            "cannot skip/regress diagnostic phases",
        )
    }
    fn timing(&self, execute: bool) -> Result<()> {
        let epoch = now()?;
        let local = output("date", &["+%Y-%m-%d %H:%M"])?;
        require(local.len() == 16, "unexpected local date format")?;
        let hour: u32 = local[11..13].parse()?;
        require(
            (8..23).contains(&hour) && epoch < self.end,
            "outside observation window",
        )?;
        require(
            if execute {
                (self.due..=self.due + 30).contains(&epoch)
            } else {
                epoch.checked_add(300).is_some_and(|v| v < self.due)
            },
            "outside diagnostic submission/arming lead",
        )
    }
    fn journal(&self) -> Result<String> {
        output(
            "journalctl",
            &[
                "-k",
                "-b",
                &self.get("boot").replace('-', ""),
                "--no-pager",
                "-o",
                "short-monotonic",
            ],
        )
    }
}

// pm_test prints an entry and exit even though it never enters physical S3.
// The test-delay marker, absence of real-resume markers and exact counts are
// necessary to prevent such cycles being imported as ordinary S3 completions.
fn diagnostic_cycles(journal: &str) -> Result<u64> {
    let mut phase = 0;
    let mut count = 0;
    for line in journal.lines() {
        require(
            !line.contains("Low-level resume complete")
                && !line.contains("Timekeeping suspended for"),
            "real sleep evidence in diagnostic boot",
        )?;
        if line.contains("PM: suspend entry (") {
            require(
                phase == 0 && line.contains("PM: suspend entry (deep)"),
                "unexpected diagnostic entry",
            )?;
            phase = 1;
        } else if line.contains("PM: suspend debug: Waiting for") {
            require(
                phase == 1 && line.contains(DELAY),
                "unexpected diagnostic delay",
            )?;
            phase = 2;
        } else if line.contains("PM: suspend exit") {
            require(phase == 2, "diagnostic exit without completed test stage")?;
            count += 1;
            phase = 0;
        }
    }
    require(phase == 0, "unfinished diagnostic kernel history")?;
    Ok(count)
}
fn inactive(unit: &str) -> Result<()> {
    require(
        output("systemctl", &["show", unit, "-p", "ActiveState", "--value"])? == "inactive",
        format!("{unit} is active"),
    )
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
            "restic-backup.service",
            "sanoid.service",
            "syncoid-*.service",
            "zfs-scrub*.service",
            "zpool-trim*.service",
            "nixos-upgrade.service",
            "nextcloud-setup.service",
            "nextcloud-cron.service",
        ],
    )?;
    require(active.is_empty(), format!("active maintenance: {active}"))?;
    require(
        Command::new("pgrep")
            .args(["-G", "nixbld"])
            .status()?
            .code()
            == Some(1),
        "active builders or builder inspection failed",
    )
}
fn health(plan: &Plan, successes: u64, serial: bool) -> Result<()> {
    plan.identity()?;
    for path in [
        "/var/lib/predator-pm-study/HOLD",
        "/var/lib/predator-pm-diagnostic/HOLD",
    ] {
        require(!Path::new(path).exists(), "study is on HOLD")?;
    }
    for (path, expected) in [
        ("/sys/power/pm_async", if serial { "0" } else { "1" }),
        ("/sys/power/pm_trace", "0"),
        ("/sys/module/pm_stage/parameters/armed", "N"),
        ("/sys/module/pm_stage/parameters/markers", "0"),
        ("/sys/power/suspend_stats/fail", "0"),
        ("/sys/power/pm_debug_messages", "1"),
        ("/sys/power/pm_print_times", "1"),
        ("/sys/module/kernel/parameters/initcall_debug", "Y"),
        ("/sys/class/power_supply/ACAD/online", "1"),
        ("/sys/class/graphics/fb0/blank", "4"),
        ("/sys/module/suspend/parameters/pm_test_delay", "5"),
    ] {
        require(text(path)? == expected, format!("unexpected {path}"))?;
    }
    require(
        text("/sys/power/suspend_stats/success")?.parse::<u64>()? == successes,
        "unexpected suspend count",
    )?;
    require(
        text("/sys/power/pm_test")?.starts_with("[none]")
            && text("/sys/power/mem_sleep")?.contains("[deep]"),
        "unexpected PM mode",
    )?;
    require(
        matches!(text("/sys/class/rtc/rtc0/wakealarm")?.as_str(), "" | "0"),
        "unexpected RTC wake alarm",
    )?;
    require(
        text("/proc/acpi/button/lid/LID0/state")?.ends_with("closed"),
        "lid open",
    )?;
    require(
        output("ethtool", &["enp3s0f1"])?
            .lines()
            .any(|l| l.trim() == "Wake-on: g"),
        "WoL changed",
    )?;
    require(
        Path::new("/dev/tpm0").exists()
            && Path::new("/dev/tpmrm0").exists()
            && fs::canonicalize("/sys/bus/acpi/devices/MSFT0101:00/driver")?
                == Path::new("/sys/bus/acpi/drivers/tpm_crb"),
        "TPM baseline changed",
    )?;
    require(
        !text("/proc/cmdline")?.contains("crb_acpi_driver_init"),
        "CRB exclusion present",
    )?;
    for name in ["acer_wmi", "acer_wmi_ph31751"] {
        require(
            !Path::new(&format!("/sys/module/{name}")).exists(),
            "Acer WMI loaded",
        )?;
    }
    let ec = output(
        "journalctl",
        &[
            "-b",
            &plan.get("boot").replace('-', ""),
            "-u",
            "predator-kbd-backlight-off.service",
            "--no-pager",
            "-o",
            "cat",
        ],
    )?;
    require(
        ec.is_empty()
            || ec == "-- No entries --"
            || (ec.contains("skipped") && !ec.contains("Starting")),
        "raw EC service ran",
    )?;
    require(
        diagnostic_cycles(&plan.journal()?)? == successes,
        "kernel diagnostic count mismatch",
    )?;
    for unit in [
        "night-suspend.timer",
        "predator-pm-next.timer",
        "systemd-suspend.service",
    ] {
        inactive(unit)?;
    }
    require(
        output(
            "nmcli",
            &["-g", "GENERAL.CON-UUID", "device", "show", "enp3s0f1"],
        )? == LAN_UUID,
        "LAN profile changed",
    )?;
    require(
        output("nmcli", &["-g", "IP4.DNS", "device", "show", "enp3s0f1"])?
            .lines()
            .any(|v| v == "192.168.1.1"),
        "LAN DNS changed",
    )?;
    require(
        output("timedatectl", &["show", "-p", "NTPSynchronized", "--value"])? == "yes",
        "NTP not synchronized",
    )?;
    require(
        output("zpool", &["list", "-H", "-o", "name"])? == "fast\ntank"
            && output("zpool", &["status", "-x"])? == "all pools are healthy"
            && !output("zpool", &["status"])?.contains("in progress"),
        "pool health/maintenance changed",
    )?;
    for (mount, dataset) in [
        ("/srv/data/fast", "fast/data"),
        ("/var/lib/nextcloud/data", "fast/ncdata"),
        ("/srv/backup", "tank/backup"),
        ("/srv/backup/recovery", "tank/backup/recovery"),
        ("/srv/data/archive", "tank/archive"),
    ] {
        require(
            output("findmnt", &["-n", "-o", "SOURCE", "-M", mount])? == dataset,
            format!("wrong mount {mount}"),
        )?;
    }
    for service in [
        "nginx",
        "postgresql",
        "redis-nextcloud",
        "phpfpm-nextcloud",
        "podman.socket",
        "podman-immich_server",
        "podman-immich_postgres",
        "podman-immich_redis",
        "podman-immich_ml",
        "hermes-agent",
        "hermes-backend",
        "systemd-timesyncd",
    ] {
        require(
            output("systemctl", &["is-active", service])? == "active",
            format!("inactive {service}"),
        )?;
    }
    let containers = output("podman", &["ps", "--format", "{{.Names}} {{.Status}}"])?;
    for name in [
        "immich_server",
        "immich_postgres",
        "immich_redis",
        "immich_ml",
    ] {
        require(
            containers
                .lines()
                .any(|line| line.starts_with(&format!("{name} ")) && line.contains("(healthy)")),
            format!("unhealthy container {name}"),
        )?;
    }
    require(
        output(
            "systemctl",
            &["list-units", "--failed", "--no-legend", "--plain"],
        )?
        .is_empty(),
        "failed units present",
    )?;
    maintenance()
}

fn receive(rx: &mpsc::Receiver<io::Result<String>>, timeout: Duration) -> (String, Result<()>) {
    let until = Instant::now() + timeout;
    let mut journal = String::new();
    let outcome = (|| loop {
        let left = until
            .checked_duration_since(Instant::now())
            .ok_or("diagnostic journal deadline exceeded")?;
        let line = rx
            .recv_timeout(left)
            .map_err(|e| format!("diagnostic journal ended/timed out: {e}"))??;
        journal.push_str(&line);
        journal.push('\n');
        require(
            !line.contains("Low-level resume complete")
                && !line.contains("Timekeeping suspended for"),
            "unexpected real sleep during diagnostic",
        )?;
        let finished = journal.contains("systemd[1]: Finished System Suspend.")
            || journal.contains("systemd[1]: Finished systemd-suspend.service - System Suspend.");
        if finished && journal.contains("PM: suspend exit") {
            require(
                diagnostic_cycles(&journal)? == 1,
                "expected one diagnostic cycle",
            )?;
            require(
                journal.contains("System returned from sleep operation"),
                "missing systemd return",
            )?;
            return Ok(());
        }
    })();
    (journal, outcome)
}

fn network_ready(snapshot: &str) -> Result<bool> {
    let value = |key: &str| snapshot.lines().find_map(|l| l.strip_prefix(key));
    let state: u32 = value("GENERAL.STATE:")
        .ok_or("missing LAN state")?
        .split_whitespace()
        .next()
        .ok_or("empty LAN state")?
        .parse()?;
    require(
        matches!(
            state,
            0 | 10 | 20 | 30 | 40 | 50 | 60 | 70 | 80 | 90 | 100 | 110 | 120
        ),
        "unknown LAN state",
    )?;
    require(
        !matches!(state, 0 | 10 | 120),
        "LAN unknown/unmanaged/failed",
    )?;
    let uuid = value("GENERAL.CON-UUID:").ok_or("missing LAN UUID field")?;
    if state != 100 {
        return Ok(false);
    }
    require(
        uuid == LAN_UUID,
        format!("connected LAN profile mismatch: {uuid}"),
    )?;
    Ok(snapshot
        .lines()
        .any(|l| l.starts_with("IP4.DNS[") && l.ends_with(":192.168.1.1")))
}

// Subscribe before checking. A final snapshot closes subscription/event races;
// no periodic polling and no attempt to change the network configuration.
fn receive_network(
    rx: &mpsc::Receiver<()>,
    timeout: Duration,
    mut snapshot: impl FnMut() -> Result<String>,
) -> (String, Result<()>) {
    let until = Instant::now() + timeout;
    let mut evidence = String::new();
    let result = (|| loop {
        let current = snapshot()?;
        evidence.push_str(&format!(
            "elapsed_ms={}\n{current}\n",
            timeout
                .saturating_sub(until.saturating_duration_since(Instant::now()))
                .as_millis()
        ));
        if network_ready(&current)? {
            return Ok(());
        }
        let remaining = until.saturating_duration_since(Instant::now());
        if remaining.is_zero() || rx.recv_timeout(remaining).is_err() {
            let final_state = snapshot()?;
            evidence.push_str(&format!("final_snapshot\n{final_state}\n"));
            require(
                network_ready(&final_state)?,
                "LAN readiness deadline/monitor ended before expected profile and DNS",
            )?;
            return Ok(());
        }
    })();
    (evidence, result)
}

fn wait_network(plan: &Plan, suffix: &str) -> Result<()> {
    let mut child = Command::new("busctl").args([
        "--system", "--no-pager",
        "--match=type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged'",
        "monitor", "org.freedesktop.NetworkManager",
    ]).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
    let pipe = child.stdout.take().expect("piped network monitor");
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(pipe).lines() {
            match line {
                Ok(line) if line.contains("Member=PropertiesChanged") => {
                    if tx.send(()).is_err() {
                        break;
                    }
                }
                Ok(_) => (),
                Err(_) => break,
            }
        }
    });
    let (captured, result) = receive_network(&rx, Duration::from_secs(30), || {
        output(
            "timeout",
            &[
                "5s",
                "nmcli",
                "-t",
                "-f",
                "GENERAL.STATE,GENERAL.CON-UUID,IP4.DNS",
                "device",
                "show",
                "enp3s0f1",
            ],
        )
    });
    let stop = child.kill();
    let reaped = child.wait();
    let joined = reader.join();
    plan.save(suffix, &format!("epoch={}\nreadiness={result:?}\nstop={stop:?}\nreaped={reaped:?}\nreader_joined={}\n{captured}", now()?, joined.is_ok()))?;
    stop?;
    reaped?;
    require(joined.is_ok(), "network monitor reader panicked")?;
    result
}

fn restoration_allowed(state: &str, pid: &str, jobs: &str) -> Result<()> {
    require(
        state == "inactive" && pid == "0",
        "sleep unit not fully inactive; refusing PM restoration",
    )?;
    require(
        !jobs.contains("suspend") && !jobs.contains("sleep"),
        "sleep job pending; refusing PM restoration",
    )
}

fn safe_restore(plan: &Plan) -> Result<()> {
    plan.identity()?;
    // Never turn pm_test off while a submitted sleep operation may still reach
    // its test checkpoint: doing so could accidentally permit actual S3.
    let state = output(
        "systemctl",
        &[
            "show",
            "systemd-suspend.service",
            "-p",
            "ActiveState",
            "--value",
        ],
    )?;
    let pid = output(
        "systemctl",
        &[
            "show",
            "systemd-suspend.service",
            "-p",
            "MainPID",
            "--value",
        ],
    )?;
    let jobs = output("systemctl", &["list-jobs", "--no-legend", "--plain"])?;
    restoration_allowed(&state, &pid, &jobs)?;
    let selected = text("/sys/power/pm_test")?;
    require(
        selected.contains(&format!("[{}]", plan.get("level"))) || selected.starts_with("[none]"),
        "unrelated pm_test mode; refusing restoration",
    )?;
    require(
        matches!(text("/sys/power/pm_async")?.as_str(), "0" | "1"),
        "unexpected pm_async",
    )?;
    fs::write("/sys/power/pm_test", "none")?;
    fs::write("/sys/power/pm_async", "1")?;
    require(
        text("/sys/power/pm_test")?.starts_with("[none]") && text("/sys/power/pm_async")? == "1",
        "PM cleanup readback failed",
    )?;
    marker(&format!(
        "DIAG CLEANUP id={} pm_test=none pm_async=1",
        plan.get("id")
    ))
}

fn attempt(plan: &Plan, raw: &str) -> Result<()> {
    plan.save("requested", raw)?;
    plan.predecessor()?;
    plan.timing(true)?;
    health(plan, plan.ordinal - 1, false)?;
    require(
        output("systemctl", &["list-jobs", "--no-legend", "--plain"])?.is_empty(),
        "jobs active",
    )?;
    plan.save("before-kernel", &plan.journal()?)?;
    plan.save(
        "before",
        &format!(
            "epoch={}\nuptime={}\nsuccess={}\nfail=0\npm_async=1\npm_test=none\n{}\n{}\n",
            now()?,
            text("/proc/uptime")?,
            plan.ordinal - 1,
            output("timedatectl", &["timesync-status"])?,
            output("zpool", &["status"])?
        ),
    )?;
    let operation = (|| {
        fs::write("/sys/power/pm_async", "0")?;
        health(plan, plan.ordinal - 1, true)?;
        plan.timing(true)?;
        let cursor_text = output(
            "journalctl",
            &[
                "-b",
                &plan.get("boot").replace('-', ""),
                "-n",
                "0",
                "--show-cursor",
                "--no-pager",
            ],
        )?;
        let cursor = cursor_text
            .lines()
            .find_map(|l| l.strip_prefix("-- cursor: "))
            .ok_or("missing journal cursor")?;
        plan.save("cursor", cursor)?;
        let mut child = Command::new("journalctl")
            .args([
                "-b",
                &plan.get("boot").replace('-', ""),
                "--after-cursor",
                cursor,
                "--follow",
                "--lines=all",
                "--no-pager",
                "-o",
                "short-monotonic",
            ])
            .stdout(Stdio::piped())
            .spawn()?;
        let pipe = child.stdout.take().expect("piped journal stdout");
        let (tx, rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(pipe).lines() {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut captured = String::new();
        let result: Result<()> = (|| {
            // Check again immediately before selecting a mode and submitting.
            maintenance()?;
            plan.timing(true)?;
            fs::write("/sys/power/pm_test", plan.get("level"))?;
            require(
                text("/sys/power/pm_test")?.contains(&format!("[{}]", plan.get("level"))),
                "test mode readback mismatch",
            )?;
            marker(&format!(
                "DIAG BEGIN id={} boot={} level={} ordinal={} age_since_boot={} pm_async=0 test_delay=5 real_s3=forbidden",
                plan.get("id"),
                plan.get("boot"),
                plan.get("level"),
                plan.ordinal,
                now()?.checked_sub(plan.btime).ok_or("clock before boot")?
            ))?;
            output("journalctl", &["--sync"])?;
            plan.identity()?;
            plan.timing(true)?;
            output(
                "busctl",
                &[
                    "--timeout=30s",
                    "call",
                    "org.freedesktop.login1",
                    "/org/freedesktop/login1",
                    "org.freedesktop.login1.Manager",
                    "SuspendWithFlags",
                    "t",
                    "1",
                ],
            )?;
            let (journal, outcome) = receive(&rx, Duration::from_secs(120));
            captured = journal;
            outcome
        })();
        let stop = child.kill();
        let reaped = child.wait();
        let joined = reader.join();
        plan.save("observed-journal", &captured)?;
        plan.save("return", &format!("epoch={}\nboot={}\nrequest_and_return={result:?}\nstop={stop:?}\nreaped={reaped:?}\nreader_joined={}\nuptime={}\n", now()?, text("/proc/sys/kernel/random/boot_id")?, joined.is_ok(), text("/proc/uptime")?))?;
        stop?;
        reaped?;
        require(joined.is_ok(), "journal reader panicked")?;
        result
    })();
    let cleanup = safe_restore(plan);
    plan.save(
        "outcome",
        &format!(
            "epoch={}\noperation={operation:?}\ncleanup={cleanup:?}\n",
            now()?
        ),
    )?;
    operation?;
    cleanup?;
    plan.save("after-kernel", &plan.journal()?)?;
    wait_network(plan, "network-readiness")?;
    health(plan, plan.ordinal, false)?;
    require(
        now()? <= plan.due + 180,
        "diagnostic return outside 180-second bound",
    )?;
    let completed = format!(
        "result=DIAGNOSTIC_RETURN_VERIFIED\nid={}\nboot={}\ngeneration={}\nkernel={}\nmodules={}\nruntime={}\nlevel={}\nordinal={}\ncompleted_epoch={}\nreal_s3=false\nphysical_sleep_seconds=not-applicable\n",
        plan.get("id"),
        plan.get("boot"),
        plan.get("generation"),
        plan.get("kernel"),
        plan.get("modules"),
        plan.get("runtime"),
        plan.get("level"),
        plan.ordinal,
        now()?
    );
    plan.save("completed", &completed)?;
    marker(&format!(
        "DIAG VERIFIED id={} level={} ordinal={} real_s3=false",
        plan.get("id"),
        plan.get("level"),
        plan.ordinal
    ))
}

fn validate_retained_return(
    plan: &Plan,
    returned: &str,
    outcome: &str,
    observed: &str,
) -> Result<u64> {
    let returned = fields(returned)?;
    for (key, expected) in [
        ("boot", plan.get("boot")),
        ("request_and_return", "Ok(())"),
        ("stop", "Ok(())"),
        ("reaped", "Ok(ExitStatus(unix_wait_status(9)))"),
        ("reader_joined", "true"),
    ] {
        require(
            returned.get(key).map(String::as_str) == Some(expected),
            format!("unverified original return: {key}"),
        )?;
    }
    let epoch: u64 = returned
        .get("epoch")
        .ok_or("missing original return time")?
        .parse()?;
    require(
        (plan.due..=plan.due + 180).contains(&epoch),
        "original return outside bound",
    )?;
    let outcome = fields(outcome)?;
    for key in ["operation", "cleanup"] {
        require(
            outcome.get(key).map(String::as_str) == Some("Ok(())"),
            format!("original {key} failed"),
        )?;
    }
    require(
        diagnostic_cycles(observed)? == 1
            && observed.contains("System returned from sleep operation")
            && (observed.contains("systemd[1]: Finished System Suspend.")
                || observed
                    .contains("systemd[1]: Finished systemd-suspend.service - System Suspend.")),
        "original cycle/service return incomplete",
    )?;
    require(
        observed.contains(&format!(
            "DIAG BEGIN id={} boot={} level=freezer ordinal=1",
            plan.get("id"),
            plan.get("boot")
        )),
        "original phase marker mismatch",
    )?;
    Ok(epoch)
}

// Explicit audited transition for the known v1 first-freezer post-LAN-check
// failure. It never backdates or writes the missing original completed file.
fn revalidate(mut plan: Plan, original_sha: &str, raw: &str) -> Result<()> {
    require(
        plan.get("schema") == "pm-test-1" && plan.ordinal == 1 && plan.get("level") == "freezer",
        "revalidation supports only the reviewed v1 first-freezer return",
    )?;
    let old_runtime = plan.get("runtime").to_owned();
    let old_runtime_sha = hash(Path::new(&old_runtime))?;
    require(
        old_runtime_sha == "f1a3981959b6ea5eae30fa1e668b62cb15c7c21620529bfd3e777b59c68944d9",
        "unreviewed original runtime",
    )?;
    let file = |suffix: &str| format!("{STATE}/{}-{suffix}.txt", plan.get("id"));
    require(
        !Path::new(&file("completed")).exists() && !Path::new(&file("revalidated")).exists(),
        "original completion/checkpoint already exists",
    )?;
    require(
        now()? >= plan.get("window_start").parse()? && now()? < plan.end,
        "revalidation outside original observation window",
    )?;
    let mut evidence = String::new();
    for suffix in [
        "requested",
        "before",
        "before-kernel",
        "cursor",
        "observed-journal",
        "return",
        "outcome",
        "after-kernel",
    ] {
        root_file(Path::new(&file(suffix)))?;
        evidence.push_str(&format!(
            "original_{suffix}_sha={}\n",
            hash(Path::new(&file(suffix)))?
        ));
    }
    require(
        root_file(Path::new(&file("requested")))? == raw,
        "original request differs from manifest",
    )?;
    require(
        diagnostic_cycles(&root_file(Path::new(&file("before-kernel")))?)? == 0
            && diagnostic_cycles(&root_file(Path::new(&file("after-kernel")))?)? == 1,
        "original before/after history mismatch",
    )?;
    let returned_epoch = validate_retained_return(
        &plan,
        &root_file(Path::new(&file("return")))?,
        &root_file(Path::new(&file("outcome")))?,
        &root_file(Path::new(&file("observed-journal")))?,
    )?;
    let service = format!("predator-pm-diagnostic-{}.service", plan.get("id"));
    let journal = output(
        "journalctl",
        &[
            "-b",
            &plan.get("boot").replace('-', ""),
            "-u",
            &service,
            "--no-pager",
            "-o",
            "cat",
        ],
    )?;
    let errors: Vec<_> = journal
        .lines()
        .filter(|l| l.starts_with("Error:"))
        .collect();
    require(
        errors == ["Error: \"LAN profile changed\""],
        "original service failure is not the reviewed LAN post-check error",
    )?;
    let current = env::current_exe()?
        .to_str()
        .ok_or("non-UTF8 runtime")?
        .to_owned();
    plan.values.insert("runtime".into(), current);
    plan.identity()?;
    let label = format!("revalidation-{}", now()?);
    plan.save(&format!("{label}-original-service"), &journal)?;
    wait_network(&plan, &format!("{label}-network"))?;
    health(&plan, 1, false)?;
    require(
        output("systemctl", &["list-jobs", "--no-legend", "--plain"])?.is_empty(),
        "jobs active during revalidation",
    )?;
    let current_history = plan.journal()?;
    require(
        diagnostic_cycles(&current_history)? == 1,
        "intervening cycle before revalidation",
    )?;
    plan.save(&format!("{label}-kernel"), &current_history)?;
    plan.save(
        &format!("{label}-health"),
        &format!(
            "epoch={}\nboot={}\nchecked_cycles=1\n{}\n{}\n{}\n",
            now()?,
            plan.get("boot"),
            output(
                "nmcli",
                &[
                    "-g",
                    "GENERAL.STATE,GENERAL.CON-UUID,IP4.DNS",
                    "device",
                    "show",
                    "enp3s0f1"
                ]
            )?,
            output("timedatectl", &["timesync-status"])?,
            output("zpool", &["status"])?
        ),
    )?;
    let record = format!(
        "result=DIAGNOSTIC_RETURN_REVALIDATED\nid={}\nboot={}\ngeneration={}\nkernel={}\nmodules={}\nruntime={}\nlevel=freezer\nordinal=1\nvalidated_epoch={}\noriginal_return_epoch={returned_epoch}\noriginal_manifest_sha={original_sha}\noriginal_runtime={old_runtime}\noriginal_runtime_sha={old_runtime_sha}\noriginal_completion=absent\noriginal_failure=LAN profile changed\nreal_s3=false\nphysical_sleep_seconds=not-applicable\n{evidence}",
        plan.get("id"),
        plan.get("boot"),
        plan.get("generation"),
        plan.get("kernel"),
        plan.get("modules"),
        plan.get("runtime"),
        now()?
    );
    plan.save("revalidated", &record)?;
    marker(&format!(
        "DIAG REVALIDATED id={} original_return_epoch={returned_epoch} original_completion=absent real_s3=false",
        plan.get("id")
    ))
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    require(
        args.len() == 4,
        "usage: predator-pm-diagnostic check|run|restore|revalidate MANIFEST SHA256",
    )?;
    let (plan, raw) = Plan::load(&args[2], &args[3])?;
    match args[1].as_str() {
        "check" => {
            plan.predecessor()?;
            plan.timing(false)?;
            health(&plan, plan.ordinal - 1, false)?;
            require(
                output("systemctl", &["list-jobs", "--no-legend", "--plain"])?.is_empty(),
                "jobs active",
            )?;
            marker(&format!(
                "DIAG READY id={} boot={} level={} ordinal={} due={} planned_age={} real_s3=false",
                plan.get("id"),
                plan.get("boot"),
                plan.get("level"),
                plan.ordinal,
                plan.due,
                plan.due - plan.btime
            ))
        }
        "run" => attempt(&plan, &raw),
        "revalidate" => revalidate(plan, &args[3], &raw),
        "restore" => {
            require(
                Path::new(&format!("{STATE}/{}-requested.txt", plan.get("id"))).exists(),
                "no requested diagnostic to clean up",
            )?;
            safe_restore(&plan)
        }
        _ => Err("unknown diagnostic command".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn connected() -> String {
        format!(
            "GENERAL.STATE:100 (connected)\nGENERAL.CON-UUID:{LAN_UUID}\nIP4.DNS[1]:192.168.1.1"
        )
    }
    const RECONNECTING: &str = "GENERAL.STATE:30 (disconnected)\nGENERAL.CON-UUID:--";
    #[test]
    fn network_readiness_waits_for_connection_and_dns_but_rejects_wrong_profile() {
        assert!(!network_ready(RECONNECTING).unwrap());
        assert!(network_ready(&connected()).unwrap());
        assert!(!network_ready(&connected().replace("IP4.DNS[1]:192.168.1.1", "")).unwrap());
        assert!(network_ready(&connected().replace(LAN_UUID, "wrong-uuid")).is_err());
        assert!(network_ready("GENERAL.STATE:120 (failed)\nGENERAL.CON-UUID:--").is_err());
    }
    #[test]
    fn readiness_events_and_final_snapshot_cover_reconnection_and_missed_events() {
        let (tx, rx) = mpsc::channel();
        tx.send(()).unwrap();
        let mut samples = [RECONNECTING.to_owned(), connected()].into_iter();
        let (saved, result) =
            receive_network(&rx, Duration::from_secs(1), || Ok(samples.next().unwrap()));
        result.unwrap();
        assert!(saved.contains("disconnected") && saved.contains(LAN_UUID));
        let (_tx, rx) = mpsc::channel();
        let mut samples = [RECONNECTING.to_owned(), connected()].into_iter();
        let (saved, result) = receive_network(&rx, Duration::from_millis(2), || {
            Ok(samples.next().unwrap())
        });
        result.unwrap();
        assert!(saved.contains("final_snapshot"));
    }
    #[test]
    fn readiness_deadline_preserves_unready_state_and_never_accepts_wrong_uuid() {
        let (_tx, rx) = mpsc::channel();
        let (saved, result) =
            receive_network(&rx, Duration::from_millis(2), || Ok(RECONNECTING.into()));
        assert!(result.is_err() && saved.contains("final_snapshot"));
        let (saved, result) = receive_network(&rx, Duration::from_secs(1), || {
            Ok(connected().replace(LAN_UUID, "wrong"))
        });
        assert!(result.is_err() && saved.contains("wrong"));
    }
    #[test]
    fn revalidation_requires_genuine_original_return_cleanup_and_bounded_time() {
        let plan = Plan::parse(&manifest()).unwrap();
        let returned = format!(
            "epoch=3227\nboot={}\nrequest_and_return=Ok(())\nstop=Ok(())\nreaped=Ok(ExitStatus(unix_wait_status(9)))\nreader_joined=true",
            plan.get("boot")
        );
        let outcome = "epoch=3227\noperation=Ok(())\ncleanup=Ok(())";
        let observed = format!(
            "DIAG BEGIN id=diag-freezer boot={} level=freezer ordinal=1\n{CYCLE}System returned from sleep operation\nsystemd[1]: Finished System Suspend.\n",
            plan.get("boot")
        );
        assert_eq!(
            validate_retained_return(&plan, &returned, outcome, &observed).unwrap(),
            3227
        );
        for bad in [
            returned.replace("epoch=3227", "epoch=9999"),
            returned.replace("reader_joined=true", "reader_joined=false"),
            returned.replace("request_and_return=Ok(())", "request_and_return=Err(x)"),
        ] {
            assert!(validate_retained_return(&plan, &bad, outcome, &observed).is_err());
        }
        assert!(
            validate_retained_return(
                &plan,
                &returned,
                &outcome.replace("cleanup=Ok(())", "cleanup=Err(x)"),
                &observed
            )
            .is_err()
        );
        assert!(
            validate_retained_return(
                &plan,
                &returned,
                outcome,
                &(observed + "Timekeeping suspended for 900 seconds\n")
            )
            .is_err()
        );
    }
    #[test]
    fn explicit_revalidated_predecessor_does_not_impersonate_original_completion() {
        let next = manifest()
            .replace("schema=pm-test-1", "schema=pm-test-2")
            .replace("ordinal=1", "ordinal=2")
            .replace("level=freezer", "level=devices")
            .replace("previous_id=none", "previous_id=prior-freezer")
            .replace(
                "previous_sha=none",
                &format!("previous_sha={}", "a".repeat(64)),
            )
            + "\nprevious_kind=revalidated";
        let plan = Plan::parse(&next).unwrap();
        let mut record = BTreeMap::new();
        for key in ["boot", "generation", "kernel", "modules", "runtime"] {
            record.insert(key.into(), plan.get(key).into());
        }
        for (key, value) in [
            ("id", "prior-freezer"),
            ("level", "freezer"),
            ("ordinal", "1"),
            ("result", "DIAGNOSTIC_RETURN_REVALIDATED"),
            ("original_completion", "absent"),
            ("real_s3", "false"),
            ("validated_epoch", "3100"),
        ] {
            record.insert(key.into(), value.into());
        }
        plan.validate_predecessor_record(&record, "revalidated")
            .unwrap();
        assert!(
            plan.validate_predecessor_record(&record, "completed")
                .is_err()
        );
        record.insert("validated_epoch".into(), "9999".into());
        assert!(
            plan.validate_predecessor_record(&record, "revalidated")
                .is_err()
        );
        assert!(
            Plan::parse(&next.replace("previous_kind=revalidated", "previous_kind=anything"))
                .is_err()
        );
        assert!(Plan::parse(&(manifest() + "\nprevious_kind=revalidated")).is_err());
    }
    #[test]
    fn cleanup_cannot_enable_real_s3_while_sleep_is_active_or_queued() {
        restoration_allowed("inactive", "0", "").unwrap();
        for (state, pid, jobs) in [
            ("active", "42", ""),
            ("activating", "0", ""),
            ("inactive", "42", ""),
            ("failed", "0", ""),
            ("inactive", "0", "17 suspend.target start waiting"),
            ("inactive", "0", "17 sleep.target start running"),
        ] {
            assert!(restoration_allowed(state, pid, jobs).is_err());
        }
    }
    const CYCLE: &str =
        "PM: suspend entry (deep)\nPM: suspend debug: Waiting for 5 second(s).\nPM: suspend exit\n";
    #[test]
    fn diagnostic_history_rejects_real_sleep_and_incomplete_or_misordered_cycles() {
        assert_eq!(diagnostic_cycles("").unwrap(), 0);
        assert_eq!(diagnostic_cycles(&CYCLE.repeat(3)).unwrap(), 3);
        for bad in [
            "PM: suspend entry (deep)\nPM: suspend exit",
            "PM: suspend exit",
            "PM: suspend entry (deep)",
            "PM: suspend entry (s2idle)",
            "Timekeeping suspended for 900 seconds",
            "Low-level resume complete",
            "PM: suspend debug: Waiting for 5 second(s).",
        ] {
            assert!(diagnostic_cycles(bad).is_err(), "{bad}");
        }
        assert!(diagnostic_cycles(&CYCLE.replace("5 second", "1 second")).is_err());
    }
    #[test]
    fn waits_for_late_kernel_exit_after_service_finished() {
        let (tx, rx) = mpsc::channel();
        for line in [
            "PM: suspend entry (deep)",
            DELAY,
            "systemd-sleep: System returned from sleep operation",
            "systemd[1]: Finished System Suspend.",
            "PM: suspend exit",
        ] {
            tx.send(Ok(line.into())).unwrap();
        }
        let (saved, outcome) = receive(&rx, Duration::from_secs(1));
        outcome.unwrap();
        assert!(saved.ends_with("PM: suspend exit\n"));
    }
    #[test]
    fn timeout_preserves_partial_capture_without_accepting_kernel_exit_alone() {
        let (tx, rx) = mpsc::channel();
        for line in CYCLE.lines() {
            tx.send(Ok(line.into())).unwrap();
        }
        let (saved, outcome) = receive(&rx, Duration::from_millis(10));
        assert!(outcome.is_err());
        assert_eq!(saved, CYCLE);
    }
    #[test]
    fn channel_loss_and_unexpected_sleep_are_explicit_errors() {
        let (tx, rx) = mpsc::channel();
        tx.send(Ok("Timekeeping suspended for 900 seconds".into()))
            .unwrap();
        assert!(receive(&rx, Duration::from_secs(1)).1.is_err());
        drop(tx);
        assert!(receive(&rx, Duration::from_secs(1)).1.is_err());
    }
    fn manifest() -> String {
        "schema=pm-test-1\nid=diag-freezer\nboot=f8eb6e44-85ae-4e56-bc00-587876699945\ngeneration=/nix/store/g\nkernel=/nix/store/k/bzImage\nmodules=/nix/store/m\nruntime=/nix/store/r/bin/predator-pm-diagnostic\nbtime=1000\ndue=3220\nwindow_start=3000\nwindow_end=5000\nlevel=freezer\nordinal=1\nprevious_id=none\nprevious_sha=none\npm_async=0\npm_trace=0\nrecorder=off\nmem_sleep=deep\ntest_delay=5\ntpm=initialized\nec=not-run\nlogged_in_users=allow-inhibitors-enforced".into()
    }
    #[test]
    fn explicit_diagnostic_schema_cannot_select_real_suspend_or_skip_first_phase() {
        Plan::parse(&manifest()).unwrap();
        for (from, to) in [
            ("level=freezer", "level=none"),
            ("level=freezer", "level=devices"),
            ("test_delay=5", "test_delay=0"),
            ("pm_async=0", "pm_async=1"),
            ("recorder=off", "recorder=rtc"),
            ("tpm=initialized", "tpm=absent"),
            ("ordinal=1", "ordinal=2"),
        ] {
            assert!(Plan::parse(&manifest().replace(from, to)).is_err());
        }
        assert!(Plan::parse(&(manifest() + "\nlevel=freezer")).is_err());
        assert!(Plan::parse(&(manifest() + "\nunknown=x")).is_err());
    }
    #[test]
    fn rejects_invalid_windows_and_predecessor_paths() {
        for (from, to) in [
            ("window_end=5000", "window_end=3300"),
            ("btime=1000", "btime=3220"),
            ("window_end=5000", "window_end=999999"),
            ("id=diag-freezer", "id=../bad"),
        ] {
            assert!(Plan::parse(&manifest().replace(from, to)).is_err());
        }
        let second = manifest()
            .replace("ordinal=1", "ordinal=2")
            .replace("level=freezer", "level=devices")
            .replace("previous_id=none", "previous_id=diag-prior")
            .replace(
                "previous_sha=none",
                &format!("previous_sha={}", "a".repeat(64)),
            );
        Plan::parse(&second).unwrap();
        assert!(
            Plan::parse(&second.replace("previous_id=diag-prior", "previous_id=../bad")).is_err()
        );
    }
}
