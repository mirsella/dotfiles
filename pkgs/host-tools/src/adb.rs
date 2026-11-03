//! Retry missed ADB mDNS connections without retaining an obsolete IP or port,
//! drop a transport once its phone stops advertising it, and turn Wireless
//! debugging off once a phone has been idle for a while.

use crate::util;
use anyhow::{Context, Result, anyhow, bail, ensure};
use std::{
    collections::HashSet,
    io::{self, Read, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream},
    os::fd::AsRawFd,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
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
/// How often the idle window reads the device socket's counters. This bounds how
/// stale the last-traffic estimate can be without polling anything every minute.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(600);
const SERVER: AdbServer = AdbServer(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 5037)));

pub fn run(adb: &str, serials: &[String], once: bool, idle: Option<Duration>) -> Result<()> {
    ensure!(
        !serials.is_empty(),
        "expected at least one Android serial number"
    );
    let mut unique = HashSet::new();
    for serial in serials {
        ensure!(
            !serial.is_empty()
                && serial
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-'),
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
    // Subscribe before querying, so a disconnect during reconciliation is not missed.
    // A broken subscription exits; systemd owns restart and backoff.
    let mut tracker = SERVER.request("host:track-devices")?;
    let mut devices = read_frame(&mut tracker)?;
    let mut phones = serials
        .iter()
        .map(|serial| Phone::new(serial))
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
        if let Some(idle) = idle {
            for phone in &mut phones {
                phone.enforce_idle(adb, idle);
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
    last_message: String,
    /// Set while the phone advertises nothing; see `ABSENT_GRACE`.
    missing_since: Option<Instant>,
    /// The idle window opens whenever the connection changes, and `baseline`
    /// holds the device socket's payload bytes then, which only real ADB use changes.
    idle_since: Instant,
    sampled: Instant,
    baseline: Option<u64>,
    unreadable: bool,
}

impl Phone {
    fn new(serial: &str) -> Self {
        let now = Instant::now();
        Self {
            serial: serial.to_owned(),
            connection: None,
            last_message: String::new(),
            missing_since: None,
            idle_since: now,
            sampled: now,
            baseline: None,
            unreadable: false,
        }
    }

    /// Bring the held transport in line with what this phone advertises, logging
    /// changes when `announce` is set, and report whether it is online.
    fn sync(&mut self, devices: &str, announce: bool) -> Result<bool> {
        let result = reconcile(
            &self.serial,
            devices,
            self.connection.as_ref(),
            &mut self.missing_since,
            |service| SERVER.query(service),
        );
        let message = match &result {
            Ok(Some(connection)) => format!("{} connected at {}", self.serial, connection.address),
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
            self.reset_idle();
        }
        Ok(connected)
    }

    /// Restart the idle window after the held transport changes.
    fn reset_idle(&mut self) {
        let now = Instant::now();
        self.idle_since = now;
        self.sampled = now;
        self.unreadable = false;
        self.baseline = self
            .connection
            .as_ref()
            .and_then(|connection| transport_bytes(&connection.address));
    }

    /// Turn Wireless debugging off after `idle` without device traffic.
    fn enforce_idle(&mut self, adb: &str, idle: Duration) {
        let Some(connection) = &self.connection else {
            return;
        };
        // Only payload counts: keepalive probes are empty segments, so the
        // counters of an idle phone stay put even while ADB keeps probing it.
        if self.sampled.elapsed() >= SAMPLE_INTERVAL {
            self.sampled = Instant::now();
            match transport_bytes(&connection.address) {
                Some(bytes) => {
                    self.unreadable = false;
                    if self.baseline != Some(bytes) {
                        self.idle_since = Instant::now();
                    }
                    self.baseline = Some(bytes);
                }
                None if !self.unreadable => {
                    self.unreadable = true;
                    eprintln!(
                        "adb-watch: cannot read the device socket to {}; not treating it as idle",
                        connection.address
                    );
                }
                None => {}
            }
        }
        if self.idle_since.elapsed() >= idle {
            self.idle_since = Instant::now();
            // The phone only pays for this transport, so stop holding it until
            // it is used again; enabling Wireless debugging is a phone-side tap.
            match disable_wireless(adb, &connection.serial) {
                Ok(()) => eprintln!(
                    "adb-watch: turned Wireless debugging off on {} after {} minutes without ADB activity",
                    self.serial,
                    idle.as_secs() / 60
                ),
                Err(error) => eprintln!("adb-watch: {error:#}"),
            }
        }
    }
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

/// Payload bytes moved on the device socket to `address`, from `ss` TCP info.
/// Keepalive probes carry no payload, so unchanged bytes mean no ADB use.
fn transport_bytes(address: &str) -> Option<u64> {
    let output = Command::new("ss").args(["-tnHi"]).output().ok()?;
    bytes_for_peer(&String::from_utf8_lossy(&output.stdout), address)
}

fn bytes_for_peer(text: &str, address: &str) -> Option<u64> {
    let wanted = address.replace(['[', ']'], "");
    let mut matched = false;
    for line in text.lines() {
        if !line.starts_with('\t') {
            matched = line
                .split_whitespace()
                .nth(4)
                .is_some_and(|peer| peer.replace(['[', ']'], "") == wanted);
            continue;
        }
        if !matched {
            continue;
        }
        // Both counters together are the whole payload of the device socket.
        let field = |name: &str| -> Option<u64> {
            line.split_whitespace()
                .find_map(|field| Some(field.strip_prefix(name)?.parse().ok()?))
        };
        return Some(field("bytes_sent:")? + field("bytes_received:")?);
    }
    None
}

/// Turn Wireless debugging off on the phone; only the phone can turn it back on.
fn disable_wireless(adb: &str, serial: &str) -> Result<()> {
    util::command_output(client(adb).args([
        "-s",
        serial,
        "shell",
        "settings",
        "put",
        "global",
        "adb_wifi_enabled",
        "0",
    ]))
    .with_context(|| format!("turn Wireless debugging off on {serial}"))
    .map(|_| ())
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

/// Bring the phone's transport in line with what it advertises: connect when it
/// is missing or stale, drop what nothing advertises, and return the transport
/// the watcher now holds.
fn reconcile(
    serial: &str,
    devices: &str,
    held: Option<&Connection>,
    missing_since: &mut Option<Instant>,
    mut query: impl FnMut(&str) -> Result<String>,
) -> Result<Option<Connection>> {
    let services = discover(&query("host:mdns:services")?, serial)?;
    if services.is_empty() {
        // Cache expiry and a Wi-Fi roam look exactly like a phone that left, so a
        // quiet mDNS list alone must not cut the transport: wait for the grace
        // period, during which the transport is reported as still held.
        let since = missing_since.get_or_insert_with(Instant::now);
        if let Some(held) = held.filter(|held| listed(devices, &held.serial)) {
            if since.elapsed() < ABSENT_GRACE {
                return Ok(Some(held.clone()));
            }
            disconnect(&mut query, &held.serial)?;
        }
        return Ok(None);
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
        reconcile_with(None, held, devices, steps)
    }

    /// Run one reconciliation as if the phone had been absent past the grace period.
    fn after_grace(
        held: Option<&Connection>,
        devices: &str,
        steps: &[(&str, &str)],
    ) -> Result<Option<Connection>> {
        let since = Instant::now().checked_sub(ABSENT_GRACE + Duration::from_secs(1));
        reconcile_with(since, held, devices, steps)
    }

    fn reconcile_with(
        missing_since: Option<Instant>,
        held: Option<&Connection>,
        devices: &str,
        steps: &[(&str, &str)],
    ) -> Result<Option<Connection>> {
        let mut missing_since = missing_since;
        let mut steps = steps.iter();
        let result = reconcile("phone", devices, held, &mut missing_since, |service| {
            let &(expected, response) = steps.next().expect("unexpected ADB request");
            assert_eq!(service, expected);
            Ok(response.to_owned())
        });
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
        assert_eq!(bytes_for_peer(text, "192.168.1.76:41325"), Some(1540));
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
            Some(11)
        );
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
                &mut missing_since,
                &mut query
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
