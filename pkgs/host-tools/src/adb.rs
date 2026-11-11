//! Retry missed ADB mDNS connections without retaining an obsolete IP or port,
//! drop a transport once its phone stops advertising it, and turn Wireless
//! debugging off once no workstation sharing a phone has used it for a while.

use crate::util;
use anyhow::{Context, Result, anyhow, bail, ensure};
use std::{
    collections::HashSet,
    env, fs,
    io::{self, Read, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const SERVICE_TYPE: &str = "_adb-tls-connect._tcp";
const RETRY_INTERVAL: Duration = Duration::from_secs(2);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// While connected, re-check mDNS this often so a transport the phone no longer
/// advertises is dropped even when ADB reports no device change.
const MDNS_POLL_MS: i32 = 5_000;
/// A missing mDNS advertisement alone is not proof of a dead transport: cache
/// expiry and Wi-Fi roams look the same, so wait this long before dropping one.
const ABSENT_GRACE: Duration = Duration::from_secs(15);
/// How often the device socket's payload counters are read. They are local,
/// so sampling costs the phone nothing, and frequent samples publish a new use
/// before another workstation's deadline can cut it.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(60);
/// While a phone stays in use, its marker is refreshed at most this often, so
/// every workstation treats a marker as current for this long past the idle limit.
const MARK_INTERVAL: Duration = Duration::from_secs(30 * 60);
/// How long a failed phone command waits before the phone is asked again.
const PHONE_RETRY: Duration = Duration::from_secs(15 * 60);
/// Lets the device socket's counters settle after our own phone command, so
/// that command is not mistaken for use at the next sample.
const SETTLE: Duration = Duration::from_secs(1);
/// Workstations sharing a phone record their latest ADB use here, one file per
/// hostname, timestamped by the phone's own clock.
const MARKER_DIR: &str = "/data/local/tmp/adb-wireless";
/// Android build tools (Gradle's Android plugin, Android Studio, Flutter) read
/// the property list of every phone the server lists, including phones they do
/// not target, so such a read is not use of this phone. One is a `getprop`
/// request of 174 bytes, more when the reply takes several acknowledgements.
const PROBE_REQUEST_MAX: u64 = 512;
/// A probe's reply is the property list plus about 300 bytes of ADB and TLS
/// framing, and the list itself changes by a few hundred bytes over a day: allow
/// this fraction of the list, and never less than `PROBE_REPLY_SLACK`.
const PROBE_REPLY_FRACTION: u64 = 50;
const PROBE_REPLY_SLACK: u64 = 512;
/// Builds can overlap, but more probes than this in one sample is not a pattern
/// worth trusting: the tolerance widens with every probe.
const PROBE_BATCH_MAX: u64 = 4;
/// A shorter list could not be told apart from an ordinary command's reply.
const PROPERTY_LIST_MIN: u64 = 16 * 1024;
const SERVER: AdbServer = AdbServer(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 5037)));

pub fn run(adb: &str, serials: &[String], once: bool, idle: Option<Duration>) -> Result<()> {
    ensure!(
        !serials.is_empty(),
        "expected at least one Android serial number"
    );
    let mut unique = HashSet::new();
    for serial in serials {
        ensure!(
            plain_name(serial),
            "expected an Android serial number, not an IP address or mDNS service name: {serial}"
        );
        ensure!(
            unique.insert(serial.as_str()),
            "duplicate Android serial number: {serial}"
        );
    }
    // Spawn the SDK client only to start/check the server, never for routine queries.
    // `-H`/`-P` make adb treat the server as remote: it then refuses to spawn one
    // ("cannot start server on remote host") and can only connect when one already
    // runs. Start the default local socket instead, and let adb wait for the daemon.
    util::command_output(
        client(adb)
            .arg("start-server")
            .env("ADB_MDNS_AUTO_CONNECT", "adb-tls-connect"),
    )
    .context("start the local ADB server")?;
    // The hostname names this workstation's ADB-use marker in a phone shell command.
    let idle = match idle {
        Some(idle) => {
            let host = util::read("/proc/sys/kernel/hostname")?.trim().to_owned();
            ensure!(
                plain_name(&host),
                "expected a plain hostname to name this workstation's ADB-use marker: {host:?}"
            );
            Some((idle, host))
        }
        None => None,
    };
    // The address of a transport comes only from mDNS, so each phone's is noted
    // where it outlives a watcher restart but not the ADB server's session.
    let memory = PathBuf::from(
        env::var_os("XDG_RUNTIME_DIR")
            .context("XDG_RUNTIME_DIR is not set, so held ADB transports cannot be remembered")?,
    )
    .join("adb-watch");
    fs::create_dir_all(&memory).with_context(|| format!("create {}", memory.display()))?;
    // Subscribe before querying, so a disconnect during reconciliation is not missed.
    // A broken subscription exits; systemd owns restart and backoff.
    let mut tracker = SERVER.request("host:track-devices")?;
    let mut devices = read_frame(&mut tracker)?;
    let mut phones = serials
        .iter()
        .map(|serial| Phone::new(serial).recalling(memory.join(serial)))
        .collect::<Vec<_>>();
    loop {
        let mut all_connected = true;
        let mut failure = None;
        for phone in &mut phones {
            match phone.sync(&devices, !once) {
                Ok(connected) => all_connected &= connected,
                Err(error) => {
                    all_connected = false;
                    failure.get_or_insert(error);
                }
            }
        }
        if once {
            let Some(error) = failure else {
                for phone in &phones {
                    println!("{}", phone.last_message);
                }
                return Ok(());
            };
            return Err(error);
        }
        if let Some((idle, host)) = &idle {
            for phone in &mut phones {
                phone.enforce_idle(adb, host, *idle);
            }
        }
        if !all_connected {
            // Rate-limit failures even when our own attempts generate device events.
            thread::sleep(RETRY_INTERVAL);
        }
        update_devices(
            &mut tracker,
            &mut devices,
            if all_connected { MDNS_POLL_MS } else { 0 },
        )?;
    }
}

/// One watched phone: the transport it holds and the state that decides idleness.
struct Phone {
    serial: String,
    /// A failed query says nothing about it, so only a reconciliation replaces it.
    connection: Option<Connection>,
    /// The transport last held, also noted in `memory` for the next watcher: a
    /// phone that advertises nothing can only be watched at a known address.
    remembered: Option<Connection>,
    memory: Option<PathBuf>,
    last_message: String,
    /// Set while the phone advertises nothing; see `ABSENT_GRACE`.
    missing_since: Option<Instant>,
    /// Boot-time deadline, including suspend. None recovers the phone's durable
    /// history after a watcher restart; changing transports never restarts it.
    idle_deadline: Option<Duration>,
    next_sample: Duration,
    /// Counters and boot time for the last sample of this device's socket.
    baseline: Option<SocketSample>,
    unreadable: bool,
    /// Latest detection waiting for the rate-limited phone marker write.
    pending_activity: Option<serde_json::Value>,
    /// Earliest `boot_time` this workstation may write its marker again.
    next_mark: Duration,
    /// Neither marker reads nor a shutdown reach the phone before this.
    shutdown_after: Duration,
    /// Size of the phone's property list, reported by each history command.
    /// None counts every payload change as use; see `PROBE_REQUEST_MAX`.
    property_bytes: Option<u64>,
}

impl Phone {
    fn new(serial: &str) -> Self {
        Self {
            serial: serial.to_owned(),
            connection: None,
            remembered: None,
            memory: None,
            last_message: String::new(),
            missing_since: None,
            idle_deadline: None,
            next_sample: Duration::ZERO,
            baseline: None,
            unreadable: false,
            pending_activity: None,
            next_mark: Duration::ZERO,
            shutdown_after: Duration::ZERO,
            property_bytes: None,
        }
    }

    /// Start from the transport the previous watcher noted in `memory`, and note
    /// there each one this watcher holds.
    fn recalling(mut self, memory: PathBuf) -> Self {
        self.remembered = recall(&memory);
        self.memory = Some(memory);
        self
    }

    fn remember(&mut self, connection: &Connection) {
        self.remembered = Some(connection.clone());
        let Some(memory) = &self.memory else {
            return;
        };
        let note = format!("{} {}\n", connection.serial, connection.address);
        if let Err(error) = fs::write(memory, note) {
            eprintln!(
                "adb-watch: note the transport of {} in {}: {error}; a restarted watcher cannot adopt it while unadvertised",
                self.serial,
                memory.display()
            );
        }
    }

    /// Bring the held transport in line with what this phone advertises, logging
    /// changes when `announce` is set, and report whether it is online.
    fn sync(&mut self, devices: &str, announce: bool) -> Result<bool> {
        let result = reconcile(
            &self.serial,
            devices,
            self.connection.as_ref(),
            self.remembered.as_ref(),
            &mut self.missing_since,
            |service| SERVER.query(service),
            answering,
        );
        let message = match &result {
            // Reconciliation keeps an unadvertised transport past the grace
            // period only while the phone still answers on it.
            Ok(Some(connection))
                if self
                    .missing_since
                    .is_some_and(|since| since.elapsed() >= ABSENT_GRACE) =>
            {
                format!(
                    "{} no longer advertises Wireless debugging, but still answers at {}; keeping its connection",
                    self.serial, connection.address
                )
            }
            Ok(Some(connection)) => format!("{} connected at {}", self.serial, connection.address),
            // Only mDNS gives a transport's address, so one no watcher held when
            // the advertisement stopped cannot be sampled.
            Ok(None) if online_unadvertised(devices, &self.serial) => format!(
                "{} is connected but not advertised, so its address is unknown: its ADB use is not watched until it advertises again",
                self.serial
            ),
            Ok(None) => format!(
                "waiting for {} to advertise Wireless debugging on the LAN",
                self.serial
            ),
            Err(error) => format!(
                "{}: {error:#}; retrying in {}s",
                self.serial,
                RETRY_INTERVAL.as_secs()
            ),
        };
        if message != self.last_message {
            if announce {
                eprintln!("adb-watch: {message}");
            }
            self.last_message = message;
        }
        let reached = result?;
        let connected = reached.is_some();
        // A failed query says nothing about the transport, so only a completed
        // reconciliation replaces it.
        if reached != self.connection {
            self.connection = reached;
            if let Some(connection) = self.connection.clone() {
                self.remember(&connection);
            }
            let baseline = self
                .connection
                .as_ref()
                .and_then(|connection| transport_bytes(&connection.address));
            self.reset_transport(boot_time(), baseline);
        }
        Ok(connected)
    }

