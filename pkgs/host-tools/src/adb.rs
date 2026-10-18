//! Retry missed ADB mDNS connections without retaining an obsolete IP or port.

use crate::util;
use anyhow::{Context, Result, bail, ensure};
use std::{
    io::{self, Read, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream},
    os::fd::AsRawFd,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

const SERVICE_TYPE: &str = "_adb-tls-connect._tcp";
const RETRY_INTERVAL: Duration = Duration::from_secs(2);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const SERVER: AdbServer = AdbServer(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 5037)));

pub fn run(adb: &str, serial: &str, once: bool) -> Result<()> {
    ensure!(
        !serial.is_empty()
            && serial
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "expected an Android serial number, not an IP address or mDNS service name"
    );
    // Spawn the SDK client only to start/check the server, never for routine queries.
    // `-H`/`-P` make adb treat the server as remote: it then refuses to spawn one
    // ("cannot start server on remote host") and can only connect when one already
    // runs. Start the default local socket instead, clearing anything that could
    // redirect it, and let adb wait for the daemon it spawns.
    util::command_output(
        Command::new("timeout")
            .args(["--foreground", "--kill-after=2s", "10s", adb])
            .arg("start-server")
            .env("ADB_MDNS_AUTO_CONNECT", "adb-tls-connect")
            .env_remove("ADB_SERVER_SOCKET")
            .env_remove("ANDROID_ADB_SERVER_ADDRESS")
            .env_remove("ANDROID_ADB_SERVER_PORT")
            .stdin(Stdio::null()),
    )
    .context("start the local ADB server")?;
    // Subscribe before querying, so a disconnect during reconciliation is not missed.
    // A broken subscription exits; systemd owns restart and backoff.
    let mut tracker = SERVER.request("host:track-devices")?;
    let mut devices = read_frame(&mut tracker)?;
    let mut previous = String::new();
    loop {
        let result = reconcile(serial, &devices, |service| SERVER.query(service));
        let connected = matches!(result, Ok(Some(_)));
        let message = match result {
            Ok(Some(address)) => format!("{serial} connected at {address}"),
            Ok(None) => format!("waiting for {serial} to advertise Wireless debugging on the LAN"),
            Err(error) if once => return Err(error),
            Err(error) => format!("{error:#}; retrying in 2s"),
        };
        if once {
            println!("{message}");
            return Ok(());
        }
        if message != previous {
            eprintln!("adb-watch: {message}");
            previous = message;
        }
        if !connected {
            // Rate-limit failures even when our own attempts generate device events.
            thread::sleep(RETRY_INTERVAL);
        }
        update_devices(&mut tracker, &mut devices, if connected { -1 } else { 0 })?;
    }
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

fn reconcile(
    serial: &str,
    devices: &str,
    mut query: impl FnMut(&str) -> Result<String>,
) -> Result<Option<String>> {
    let services = discover(&query("host:mdns:services")?, serial)?;
    if services.is_empty() {
        return Ok(None);
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
        if matching_devices(devices, service).any(|(_, state)| state == "device") {
            return Ok(Some(service.address.clone()));
        }
    }
    let mut errors = Vec::new();
    for service in services {
        let attempt = (|| -> Result<()> {
            // Clear only this phone's stale transport. USB and other phones are untouched.
            for (name, _) in matching_devices(devices, &service) {
                query(&format!("host:disconnect:{name}"))?;
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
            ensure!(
                matching_devices(&devices, &service).any(|(_, state)| state == "device"),
                "{} is not online yet (check pairing if this persists)",
                service.address
            );
            Ok(())
        })();
        match attempt {
            Ok(()) => return Ok(Some(service.address)),
            Err(error) => errors.push(format!("{}: {error:#}", service.address)),
        }
    }
    bail!("{}", errors.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISCOVERY: &str = "List of discovered mdns services\nadb-phone-new _adb-tls-connect._tcp 192.168.1.76:41325\nadb-phone-new _adb-tls-pairing._tcp 192.168.1.76:39999\nadb-other-new _adb-tls-connect._tcp 192.168.1.77:44444\nadb-phone2-new _adb-tls-connect._tcp 192.168.1.78:44444\n";

    fn cycle(devices: &str, steps: &[(&str, &str)]) -> Result<Option<String>> {
        let mut steps = steps.iter();
        let result = reconcile("phone", devices, |service| {
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
        assert_eq!(address.as_deref(), Some("192.168.1.76:41325"));
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
        assert_eq!(address.as_deref(), Some("192.168.1.76:41325"));

        let address = cycle(
            "192.168.1.76:41325 offline\n",
            &[
                ("host:mdns:services", DISCOVERY),
                ("host:devices", "192.168.1.76:41325 device\n"),
            ],
        )
        .unwrap();
        assert_eq!(address.as_deref(), Some("192.168.1.76:41325"));
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