    /// Exclude connection handshakes without forgetting the last actual use.
    fn reset_transport(&mut self, now: Duration, baseline: Option<SocketBytes>) {
        self.next_sample = now + SAMPLE_INTERVAL;
        self.unreadable = false;
        self.baseline = baseline.map(|bytes| SocketSample { bytes, at: now });
    }

    fn observe(&mut self, bytes: SocketBytes, peer: &str, now: Duration, idle: Duration) {
        let previous = self.baseline.replace(SocketSample { bytes, at: now });
        if previous.is_some_and(|sample| {
            sample.bytes.sent == bytes.sent && sample.bytes.received == bytes.received
        }) {
            return;
        }
        let reason = match previous {
            None => "unknown_baseline",
            Some(sample)
                if bytes.sent < sample.bytes.sent || bytes.received < sample.bytes.received =>
            {
                "counters_decreased"
            }
            Some(_) => "payload_changed",
        };
        if let (Some(sample), Some(properties)) = (previous, self.property_bytes)
            && reason == "payload_changed"
        {
            let sent = bytes.sent - sample.bytes.sent;
            let received = bytes.received - sample.bytes.received;
            if let Some(probes) = property_probes(sent, received, properties) {
                eprintln!(
                    "adb-watch: not counting {probes} build-tool property read(s) of {} as use: sent {sent} bytes, received {received}, property list {properties}",
                    self.serial
                );
                return;
            }
        }
        let activity = serde_json::json!({
            "reason": reason,
            "serial": self.serial,
            "peer": peer,
            "detected_at_host_unix": SystemTime::now().duration_since(UNIX_EPOCH)
                .expect("workstation clock predates the Unix epoch").as_secs(),
            "sample_elapsed_ms": previous.map(|sample| (now - sample.at).as_millis()),
            "previous": previous.map(|sample| sample.bytes),
            "current": bytes,
            "delta": previous.map(|sample| serde_json::json!({
                "sent": bytes.sent.checked_sub(sample.bytes.sent),
                "received": bytes.received.checked_sub(sample.bytes.received),
                "retransmitted": bytes.retransmitted.checked_sub(sample.bytes.retransmitted),
            })),
            // TCP counters cannot identify the ADB service or recover a command.
            "command": null,
        });
        eprintln!("adb-watch: detected ADB activity: {activity}");
        self.idle_deadline = Some(now + idle);
        self.pending_activity = Some(activity);
    }

    /// Sample this workstation's use, record it on the phone for the other
    /// workstations, and turn Wireless debugging off once none has used it for `idle`.
    fn enforce_idle(&mut self, adb: &str, host: &str, idle: Duration) {
        let now = boot_time();
        if now < self.next_sample {
            return;
        }
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.next_sample = now + SAMPLE_INTERVAL;
        // Only payload counts: keepalive probes are empty segments, so the
        // counters of an idle phone stay put.
        let Some(mut bytes) = transport_bytes(&connection.address) else {
            if !self.unreadable {
                self.unreadable = true;
                eprintln!(
                    "adb-watch: cannot read the device socket to {}; not treating it as idle",
                    connection.address
                );
            }
            return;
        };
        self.unreadable = false;
        // A sample taken in the middle of an exchange would split it into two
        // halves that match no known pattern, so let a moving socket finish.
        if self.baseline.is_some_and(|sample| sample.bytes != bytes) {
            thread::sleep(SETTLE);
            bytes = transport_bytes(&connection.address).unwrap_or(bytes);
        }
        self.observe(bytes, &connection.address, now, idle);
        if let Some(activity) = self
            .pending_activity
            .clone()
            .filter(|_| now >= self.next_mark)
        {
            let mark = HistoryCommand::Mark {
                host,
                activity: &activity,
            };
            match self.history(adb, &connection, mark) {
                Ok(_) => {
                    eprintln!(
                        "adb-watch: {} wrote {host} activity marker: {activity}",
                        self.serial
                    );
                    self.pending_activity = None;
                    self.next_mark = now + MARK_INTERVAL;
                }
                Err(error) => {
                    eprintln!(
                        "adb-watch: record ADB use on {}: {error:#}; retrying in {} minutes",
                        self.serial,
                        PHONE_RETRY.as_secs() / 60
                    );
                    self.next_mark = now + PHONE_RETRY;
                }
            }
            self.rebaseline(&connection);
        }
        if self.idle_deadline.is_none_or(|deadline| now >= deadline) && now >= self.shutdown_after {
            self.expire(adb, host, &connection, idle);
        }
    }

    /// Run a history command and learn the property-list size it reports.
    fn history(
        &mut self,
        adb: &str,
        connection: &Connection,
        command: HistoryCommand<'_>,
    ) -> Result<String> {
        let output = phone_shell(adb, &connection.serial, &history_command(command))?;
        let (properties, output) = history_output(&output)?;
        let properties = Some(properties).filter(|size| *size >= PROPERTY_LIST_MIN);
        if properties.is_some() != self.property_bytes.is_some() {
            match properties {
                Some(size) => eprintln!(
                    "adb-watch: {} has a {size}-byte property list; a short request answered by that much is a build tool's read, not use",
                    self.serial
                ),
                None => eprintln!(
                    "adb-watch: {} reports a property list too short to recognise build-tool reads; every payload change counts as use",
                    self.serial
                ),
            }
        }
        self.property_bytes = properties;
        Ok(output.to_owned())
    }

    /// Exclude our own phone command from the next sample.
    fn rebaseline(&mut self, connection: &Connection) {
        thread::sleep(SETTLE);
        self.baseline = transport_bytes(&connection.address).map(|bytes| SocketSample {
            bytes,
            at: boot_time(),
        });
    }

    /// Recover all workstations' history, including our own after a restart.
    /// An empty history starts a new debugging session; only expired history
    /// permits shutdown.
    fn expire(&mut self, adb: &str, host: &str, connection: &Connection, idle: Duration) {
        let result = (|| -> Result<()> {
            let newest = self
                .history(adb, connection, HistoryCommand::Read)
                .and_then(|output| newest_use(&output))
                .context("read shared ADB use")?;
            if let Some(delay) = idle_delay(newest.as_ref().map(|(age, _)| *age), idle) {
                if let Some((age, other)) = newest {
                    eprintln!(
                        "adb-watch: {other}'s activity marker for {} is {} minutes old; keeping Wireless debugging on",
                        self.serial,
                        age.as_secs() / 60
                    );
                } else {
                    let activity = serde_json::json!({
                        "reason": "new_session",
                        "serial": self.serial,
                        "peer": connection.address,
                        "command": null,
                    });
                    let mark = HistoryCommand::Mark {
                        host,
                        activity: &activity,
                    };
                    self.history(adb, connection, mark)
                        .context("start ADB idle history")?;
                    self.pending_activity = None;
                    self.next_mark = boot_time() + MARK_INTERVAL;
                    eprintln!(
                        "adb-watch: started a new Wireless debugging idle window on {}",
                        self.serial
                    );
                }
                self.idle_deadline = Some(boot_time() + delay);
            } else {
                let port = connection.address.parse::<SocketAddr>()?.port();
                let output = self
                    .history(adb, connection, HistoryCommand::Shutdown { idle, port })
                    .context("turn Wireless debugging off")?;
                self.idle_deadline = None;
                self.pending_activity = None;
                match output.trim() {
                    "" => eprintln!(
                        "adb-watch: turned Wireless debugging off on {} after {} minutes without ADB use from any workstation",
                        self.serial,
                        idle.as_secs() / 60
                    ),
                    "active" => eprintln!(
                        "adb-watch: {} activity or debugging port changed during the idle check; postponing shutdown",
                        self.serial
                    ),
                    _ => bail!("unexpected shutdown response: {output:?}"),
                }
            }
            Ok(())
        })();
        self.rebaseline(connection);
        if let Err(error) = result {
            eprintln!(
                "adb-watch: {}: {error:#}; keeping Wireless debugging on and retrying in {} minutes",
                self.serial,
                PHONE_RETRY.as_secs() / 60
            );
            self.shutdown_after = boot_time() + PHONE_RETRY;
        }
    }
}

/// Time since boot including suspend, which `Instant` excludes on Linux. Marker
/// ages on the phone include a suspended workstation's sleep, so the marker
/// deadlines and refresh intervals must count it too.
fn boot_time() -> Duration {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `now` is a valid timespec for the kernel to fill.
    let status = unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut now) };
    assert_eq!(status, 0, "clock_gettime(CLOCK_BOOTTIME) failed");
    Duration::new(now.tv_sec as u64, now.tv_nsec as u32)
}

/// The newest ADB-use marker on the phone and its age, from `date +%s` followed
/// by `stat -c '%Y %n'` lines for each hostname, all on the phone's clock.
/// Our own marker is also needed to recover history after a watcher restart.
fn newest_use(output: &str) -> Result<Option<(Duration, String)>> {
    let mut lines = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let now: u64 = lines
        .next()
        .context("the phone printed no time")?
        .parse()
        .context("invalid phone time")?;
    let mut newest: Option<(Duration, String)> = None;
    for line in lines {
        let (mtime, name) = line
            .split_once(' ')
            .with_context(|| format!("malformed ADB-use marker entry: {line}"))?;
        let mtime: u64 = mtime
            .parse()
            .with_context(|| format!("invalid ADB-use marker time: {line}"))?;
        ensure!(
            plain_name(name),
            "invalid ADB-use marker hostname: {name:?}"
        );
        // A marker from the future is a phone clock jump; read it as current use.
        let age = Duration::from_secs(now.saturating_sub(mtime));
        if newest.as_ref().is_none_or(|(newest, _)| age < *newest) {
            newest = Some((age, name.to_owned()));
        }
    }
    Ok(newest)
}

/// Continued use can be published up to MARK_INTERVAL late. Missing history is
/// a new session, not proof that a phone was idle before we first saw it.
fn idle_delay(age: Option<Duration>, idle: Duration) -> Option<Duration> {
    match age {
        None => Some(idle),
        Some(age) => (idle + MARK_INTERVAL)
            .checked_sub(age)
            .filter(|d| !d.is_zero()),
    }
}

/// How many build-tool property reads explain this sample's traffic, if it is
/// nothing else: only short requests went out, and what came back is that many
/// property lists. Anything else stays unexplained and counts as use.
fn property_probes(sent: u64, received: u64, properties: u64) -> Option<u64> {
    let probes = (received + properties / 2) / properties;
    let tolerance = (properties / PROBE_REPLY_FRACTION).max(PROBE_REPLY_SLACK);
    ((1..=PROBE_BATCH_MAX).contains(&probes)
        && received.abs_diff(probes * properties) <= probes * tolerance
        && sent <= probes * PROBE_REQUEST_MAX)
        .then_some(probes)
}

/// Every history command first prints the size of the phone's property list.
fn history_output(output: &str) -> Result<(u64, &str)> {
    let (properties, rest) = output
        .split_once('\n')
        .context("the phone printed no property-list size")?;
    let properties = properties
        .strip_prefix("properties ")
        .with_context(|| format!("expected the property-list size, not {properties:?}"))?
        .trim()
        .parse()
        .context("invalid property-list size")?;
    Ok((properties, rest))
}

enum HistoryCommand<'a> {
    Read,
    Mark {
        host: &'a str,
        activity: &'a serde_json::Value,
    },
    Shutdown {
        idle: Duration,
        port: u16,
    },
}

/// Android's shell is the adapter for shared files and settings. Serialize these
/// short operations with Android 11's flock, failing open if locked. The kernel
/// releases the lock even if the shell is killed or the phone loses power.
/// Android's shell closes private descriptors on exec: duplicate fd 9 onto
/// stdin for flock, keeping the shared open-file description owned by the shell.
/// Keep history through a failed shutdown. A closing port is retired only once
/// Android advertises a different listener, never on an ordinary TCP reconnect.
fn history_command(command: HistoryCommand<'_>) -> String {
    let operation = match command {
        HistoryCommand::Read => {
            "date +%s; for f in *; do [ -f \"$f\" ] || continue; stat -c '%Y %n' \"$f\"; done"
                .to_owned()
        }
        HistoryCommand::Mark { host, activity } => format!(
            "recorded=$(date +%s)\nprintf '{{\"recorded_at_phone_unix\":%s,\"host\":\"{host}\",\"activity\":%s}}\\n' \"$recorded\" '{}' > .marker-tmp\nmv -f -- .marker-tmp {host}",
            activity.to_string().replace('\'', "'\\''"),
        ),
        HistoryCommand::Shutdown { idle, port } => format!(
            r#"if [ "$port" != {port} ]; then echo active; exit 0; fi
now=$(date +%s)
for f in *; do
    [ -f "$f" ] || continue
    mtime=$(stat -c %Y "$f")
    if [ "$((now - mtime))" -lt {window} ]; then echo active; exit 0; fi
done
printf '%s\n' "$port" > .closing
# Do not hold a lock across a command that tears down this shell's transport.
flock -u 0 <&9
exec 9>&-
settings put global adb_wifi_enabled 0"#,
            window = (idle + MARK_INTERVAL).as_secs(),
        ),
    };
    format!(
        r#"set -eu
mkdir -p {MARKER_DIR}
cd {MARKER_DIR}
exec 9>.lock
flock -nx 0 <&9 || {{ echo 'Cannot lock ADB idle history; leaving debugging on' >&2; exit 1; }}
# Older Android exposes the property; some ROMs expose only the service dump.
# If both report a listener they must agree, including during a port change.
port=$({{ getprop service.adb.tls.port; dumpsys adb | sed -n 's/^[[:space:]]*tls_port=//p'; }} | sed '/^$/d' | sort -u)
case "$port" in ''|*[!0-9]*|0) echo 'invalid Wireless debugging listener port' >&2; exit 1;; esac
if [ -f .closing ] && [ "$(cat .closing)" != "$port" ]; then
    rm -f -- * .closing
fi
# Build tools read this list from every listed phone; its size lets the watcher
# tell those reads from use without asking the phone anything more.
echo "properties $(getprop | wc -c)"
{operation}"#,
    )
}

/// The SDK client under a hard timeout, with any server redirection removed so it
/// talks to the local server this watcher uses.
fn client(adb: &str) -> Command {
    let mut command = Command::new("timeout");
    command
        .args(["--foreground", "--kill-after=2s", "10s", adb])
        .env_remove("ADB_SERVER_SOCKET")
        .env_remove("ANDROID_ADB_SERVER_ADDRESS")
        .env_remove("ANDROID_ADB_SERVER_PORT")
        .stdin(Stdio::null());
    command
}

/// ADB's length-prefixed smart-socket protocol on loopback, shared with the SDK.
struct AdbServer(SocketAddr);

impl AdbServer {
    fn request(&self, service: &str) -> Result<TcpStream> {
        ensure!(
            service.len() <= 0xffff,
            "ADB request exceeds the protocol limit"
        );
        let mut socket = TcpStream::connect_timeout(&self.0, REQUEST_TIMEOUT)
            .context("connect to the local ADB server")?;
        socket.set_read_timeout(Some(REQUEST_TIMEOUT))?;
        socket.set_write_timeout(Some(REQUEST_TIMEOUT))?;
        socket.set_nodelay(true)?;
        socket.write_all(format!("{:04x}{service}", service.len()).as_bytes())?;
        let mut status = [0; 4];
        socket.read_exact(&mut status)?;
        match &status {
            b"OKAY" => Ok(socket),
            b"FAIL" => bail!("ADB {service}: {}", read_frame(&mut socket)?),
            _ => bail!("invalid ADB response: {status:?}"),
        }
    }

    fn query(&self, service: &str) -> Result<String> {
        read_frame(&mut self.request(service)?).with_context(|| format!("ADB {service}"))
    }
}

fn read_frame(reader: &mut impl Read) -> Result<String> {
    let mut length = [0; 4];
    reader
        .read_exact(&mut length)
        .context("read ADB frame length")?;
    ensure!(
        length.iter().all(u8::is_ascii_hexdigit),
        "invalid ADB frame length"
    );
    let length = usize::from_str_radix(std::str::from_utf8(&length)?, 16)
        .context("invalid ADB frame length")?;
    let mut bytes = vec![0; length];
    reader
        .read_exact(&mut bytes)
        .context("read ADB frame body")?;
    String::from_utf8(bytes).context("ADB returned non-UTF-8 output")
}

fn update_devices(tracker: &mut TcpStream, devices: &mut String, timeout_ms: i32) -> Result<()> {
    let mut event = libc::pollfd {
        fd: tracker.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let mut timeout = timeout_ms;
    loop {
        let ready = unsafe { libc::poll(&mut event, 1, timeout) };
        if ready < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error).context("wait for ADB device changes");
        }
        if ready == 0 {
            return Ok(());
        }
        // EOF also wakes poll: read_frame reports it, triggering service recovery.
        // Each notification replaces the full list; keep the latest queued snapshot.
        *devices = read_frame(tracker)?;
        timeout = 0;
    }
}

#[derive(Debug, PartialEq)]
struct Service {
    name: String,
    address: String,
}

fn discover(output: &str, serial: &str) -> Result<Vec<Service>> {
    let prefix = format!("adb-{serial}-");
    let mut services = Vec::new();
    for line in output.lines() {
        let mut fields = line.split_whitespace();
        let (Some(name), Some(kind)) = (fields.next(), fields.next()) else {
            continue;
        };
        if !name.starts_with(&prefix) || kind.trim_end_matches('.') != SERVICE_TYPE {
            continue;
        }
        let address: SocketAddr = fields
            .next()
            .with_context(|| format!("missing address in ADB discovery entry: {line}"))?
            .parse()
            .context("invalid ADB discovery address")?;
        ensure!(
            fields.next().is_none(),
            "malformed ADB discovery entry: {line}"
        );
        ensure!(address.port() != 0, "ADB advertised port zero");
        let service = Service {
            name: format!("{name}.{SERVICE_TYPE}"),
            address: address.to_string(),
        };
        if !services.contains(&service) {
            services.push(service);
        }
    }
    Ok(services)
}

fn matching_devices<'a>(
    devices: &'a str,
    service: &'a Service,
) -> impl Iterator<Item = (&'a str, &'a str)> {
    devices.lines().filter_map(move |line| {
        let mut fields = line.split_whitespace();
        let name = fields.next()?;
        let state = fields.next()?;
        (name.trim_end_matches('.') == service.name || name == service.address)
            .then_some((name, state))
    })
}

/// Device entries named after this phone's mDNS service, however they are spelled.
fn stale_transports<'a>(devices: &'a str, serial: &str) -> impl Iterator<Item = &'a str> {
    let prefix = format!("adb-{serial}-");
    devices.lines().filter_map(move |line| {
        let name = line.split_whitespace().next()?;
        (name.starts_with(&prefix) && name.trim_end_matches('.').ends_with(SERVICE_TYPE))
            .then_some(name)
    })
}

/// Disconnecting a transport that is already gone reaches the same end state.
fn disconnect(query: &mut impl FnMut(&str) -> Result<String>, name: &str) -> Result<()> {
    match query(&format!("host:disconnect:{name}")) {
        Ok(_) => Ok(()),
        Err(error) => {
            // ADB answers `no such device` for a gone address and `couldn't parse`
            // for a gone mDNS name, which it cannot read as a socket spec.
            let message = format!("{error:#}");
            if message.contains("no such device") || message.contains("couldn't parse") {
                Ok(())
            } else {
                Err(error).with_context(|| format!("disconnect {name}"))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
struct SocketBytes {
    sent: u64,
    received: u64,
    retransmitted: u64,
}

#[derive(Clone, Copy)]
struct SocketSample {
    bytes: SocketBytes,
    at: Duration,
}

/// Payload bytes moved on the device socket to `address`, from `ss` TCP info.
/// Keepalive probes carry no payload, so unchanged bytes mean no ADB use.
fn transport_bytes(address: &str) -> Option<SocketBytes> {
    let output = Command::new("ss").args(["-tnHi"]).output().ok()?;
    bytes_for_peer(&String::from_utf8_lossy(&output.stdout), address)
}

fn bytes_for_peer(text: &str, address: &str) -> Option<SocketBytes> {
    let field = peer_fields(text, address)?;
    Some(SocketBytes {
        sent: field("bytes_sent:")?.parse().ok()?,
        received: field("bytes_received:")?.parse().ok()?,
        // ss omits this field when no payload has been retransmitted.
        retransmitted: field("bytes_retrans:").unwrap_or("0").parse().ok()?,
    })
}

/// Whether the phone acknowledged the device socket to `address` within
/// `ABSENT_GRACE`. ADB probes an idle device socket every second, so a phone
/// that is still there always has; one that left stops, whatever mDNS shows.
fn answering(address: &str) -> bool {
    Command::new("ss")
        .args(["-tnHi"])
        .output()
        .ok()
        .and_then(|output| last_ack_for_peer(&String::from_utf8_lossy(&output.stdout), address))
        .is_some_and(|age| age < ABSENT_GRACE)
}

/// Time since the peer's latest acknowledgement, which keepalive replies renew.
fn last_ack_for_peer(text: &str, address: &str) -> Option<Duration> {
    let milliseconds = peer_fields(text, address)?("lastack:")?.parse().ok()?;
    Some(Duration::from_millis(milliseconds))
}

/// Lookup of the `name:value` TCP info fields `ss` prints for the socket to `address`.
fn peer_fields<'a>(text: &'a str, address: &str) -> Option<impl Fn(&str) -> Option<&'a str>> {
    let wanted = address.replace(['[', ']'], "");
    let mut matched = false;
    for line in text.lines() {
        if !line.starts_with('\t') {
            matched = line
                .split_whitespace()
                .nth(4)
                .is_some_and(|peer| peer.replace(['[', ']'], "") == wanted);
        } else if matched {
            return Some(move |name: &str| {
                line.split_whitespace()
                    .find_map(|field| field.strip_prefix(name))
            });
        }
    }
    None
}

/// Run one shell command on the phone through the local server.
fn phone_shell(adb: &str, serial: &str, command: &str) -> Result<String> {
    let output = util::command_output(client(adb).args(["-s", serial, "shell", command]))
        .with_context(|| format!("run `{command}` on {serial}"))?;
    String::from_utf8(output).with_context(|| format!("{serial} returned non-UTF-8 output"))
}

/// Serials and hostnames are spliced into ADB names and phone shell commands.
fn plain_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

#[derive(Clone, Debug, PartialEq)]
struct Connection {
    /// Serial for `adb -s`: the advertised mDNS name or the address.
    serial: String,
    /// Address the service advertises, which is also the device socket's peer.
    address: String,
}

fn listed(devices: &str, name: &str) -> bool {
    devices
        .lines()
        .any(|line| line.split_whitespace().next() == Some(name))
}

/// The transport a previous watcher noted at `path`, if any.
fn recall(path: &Path) -> Option<Connection> {
    let note = match fs::read_to_string(path) {
        Ok(note) => note,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
        Err(error) => {
            eprintln!("adb-watch: read {}: {error}", path.display());
            return None;
        }
    };
    let mut fields = note.split_whitespace();
    match (fields.next(), fields.next(), fields.next()) {
        (Some(serial), Some(address), None) if address.parse::<SocketAddr>().is_ok() => {
            Some(Connection {
                serial: serial.to_owned(),
                address: address.to_owned(),
            })
        }
        _ => {
            eprintln!(
                "adb-watch: ignoring the malformed transport note {}",
                path.display()
            );
            None
        }
    }
}

/// Whether an online transport is named after this phone's mDNS service.
fn online_unadvertised(devices: &str, serial: &str) -> bool {
    stale_transports(devices, serial).any(|name| {
        devices.lines().any(|line| {
            let mut fields = line.split_whitespace();
            fields.next() == Some(name) && fields.next() == Some("device")
        })
    })
}

/// Bring the phone's transport in line with what it advertises: connect when it
/// is missing or stale, drop what nothing advertises unless the phone still
/// answers on it, and return the transport the watcher now holds. `remembered`
/// is the last one any watcher held, taken up again if the phone answers on it.
fn reconcile(
    serial: &str,
    devices: &str,
    held: Option<&Connection>,
    remembered: Option<&Connection>,
    missing_since: &mut Option<Instant>,
    mut query: impl FnMut(&str) -> Result<String>,
    answering: impl Fn(&str) -> bool,
) -> Result<Option<Connection>> {
    let services = discover(&query("host:mdns:services")?, serial)?;
    if services.is_empty() {
        // Cache expiry and a Wi-Fi roam look exactly like a phone that left, so a
        // quiet mDNS list alone must not cut the transport: wait for the grace
        // period, during which the transport is reported as still held. Past it,
        // discovery that stays quiet (multicast is unreliable between Wi-Fi
        // clients) still must not cut a transport the phone answers on, or this
        // workstation loses the phone and its use goes unseen until mDNS returns.
        let since = missing_since.get_or_insert_with(Instant::now);
        if let Some(held) = held.filter(|held| listed(devices, &held.serial)) {
            if since.elapsed() < ABSENT_GRACE || answering(&held.address) {
                return Ok(Some(held.clone()));
            }
            disconnect(&mut query, &held.serial)?;
            return Ok(None);
        }
        // The ADB server outlives this watcher and keeps its transports, so a
        // restart or a reconnect by the server can leave one online that this
        // watcher does not hold. It is the remembered one if the phone answers
        // at that address; any other stays unwatched, and is never cut.
        return Ok(remembered
            .filter(|known| listed(devices, &known.serial) && answering(&known.address))
            .cloned());
    }
    *missing_since = None;
    // A phone that reboots under a new instance name or moves to another port
    // leaves its transport in `device` state, because TCP cannot tell a silent
    // peer from a slow one until it gives up (tcp_retries2, many minutes) and
    // every command waits on that. Drop what nothing advertises.
    let mut stale = stale_transports(devices, serial)
        .filter(|name| {
            !services
                .iter()
                .any(|service| service.name.trim_end_matches('.') == name.trim_end_matches('.'))
        })
        .collect::<Vec<_>>();
    // An address-named transport outlives the port it was opened on.
    if let Some(held) = held {
        if held.serial.parse::<SocketAddr>().is_ok()
            && !services
                .iter()
                .any(|service| service.address == held.serial)
        {
            stale.push(&held.serial);
        }
    }
    for name in stale {
        disconnect(&mut query, name)?;
    }
    // Offline snapshots can outlive their transports; refresh before disconnecting.
    let current = if services
        .iter()
        .any(|service| matching_devices(devices, service).any(|(_, state)| state != "device"))
    {
        Some(query("host:devices")?)
    } else {
        None
    };
    let devices = current.as_deref().unwrap_or(devices);
    for service in &services {
        if let Some((serial, _)) =
            matching_devices(devices, service).find(|(_, state)| *state == "device")
        {
            return Ok(Some(Connection {
                serial: serial.to_owned(),
                address: service.address.clone(),
            }));
        }
    }
    let mut errors = Vec::new();
    for service in services {
        let attempt = (|| -> Result<String> {
            // Clear only this phone's stale transport. USB and other phones are untouched.
            for (name, _) in matching_devices(devices, &service) {
                disconnect(&mut query, name)?;
            }
            let response = query(&format!("host:connect:{}", service.address))?;
            ensure!(
                response.starts_with("connected to ")
                    || response.starts_with("already connected to "),
                "{}",
                response.trim()
            );
            // "Already connected" does not guarantee an online transport.
            let devices = query("host:devices")?;
            matching_devices(&devices, &service)
                .find(|(_, state)| *state == "device")
                .map(|(serial, _)| serial.to_owned())
                .ok_or_else(|| {
                    anyhow!(
                        "{} is not online yet (check pairing if this persists)",
                        service.address
                    )
                })
        })();
        match attempt {
            Ok(serial) => {
                return Ok(Some(Connection {
                    serial,
                    address: service.address,
                }));
            }
            Err(error) => errors.push(format!("{}: {error:#}", service.address)),
        }
    }
    bail!("{}", errors.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISCOVERY: &str = "List of discovered mdns services\nadb-phone-new _adb-tls-connect._tcp 192.168.1.76:41325\nadb-phone-new _adb-tls-pairing._tcp 192.168.1.76:39999\nadb-other-new _adb-tls-connect._tcp 192.168.1.77:44444\nadb-phone2-new _adb-tls-connect._tcp 192.168.1.78:44444\n";

    fn cycle(devices: &str, steps: &[(&str, &str)]) -> Result<Option<Connection>> {
        from(None, devices, steps)
    }

    fn connection(serial: &str, address: &str) -> Connection {
        Connection {
            serial: serial.to_owned(),
            address: address.to_owned(),
        }
    }

    fn from(
        held: Option<&Connection>,
        devices: &str,
        steps: &[(&str, &str)],
    ) -> Result<Option<Connection>> {
        reconcile_with(None, held, None, devices, steps, false)
    }

    /// Run one reconciliation as if the phone had been absent past the grace period.
    fn after_grace(
        held: Option<&Connection>,
        devices: &str,
        steps: &[(&str, &str)],
    ) -> Result<Option<Connection>> {
        reconcile_with(past_grace(), held, None, devices, steps, false)
    }

    fn past_grace() -> Option<Instant> {
        Instant::now().checked_sub(ABSENT_GRACE + Duration::from_secs(1))
    }

    fn reconcile_with(
        missing_since: Option<Instant>,
        held: Option<&Connection>,
        remembered: Option<&Connection>,
        devices: &str,
        steps: &[(&str, &str)],
        answering: bool,
    ) -> Result<Option<Connection>> {
        let mut missing_since = missing_since;
        let mut steps = steps.iter();
        let result = reconcile(
            "phone",
            devices,
            held,
            remembered,
            &mut missing_since,
            |service| {
                let &(expected, response) = steps.next().expect("unexpected ADB request");
                assert_eq!(service, expected);
                Ok(response.to_owned())
            },
            |address| {
                assert_eq!(
                    Some(address),
                    held.or(remembered).map(|known| known.address.as_str())
                );
                answering
            },
        );
        assert!(
            steps.as_slice().is_empty(),
            "missing ADB requests: {steps:?}"
        );
        result
    }

    #[test]
    fn reconnects_changed_port_and_ignores_other_phones_and_pairing() {
        let address = cycle(
            "192.168.1.76:30000 offline\nemulator-5554 device\n",
            &[
                ("host:mdns:services", DISCOVERY),
                (
                    "host:connect:192.168.1.76:41325",
                    "connected to 192.168.1.76:41325\n",
                ),
                ("host:devices", "192.168.1.76:41325 device\n"),
            ],
        )
        .unwrap();
        assert_eq!(
            address
                .as_ref()
                .map(|connection| connection.address.as_str()),
            Some("192.168.1.76:41325")
        );
    }

    #[test]
    fn leaves_online_mdns_and_ip_transports_alone() {
        for devices in [
            "adb-phone-new._adb-tls-connect._tcp. device\n",
            "192.168.1.76:41325 device\n",
        ] {
            assert!(
                cycle(devices, &[("host:mdns:services", DISCOVERY)])
                    .unwrap()
                    .is_some()
            );
        }
    }

    #[test]
    fn repairs_only_the_matching_offline_transport() {
        cycle(
            "adb-phone-new._adb-tls-connect._tcp offline\nUSB123 device\n",
            &[
                ("host:mdns:services", DISCOVERY),
                (
                    "host:devices",
                    "adb-phone-new._adb-tls-connect._tcp offline\nUSB123 device\n",
                ),
                (
                    "host:disconnect:adb-phone-new._adb-tls-connect._tcp",
                    "disconnected\n",
                ),
                (
                    "host:connect:192.168.1.76:41325",
                    "connected to 192.168.1.76:41325\n",
                ),
                ("host:devices", "192.168.1.76:41325 device\n"),
            ],
        )
        .unwrap();
    }

    #[test]
    fn refreshes_offline_snapshots_before_disconnecting() {
        let address = cycle(
            "192.168.1.76:41325 offline\n",
            &[
                ("host:mdns:services", DISCOVERY),
                ("host:devices", ""),
                (
                    "host:connect:192.168.1.76:41325",
                    "connected to 192.168.1.76:41325\n",
                ),
                ("host:devices", "192.168.1.76:41325 device\n"),
            ],
        )
        .unwrap();
        assert_eq!(
            address
                .as_ref()
                .map(|connection| connection.address.as_str()),
            Some("192.168.1.76:41325")
        );

        let address = cycle(
            "192.168.1.76:41325 offline\n",
            &[
                ("host:mdns:services", DISCOVERY),
                ("host:devices", "192.168.1.76:41325 device\n"),
            ],
        )
        .unwrap();
        assert_eq!(
            address
                .as_ref()
                .map(|connection| connection.address.as_str()),
            Some("192.168.1.76:41325")
        );
    }

    #[test]
    fn reports_connect_failure_even_with_an_okay_protocol_status() {
        let result = cycle(
            "",
            &[
                ("host:mdns:services", DISCOVERY),
                (
                    "host:connect:192.168.1.76:41325",
                    "failed to authenticate to 192.168.1.76:41325\n",
                ),
            ],
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("failed to authenticate")
        );
    }

    #[test]
    fn finds_the_device_sockets_payload_bytes() {
        let text = "ESTAB 0 0 192.168.1.2:50000 192.168.1.76:41325\n\
                    \t cubic rto:204 bytes_sent:1200 bytes_retrans:0 bytes_acked:1200 bytes_received:340\n\
                    ESTAB 0 0 192.168.1.2:50001 192.168.1.76:9999\n\
                    \t cubic bytes_sent:7 bytes_received:9\n";
        assert_eq!(
            bytes_for_peer(text, "192.168.1.76:41325"),
            Some(SocketBytes {
                sent: 1200,
                received: 340,
                retransmitted: 0,
            })
        );
        assert_eq!(bytes_for_peer(text, "192.168.1.76:4242"), None);
        // Counters that cannot be read must not look like an idle socket.
        assert_eq!(
            bytes_for_peer("ESTAB 0 0 1.2.3.4:1 1.2.3.5:2\n\t cubic\n", "1.2.3.5:2"),
            None
        );
        assert_eq!(
            bytes_for_peer(
                "ESTAB 0 0 [::ffff:1.2.3.4]:50000 [::ffff:1.2.3.5]:40000\n\t bytes_sent:5 bytes_received:6\n",
                "[::ffff:1.2.3.5]:40000"
            ),
            Some(SocketBytes {
                sent: 5,
                received: 6,
                retransmitted: 0
            })
        );
        assert_eq!(
            bytes_for_peer(
                "ESTAB 0 0 1.2.3.4:1 1.2.3.5:2\n\t bytes_sent:8 bytes_received:9 bytes_retrans:4\n",
                "1.2.3.5:2",
            ),
            Some(SocketBytes {
                sent: 8,
                received: 9,
                retransmitted: 4
            })
        );
        assert_eq!(
            bytes_for_peer(
                "ESTAB 0 0 1.2.3.4:1 1.2.3.5:2\n\t bytes_sent:8 bytes_received:9 bytes_retrans:invalid\n",
                "1.2.3.5:2",
            ),
            None
        );
    }

    fn sent(bytes: u64) -> SocketBytes {
        SocketBytes {
            sent: bytes,
            received: 0,
            retransmitted: 0,
        }
    }

    #[test]
    fn finds_the_newest_shared_use_on_the_phones_clock() -> Result<()> {
        let output = "1000\n700 rpi\n940 laptop\n990 main\n";
        assert_eq!(
            newest_use(output)?,
            Some((Duration::from_secs(10), "main".to_owned()))
        );
        assert_eq!(newest_use("1000\n")?, None);
        assert_eq!(
            newest_use("1000\n1200 laptop\n")?,
            Some((Duration::ZERO, "laptop".to_owned()))
        );
        // Unreadable output must not look like an idle phone.
        for output in [
            "",
            "1000\nlaptop\n",
            "1000\nsoon laptop\n",
            "1000\n990 ../main\n",
        ] {
            assert!(newest_use(output).is_err());
        }
        Ok(())
    }

    #[test]
    fn reconnects_preserve_history_but_unknown_activity_is_conservative() {
        let idle = Duration::from_secs(180 * 60);
        let mut phone = Phone::new("phone");
        let peer = "192.168.1.76:41325";
        phone.reset_transport(Duration::ZERO, Some(sent(100)));
        phone.observe(sent(200), peer, Duration::from_secs(60), idle);
        let deadline = phone.idle_deadline;
        let pending = phone.pending_activity.clone();
        assert!(pending.is_some());

        // A new socket's handshake has different counters, but is not ADB use.
        phone.reset_transport(Duration::from_secs(600), Some(sent(9000)));
        phone.observe(
            sent(9000),
            "192.168.1.76:42000",
            Duration::from_secs(660),
            idle,
        );
        assert_eq!(phone.idle_deadline, deadline);
        assert_eq!(
            phone.pending_activity, pending,
            "reconnect must retain the pending detection"
        );

        phone.reset_transport(Duration::from_secs(1200), None);
        phone.observe(sent(20_000), peer, Duration::from_secs(1260), idle);
        assert_eq!(phone.idle_deadline, Some(Duration::from_secs(1260) + idle));
        let unknown = phone.pending_activity.as_ref().unwrap();
        assert_eq!(unknown["reason"], "unknown_baseline");
        assert!(unknown["previous"].is_null());
        assert!(unknown["delta"].is_null());
        phone.observe(sent(20_001), peer, Duration::from_secs(1320), idle);
        assert_eq!(phone.idle_deadline, Some(Duration::from_secs(1320) + idle));
    }

    #[test]
    fn suspend_counts_toward_the_idle_deadline_and_next_sample() {
        let idle = Duration::from_secs(180 * 60);
        let mut phone = Phone::new("phone");
        let peer = "192.168.1.76:41325";
        phone.reset_transport(Duration::ZERO, Some(sent(100)));
        phone.observe(sent(200), peer, Duration::from_secs(60), idle);

        // CLOCK_BOOTTIME advances across sleep even though Instant does not.
        let resumed = Duration::from_secs(4 * 60 * 60);
        phone.observe(sent(200), peer, resumed, idle);
        assert!(resumed >= phone.idle_deadline.unwrap());
        assert!(resumed >= phone.next_sample);
    }

    #[test]
    fn activity_trace_records_device_counters_without_inventing_a_command() {
        let mut phone = Phone::new("phone");
        let peer = "192.168.1.76:41325";
        phone.reset_transport(
            Duration::from_secs(10),
            Some(SocketBytes {
                sent: 100,
                received: 200,
                retransmitted: 0,
            }),
        );
        phone.observe(
            SocketBytes {
                sent: 120,
                received: 250,
                retransmitted: 5,
            },
            peer,
            Duration::from_secs(70),
            Duration::from_secs(180 * 60),
        );
        let trace = phone.pending_activity.as_ref().unwrap();
        assert_eq!(trace["reason"], "payload_changed");
        assert_eq!(trace["serial"], "phone");
        assert_eq!(trace["peer"], peer);
        assert_eq!(trace["sample_elapsed_ms"], 60000);
        assert_eq!(
            trace["delta"],
            serde_json::json!({"sent": 20, "received": 50, "retransmitted": 5})
        );
        assert!(trace["detected_at_host_unix"].as_u64().unwrap() > 0);
        assert!(trace["command"].is_null());

        phone.observe(
            sent(10),
            peer,
            Duration::from_secs(130),
            Duration::from_secs(180 * 60),
        );
        let trace = phone.pending_activity.as_ref().unwrap();
        assert_eq!(trace["reason"], "counters_decreased");
        assert!(trace["delta"]["sent"].is_null());
        assert!(trace["delta"]["received"].is_null());
    }

    #[test]
    fn another_devices_traffic_does_not_create_an_activity_trace() {
        let peer = "192.168.1.76:41325";
        let mut phone = Phone::new("phone");
        phone.reset_transport(Duration::ZERO, Some(sent(100)));
        let text = "ESTAB 0 0 127.0.0.1:5037 127.0.0.1:50000\n\t bytes_sent:999 bytes_received:999\n\
                    ESTAB 0 0 192.168.1.2:50001 192.168.1.77:41325\n\t bytes_sent:10000 bytes_received:20000\n\
                    ESTAB 0 0 192.168.1.2:50002 192.168.1.76:41325\n\t bytes_sent:100 bytes_received:0\n";
        phone.observe(
            bytes_for_peer(text, peer).unwrap(),
            peer,
            Duration::from_secs(60),
            Duration::from_secs(180 * 60),
        );
        assert!(phone.pending_activity.is_none());
        assert!(phone.idle_deadline.is_none());
    }

    /// Main's journal, 10–11 November: Gradle builds for the USB phone read this
    /// phone's property list, 174 bytes out and about 85.5 KB back each time.
    const PROPERTIES: u64 = 85_300;

    fn traffic(sent: u64, received: u64) -> SocketBytes {
        SocketBytes {
            sent,
            received,
            retransmitted: 0,
        }
    }

    #[test]
    fn build_tool_property_reads_are_not_use() {
        for (sent, received, probes) in [
            (174, 85_558, Some(1)),
            (174, 85_849, Some(1)),
            // More acknowledgements, a retransmitted request, two builds at once.
            (394, 85_543, Some(1)),
            (210, 85_601, Some(1)),
            (348, 171_116, Some(2)),
            // A list that drifted since it was measured still matches.
            (174, 84_100, Some(1)),
            // Ordinary commands: a tap, a screenshot, an install, a long log.
            (250, 120, None),
            (300, 1_068_512, None),
            (3_894, 85_558, None),
            (60_000_000, 85_558, None),
            (174, 80_000, None),
            (174, 91_000, None),
            (174, 128_000, None),
            (870, 427_790, None),
            (0, 0, None),
        ] {
            assert_eq!(
                property_probes(sent, received, PROPERTIES),
                probes,
                "sent {sent}, received {received}"
            );
        }
    }

    #[test]
    fn a_property_read_leaves_the_idle_deadline_and_marker_alone() {
        let idle = Duration::from_secs(180 * 60);
        let peer = "192.168.1.76:41325";
        let mut phone = Phone::new("phone");
        phone.reset_transport(Duration::ZERO, Some(traffic(1_000, 5_000)));
        // Until the phone has reported its list, the same traffic counts as use.
        phone.observe(traffic(1_174, 90_558), peer, Duration::from_secs(60), idle);
        let deadline = phone.idle_deadline;
        assert_eq!(deadline, Some(Duration::from_secs(60) + idle));
        phone.pending_activity = None;

        phone.property_bytes = Some(PROPERTIES);
        phone.observe(
            traffic(1_348, 176_116),
            peer,
            Duration::from_secs(120),
            idle,
        );
        assert_eq!(phone.idle_deadline, deadline);
        assert!(phone.pending_activity.is_none());
        // The read still moves the baseline, so the next command stands alone.
        phone.observe(
            traffic(1_598, 176_236),
            peer,
            Duration::from_secs(180),
            idle,
        );
        assert_eq!(phone.idle_deadline, Some(Duration::from_secs(180) + idle));
        assert_eq!(
            phone.pending_activity.as_ref().unwrap()["delta"],
            serde_json::json!({"sent": 250, "received": 120, "retransmitted": 0})
        );
        // Counters that restarted say nothing about what was exchanged.
        phone.pending_activity = None;
        phone.observe(traffic(174, 85_558), peer, Duration::from_secs(240), idle);
        assert_eq!(
            phone.pending_activity.as_ref().unwrap()["reason"],
            "counters_decreased"
        );
    }

    #[test]
    fn history_commands_report_the_property_list_size() -> Result<()> {
        let fixture = HistoryFixture::new()?;
        fixture.expired_marker("main")?;
        let properties = "[ro.serialno]: [phone]\n".repeat(1000);
        for action in [
            HistoryCommand::Read,
            HistoryCommand::Mark {
                host: "laptop",
                activity: &serde_json::json!({}),
            },
            HistoryCommand::Shutdown {
                idle: Duration::from_secs(180 * 60),
                port: 41000,
            },
        ] {
            let output = util::command_output(
                fixture
                    .command(action, 40000)
                    .env("PROPERTIES", &properties),
            )?;
            let (size, _) = history_output(std::str::from_utf8(&output)?)?;
            assert_eq!(size, properties.len() as u64);
        }
        // Output that lost its first line must not be read as marker history.
        for output in ["", "1000\n990 main\n", "properties many\n", "properties 12"] {
            assert!(history_output(output).is_err(), "{output:?}");
        }
        assert_eq!(
            history_output("properties  85300 \n1000\n")?,
            (85_300, "1000\n")
        );
        Ok(())
    }

    #[test]
    fn restart_recovers_our_own_history_without_a_fresh_idle_window() -> Result<()> {
        let idle = Duration::from_secs(180 * 60);
        // Main's marker was written 3h20 ago; laptop's is even older.
        let history = newest_use("20000\n8000 main\n7000 laptop\n")?;
        let remaining = idle_delay(history.map(|(age, _)| age), idle);
        assert_eq!(remaining, Some(Duration::from_secs(10 * 60)));
        // Neither a new process nor a newly booted workstation needs its old clock.
        for boot in [Duration::from_secs(5), Duration::from_secs(5 * 86400)] {
            let deadline = boot + remaining.unwrap();
            assert_eq!(deadline - boot, Duration::from_secs(10 * 60));
        }
        assert_eq!(idle_delay(Some(idle + MARK_INTERVAL), idle), None);
        assert_eq!(idle_delay(Some(idle + MARK_INTERVAL * 2), idle), None);
        // A retired closing listener starts its next session without old markers.
        assert_eq!(idle_delay(None, idle), Some(idle));
        Ok(())
    }

    #[test]
    fn another_workstations_recent_use_protects_an_idle_or_restarted_host() -> Result<()> {
        let idle = Duration::from_secs(180 * 60);
        let newest = newest_use("20000\n8000 main\n19940 laptop\n")?.unwrap();
        assert_eq!(newest.1, "laptop");
        assert_eq!(
            idle_delay(Some(newest.0), idle),
            Some(idle + MARK_INTERVAL - Duration::from_secs(60))
        );
        Ok(())
    }

    struct HistoryFixture(tempfile::TempDir);

    impl HistoryFixture {
        fn new() -> Result<Self> {
            Ok(Self(tempfile::tempdir()?))
        }

        fn script(&self, action: HistoryCommand<'_>) -> String {
            history_command(action).replace(MARKER_DIR, self.0.path().to_str().unwrap())
        }

        fn command(&self, action: HistoryCommand<'_>, port: u16) -> Command {
            let mut command = Command::new(
                std::env::var_os("HOST_TOOLS_TEST_SHELL").unwrap_or_else(|| "/bin/sh".into()),
            );
            command
                .args([
                    "-c",
                    &format!(
                        r#"
# Simulate Android closing fd 9 when it executes an external flock.
flock() {{ (exec 9>&-; command flock "$@"); }}
getprop() {{
    if [ $# = 0 ]; then printf '%s' "$PROPERTIES"; else printf '%s\n' "$PROP_PORT"; fi
}}
dumpsys() {{
    if command flock -nx 0 <.lock; then
        echo 'History lock was not retained by the shell' >&2
        return 1
    fi
    printf '  tls_port=%s\n' "$PHONE_PORT"
}}
settings() {{
    printf '%s\n' "$*" >> "$CALLS"
    if [ -n "$RACING_MARK" ]; then (eval "$RACING_MARK"); fi
    [ "$SETTINGS_FAIL" = 0 ]
}}
{}"#,
                        self.script(action)
                    ),
                ])
                .env("PHONE_PORT", port.to_string())
                .env("PROP_PORT", "")
                .env("PROPERTIES", "[ro.serialno]: [phone]\n")
                .env("CALLS", self.0.path().join(".calls"))
                .env("RACING_MARK", "")
                .env("SETTINGS_FAIL", "0");
            command
        }

        /// Run a history command and return its own output, past the size line.
        fn run(&self, command: &mut Command) -> Result<String> {
            let output = String::from_utf8(util::command_output(command)?)?;
            Ok(history_output(&output)?.1.to_owned())
        }

        fn expired_marker(&self, host: &str) -> Result<()> {
            use std::{
                fs::{File, FileTimes},
                time::SystemTime,
            };
            let file = File::create(self.0.path().join(host))?;
            file.set_times(
                FileTimes::new().set_modified(SystemTime::now() - Duration::from_secs(4 * 60 * 60)),
            )?;
            Ok(())
        }
    }

    #[test]
    fn marker_contains_the_latest_trace_but_keeps_mtime_coordination() -> Result<()> {
        let fixture = HistoryFixture::new()?;
        fixture.expired_marker("main")?;
        let activity = serde_json::json!({
            "reason": "payload_changed",
            "serial": "phone",
            "detected_at_host_unix": 123,
            "command": null,
            "note": "it's literal $(touch INJECTED) `touch BACKTICK`\na second line",
        });
        util::command_output(&mut fixture.command(
            HistoryCommand::Mark {
                host: "main",
                activity: &activity,
            },
            40000,
        ))?;
        let marker: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(fixture.0.path().join("main"))?)?;
        assert_eq!(marker["host"], "main");
        assert_eq!(marker["activity"], activity);
        assert!(marker["recorded_at_phone_unix"].as_u64().unwrap() > 123);
        for file in [".marker-tmp", "INJECTED", "BACKTICK"] {
            assert!(!fixture.0.path().join(file).exists());
        }
        // Old empty markers remain readable, and an interrupted hidden write
        // cannot look like another workstation or change its idle history.
        fixture.expired_marker("laptop")?;
        std::fs::write(fixture.0.path().join(".marker-tmp"), "partial")?;
        let history = fixture.run(&mut fixture.command(HistoryCommand::Read, 40000))?;
        assert_eq!(newest_use(&history)?.unwrap().1, "main");

        util::command_output(&mut fixture.command(
            HistoryCommand::Mark {
                host: "main",
                activity: &serde_json::json!({"reason": "new_session", "command": null}),
            },
            40000,
        ))?;
        let marker: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(fixture.0.path().join("main"))?)?;
        assert_eq!(marker["activity"]["reason"], "new_session");
        assert!(
            marker["activity"]["note"].is_null(),
            "replace, do not append"
        );
        Ok(())
    }

    #[test]
    fn failed_shutdown_retains_history_and_retries_without_new_grace() -> Result<()> {
        let fixture = HistoryFixture::new()?;
        fixture.expired_marker("main")?;
        let idle = Duration::from_secs(180 * 60);
        let shutdown = || HistoryCommand::Shutdown { idle, port: 40000 };
        assert!(
            !fixture
                .command(shutdown(), 40000)
                .env("SETTINGS_FAIL", "1")
                .output()?
                .status
                .success()
        );
        assert_eq!(
            std::fs::read_to_string(fixture.0.path().join(".closing"))?,
            "40000\n"
        );
        let history = fixture.run(&mut fixture.command(HistoryCommand::Read, 40000))?;
        let age = newest_use(&history)?.unwrap().0;
        assert_eq!(idle_delay(Some(age), idle), None);
        assert!(
            fixture
                .run(&mut fixture.command(shutdown(), 40000))?
                .is_empty()
        );
        assert_eq!(
            std::fs::read_to_string(fixture.0.path().join(".calls"))?
                .lines()
                .count(),
            2
        );
        Ok(())
    }

    #[test]
    fn closing_session_discards_a_racing_publication_only_on_new_listener() -> Result<()> {
        let fixture = HistoryFixture::new()?;
        fixture.expired_marker("main")?;
        let idle = Duration::from_secs(180 * 60);
        util::command_output(
            fixture
                .command(HistoryCommand::Shutdown { idle, port: 40000 }, 40000)
                .env(
                    "RACING_MARK",
                    fixture.script(HistoryCommand::Mark {
                        host: "laptop",
                        activity: &serde_json::json!({}),
                    }),
                ),
        )?;
        assert!(fixture.0.path().join("laptop").exists());
        assert!(fixture.0.path().join(".closing").exists());

        // A late publication cannot leak from the closing session into its next enable.
        let history = fixture.run(&mut fixture.command(HistoryCommand::Read, 41000))?;
        assert_eq!(newest_use(&history)?, None);
        util::command_output(&mut fixture.command(
            HistoryCommand::Mark {
                host: "main",
                activity: &serde_json::json!({}),
            },
            41000,
        ))?;
        let history = fixture.run(&mut fixture.command(HistoryCommand::Read, 41000))?;
        assert_eq!(newest_use(&history)?.unwrap().1, "main");
        assert!(!fixture.0.path().join(".closing").exists());
        Ok(())
    }

    #[test]
    fn shutdown_rechecks_activity_and_the_expected_listener_under_lock() -> Result<()> {
        let fixture = HistoryFixture::new()?;
        fixture.expired_marker("main")?;
        let idle = Duration::from_secs(180 * 60);
        let shutdown = || HistoryCommand::Shutdown { idle, port: 40000 };
        let output = fixture.run(&mut fixture.command(shutdown(), 41000))?;
        assert_eq!(output, "active\n");
        // A port change without a closing session must preserve idle history.
        assert!(fixture.0.path().join("main").exists());
        util::command_output(&mut fixture.command(
            HistoryCommand::Mark {
                host: "laptop",
                activity: &serde_json::json!({}),
            },
            40000,
        ))?;
        let output = fixture.run(&mut fixture.command(shutdown(), 40000))?;
        assert_eq!(output, "active\n");
        assert!(!fixture.0.path().join(".closing").exists());
        assert!(!fixture.0.path().join(".calls").exists());
        Ok(())
    }

    #[test]
    fn locked_or_unreadable_history_is_an_error_not_an_empty_session() -> Result<()> {
        use std::os::fd::AsRawFd;
        let fixture = HistoryFixture::new()?;
        fixture.expired_marker("main")?;
        let lock = std::fs::File::create(fixture.0.path().join(".lock"))?;
        // SAFETY: this is a live file descriptor, owned until explicitly dropped.
        assert_eq!(
            unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        let output = fixture.command(HistoryCommand::Read, 40000).output()?;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("Cannot lock ADB idle history"));
        drop(lock);
        let output = fixture.command(HistoryCommand::Read, 0).output()?;
        assert!(!output.status.success());
        let shell = std::env::var_os("HOST_TOOLS_TEST_SHELL").unwrap_or_else(|| "/bin/sh".into());
        let output = Command::new(shell)
            .args([
                "-c",
                &format!(
                    "getprop() {{ echo 40000; }}; dumpsys() {{ :; }}; stat() {{ return 1; }}; {}",
                    fixture.script(HistoryCommand::Read),
                ),
            ])
            .output()?;
        assert!(!output.status.success());
        util::command_output(&mut fixture.command(HistoryCommand::Read, 40000))?;
        Ok(())
    }

    #[test]
    fn listener_port_sources_support_old_android_but_must_agree() -> Result<()> {
        let fixture = HistoryFixture::new()?;
        // Current ROM: service dump only. Older Android: property only.
        util::command_output(&mut fixture.command(HistoryCommand::Read, 40000))?;
        util::command_output(
            fixture
                .command(HistoryCommand::Read, 40000)
                .env("PHONE_PORT", "")
                .env("PROP_PORT", "40000"),
        )?;
        util::command_output(
            fixture
                .command(HistoryCommand::Read, 40000)
                .env("PROP_PORT", "40000"),
        )?;
        let conflict = fixture
            .command(HistoryCommand::Read, 40000)
            .env("PROP_PORT", "41000")
            .output()?;
        assert!(!conflict.status.success());
        assert!(
            String::from_utf8_lossy(&conflict.stderr)
                .contains("invalid Wireless debugging listener port")
        );
        let missing = fixture
            .command(HistoryCommand::Read, 40000)
            .env("PHONE_PORT", "")
            .output()?;
        assert!(!missing.status.success());
        assert!(!fixture.0.path().join(".closing").exists());
        assert!(!fixture.0.path().join(".calls").exists());
        Ok(())
    }

    #[test]
    fn phone_history_lock_is_released_when_its_shell_is_killed() -> Result<()> {
        use std::io::{BufRead, BufReader};
        let fixture = HistoryFixture::new()?;
        let shell = std::env::var_os("HOST_TOOLS_TEST_SHELL").unwrap_or_else(|| "/bin/sh".into());
        let mut child = Command::new(shell)
            .args([
                "-c",
                &format!(
                    "exec 9>{}/.lock; flock -nx 0 <&9 || exit 1; echo locked; read answer",
                    fixture.0.path().display(),
                ),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let mut ready = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut ready)?;
        assert_eq!(ready, "locked\n");
        let blocked = fixture.command(HistoryCommand::Read, 40000).output()?;
        child.kill()?;
        child.wait()?;
        assert!(!blocked.status.success());
        util::command_output(&mut fixture.command(HistoryCommand::Read, 40000))?;
        Ok(())
    }

    #[test]
    fn only_this_phones_mdns_transports_are_candidates() {
        let devices = "adb-phone-new._adb-tls-connect._tcp device\n\
                       adb-phone-new._adb-tls-pairing._tcp device\n\
                       adb-phone2-old._adb-tls-connect._tcp device\n\
                       USB123 device\n\
                       192.168.1.76:41325 device\n";
        assert_eq!(
            stale_transports(devices, "phone").collect::<Vec<_>>(),
            ["adb-phone-new._adb-tls-connect._tcp"]
        );
    }

    #[test]
    fn drops_the_transport_of_a_phone_that_stopped_advertising() {
        let devices = "adb-phone-new._adb-tls-connect._tcp device\n\
                       adb-other-new._adb-tls-connect._tcp device\n\
                       emulator-5554 device\n";
        let held = connection("adb-phone-new._adb-tls-connect._tcp", "192.168.1.76:41325");
        let address = after_grace(
            Some(&held),
            devices,
            &[
                ("host:mdns:services", "List of discovered mdns services\n"),
                (
                    "host:disconnect:adb-phone-new._adb-tls-connect._tcp",
                    "disconnected adb-phone-new._adb-tls-connect._tcp\n",
                ),
            ],
        )
        .unwrap();
        assert_eq!(address, None);
    }

    #[test]
    fn keeps_the_held_transport_while_mdns_is_just_quiet() {
        let devices = "adb-phone-new._adb-tls-connect._tcp device\n";
        let held = connection("adb-phone-new._adb-tls-connect._tcp", "192.168.1.76:41325");
        let address = from(
            Some(&held),
            devices,
            &[("host:mdns:services", "List of discovered mdns services\n")],
        )
        .unwrap();
        assert_eq!(address, Some(held));
    }

    #[test]
    fn keeps_an_unadvertised_transport_the_phone_still_answers_on() {
        let devices = "adb-phone-new._adb-tls-connect._tcp device\n";
        let held = connection("adb-phone-new._adb-tls-connect._tcp", "192.168.1.76:41325");
        let quiet = [("host:mdns:services", "List of discovered mdns services\n")];
        assert_eq!(
            reconcile_with(past_grace(), Some(&held), None, devices, &quiet, true).unwrap(),
            Some(held.clone())
        );
        // Without a listed transport there is nothing left to keep or to ask.
        assert_eq!(
            reconcile_with(past_grace(), Some(&held), None, "", &quiet, true).unwrap(),
            None
        );
    }

    #[test]
    fn adopts_the_remembered_transport_of_an_unadvertised_phone() {
        let devices = "adb-phone-new._adb-tls-connect._tcp device\n";
        let known = connection("adb-phone-new._adb-tls-connect._tcp", "192.168.1.76:41325");
        let quiet = [("host:mdns:services", "List of discovered mdns services\n")];
        let adopt = |devices, answering| {
            reconcile_with(None, None, Some(&known), devices, &quiet, answering).unwrap()
        };
        assert_eq!(adopt(devices, true), Some(known.clone()));
        // A transport that is gone, or one the phone no longer answers on at the
        // remembered address, is neither adopted nor disconnected.
        assert_eq!(adopt("", true), None);
        assert_eq!(adopt(devices, false), None);
        // What the phone advertises always wins over the note.
        let moved = connection("adb-phone-new._adb-tls-connect._tcp", "192.168.1.76:5555");
        assert_eq!(
            reconcile_with(
                None,
                None,
                Some(&moved),
                devices,
                &[("host:mdns:services", DISCOVERY)],
                false
            )
            .unwrap(),
            Some(known)
        );
    }

    #[test]
    fn a_restarted_watcher_recalls_the_transport_it_held() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let memory = directory.path().join("phone");
        let held = connection("adb-phone-new._adb-tls-connect._tcp", "192.168.1.76:41325");
        let mut phone = Phone::new("phone").recalling(memory.clone());
        assert_eq!(phone.remembered, None);
        phone.remember(&held);
        assert_eq!(
            Phone::new("phone").recalling(memory.clone()).remembered,
            Some(held)
        );
        // A note that names no address is ignored, not guessed at.
        fs::write(&memory, "adb-phone-new._adb-tls-connect._tcp\n")?;
        assert_eq!(Phone::new("phone").recalling(memory).remembered, None);
        Ok(())
    }

    #[test]
    fn reads_the_phones_latest_acknowledgement() {
        let text = "ESTAB 0 0 192.168.1.2:50000 192.168.1.76:41325 timer:(keepalive,109ms,0)\n\
                    \t cubic bytes_sent:1665 bytes_received:3979 lastsnd:2005554 lastrcv:2005549 lastack:891 minrtt:4.6\n\
                    ESTAB 0 0 192.168.1.2:50001 192.168.1.77:41325\n\
                    \t cubic bytes_sent:7 bytes_received:9\n";
        assert_eq!(
            last_ack_for_peer(text, "192.168.1.76:41325"),
            Some(Duration::from_millis(891))
        );
        // A socket that is gone or reports no acknowledgement is not answering.
        assert_eq!(last_ack_for_peer(text, "192.168.1.77:41325"), None);
        assert_eq!(last_ack_for_peer(text, "192.168.1.78:41325"), None);
    }

    #[test]
    fn notices_a_connected_phone_it_cannot_watch() {
        let devices = "adb-phone-new._adb-tls-connect._tcp device\n\
                       adb-phone2-new._adb-tls-connect._tcp device\n\
                       192.168.1.76:41325 device\n";
        assert!(online_unadvertised(devices, "phone"));
        assert!(!online_unadvertised(devices, "other"));
        assert!(!online_unadvertised(
            "adb-phone-new._adb-tls-connect._tcp offline\n",
            "phone"
        ));
    }

    #[test]
    fn drops_an_advertised_transport_that_changed_its_instance_name() {
        cycle(
            "adb-phone-old._adb-tls-connect._tcp device\n",
            &[
                ("host:mdns:services", DISCOVERY),
                (
                    "host:disconnect:adb-phone-old._adb-tls-connect._tcp",
                    "disconnected adb-phone-old._adb-tls-connect._tcp\n",
                ),
                (
                    "host:connect:192.168.1.76:41325",
                    "connected to 192.168.1.76:41325\n",
                ),
                ("host:devices", "192.168.1.76:41325 device\n"),
            ],
        )
        .unwrap();
    }

    #[test]
    fn forgets_the_address_transport_once_the_phone_stops_advertising() {
        let held = connection("192.168.1.76:41325", "192.168.1.76:41325");
        let address = after_grace(
            Some(&held),
            "192.168.1.76:41325 device\n",
            &[
                ("host:mdns:services", "List of discovered mdns services\n"),
                (
                    "host:disconnect:192.168.1.76:41325",
                    "disconnected 192.168.1.76:41325\n",
                ),
            ],
        )
        .unwrap();
        assert_eq!(address, None);
    }

    #[test]
    fn forgets_an_address_transport_left_on_the_previous_port() {
        let held = connection("192.168.1.76:30000", "192.168.1.76:30000");
        let address = from(
            Some(&held),
            "192.168.1.76:30000 device\n",
            &[
                ("host:mdns:services", DISCOVERY),
                (
                    "host:disconnect:192.168.1.76:30000",
                    "disconnected 192.168.1.76:30000\n",
                ),
                (
                    "host:connect:192.168.1.76:41325",
                    "connected to 192.168.1.76:41325\n",
                ),
                ("host:devices", "192.168.1.76:41325 device\n"),
            ],
        )
        .unwrap();
        assert_eq!(
            address
                .as_ref()
                .map(|connection| connection.address.as_str()),
            Some("192.168.1.76:41325")
        );
    }

    #[test]
    fn tolerates_a_transport_that_vanished_before_the_disconnect() {
        let held = connection("192.168.1.76:41325", "192.168.1.76:41325");
        let mut missing_since = Instant::now().checked_sub(ABSENT_GRACE + Duration::from_secs(1));
        let mut query = |request: &str| -> Result<String> {
            Ok(match request {
                "host:mdns:services" => "List of discovered mdns services\n".to_owned(),
                _ => bail!("no such device '192.168.1.76:41325'"),
            })
        };
        assert_eq!(
            reconcile(
                "phone",
                "192.168.1.76:41325 device\n",
                Some(&held),
                Some(&held),
                &mut missing_since,
                &mut query,
                |_| false,
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn treats_a_vanished_mdns_name_as_already_disconnected() {
        let mut query = |request: &str| -> Result<String> {
            assert_eq!(
                request,
                "host:disconnect:adb-phone-new._adb-tls-connect._tcp"
            );
            bail!("couldn't parse 'adb-phone-new._adb-tls-connect._tcp'")
        };
        disconnect(&mut query, "adb-phone-new._adb-tls-connect._tcp").unwrap();
    }

    #[test]
    fn absent_phone_is_normal_but_invalid_endpoint_is_an_error() {
        let address = cycle(
            "",
            &[("host:mdns:services", "List of discovered mdns services\n")],
        )
        .unwrap();
        assert_eq!(address, None);
        for endpoint in ["", "invalid", "192.168.1.76:0", "192.168.1.76:41325 extra"] {
            assert!(
                discover(&format!("adb-phone-new {SERVICE_TYPE} {endpoint}"), "phone").is_err()
            );
        }
        assert_eq!(
            discover(
                "adb-phone-new _adb-tls-connect._tcp. [fd00::1]:40000",
                "phone"
            )
            .unwrap()[0]
                .address,
            "[fd00::1]:40000"
        );
    }

    #[test]
    fn frames_handle_fragmentation_empty_updates_and_invalid_payloads() {
        let mut fragmented = b"00".as_slice().chain(b"05hello0000".as_slice());
        assert_eq!(read_frame(&mut fragmented).unwrap(), "hello");
        assert_eq!(read_frame(&mut fragmented).unwrap(), "");
        for mut bytes in [b"zzzz".as_slice(), b"+001x", b"0002x", b"0001\xff", b"00"] {
            assert!(read_frame(&mut bytes).is_err());
        }
    }

    #[test]
    fn smart_socket_queries_notifications_and_server_exit() -> Result<()> {
        use std::{net::TcpListener, sync::mpsc};

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let client = AdbServer(listener.local_addr()?);
        let (advance, receiver) = mpsc::channel();
        let server = thread::spawn(move || -> Result<()> {
            for (request, response) in [
                ("host:mdns:services", "OKAY0005phone"),
                ("host:devices", "OKAY0000"),
                ("host:connect:192.168.1.76:41325", "FAIL0006denied"),
            ] {
                let (mut socket, _) = listener.accept()?;
                socket.set_read_timeout(Some(Duration::from_secs(2)))?;
                assert_eq!(read_frame(&mut socket)?, request);
                socket.write_all(response.as_bytes())?;
            }
            let (mut socket, _) = listener.accept()?;
            socket.set_read_timeout(Some(Duration::from_secs(2)))?;
            assert_eq!(read_frame(&mut socket)?, "host:track-devices");
            socket.write_all(b"OKAY000dphone device\n")?;
            receiver.recv_timeout(Duration::from_secs(2))?;
            socket.write_all(b"000ephone offline\n0000")?;
            receiver.recv_timeout(Duration::from_secs(2))?;
            Ok(()) // Closing the socket must wake the watcher without a final frame.
        });

        assert_eq!(client.query("host:mdns:services")?, "phone");
        assert_eq!(client.query("host:devices")?, "");
        assert!(
            client
                .query("host:connect:192.168.1.76:41325")
                .unwrap_err()
                .to_string()
                .contains("denied")
        );
        let mut tracker = client.request("host:track-devices")?;
        let mut devices = read_frame(&mut tracker)?;
        assert_eq!(devices, "phone device\n");
        update_devices(&mut tracker, &mut devices, 0)?;
        assert_eq!(devices, "phone device\n");
        advance.send(())?;
        update_devices(&mut tracker, &mut devices, -1)?;
        assert_eq!(devices, "");
        advance.send(())?;
        assert!(update_devices(&mut tracker, &mut devices, -1).is_err());
        server.join().unwrap()?;
        Ok(())
    }
}
