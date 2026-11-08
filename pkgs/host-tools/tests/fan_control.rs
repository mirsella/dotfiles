use dbus::{
    Message,
    arg::{PropMap, Variant},
    blocking::LocalConnection,
    channel::{MatchingReceiver, Sender},
    message::MatchRule,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    fs,
    io::{BufRead, BufReader},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        linux::net::SocketAddrExt,
        unix::{
            ffi::OsStrExt,
            net::{SocketAddr, UnixDatagram},
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    rc::Rc,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const BIN: &str = env!("CARGO_BIN_EXE_host-tools");
const SERVICE: &str = "org.freedesktop.UPower.PowerProfiles";
const OBJECT: &str = "/org/freedesktop/UPower/PowerProfiles";
const FIRMWARE: [(&str, &str); 3] = [
    ("temp_sel", "8"),
    ("auto_point1_pwm", "153"),
    ("auto_point1_temp", "30000"),
];

struct Worker(Child);
impl Worker {
    fn exited(&mut self) -> ExitStatus {
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, self.0.id(), 0) } as i32;
        assert!(fd >= 0, "open child pidfd");
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let mut event = libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        assert_eq!(
            unsafe { libc::poll(&mut event, 1, 10000) },
            1,
            "child did not exit"
        );
        self.0.wait().unwrap()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if self.0.try_wait().unwrap().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
enum Event {
    Profile(String),
    Resume,
    Stop,
}
struct Bus {
    events: mpsc::Sender<Event>,
    worker: Option<thread::JoinHandle<()>>,
    address: String,
    _bus: Worker,
    _config: tempfile::NamedTempFile,
}
impl Bus {
    fn new() -> Self {
        let binary = option_env!("HOST_TOOLS_TEST_DBUS_DAEMON").unwrap_or("/usr/bin/dbus-daemon");
        let config = tempfile::NamedTempFile::new().unwrap();
        fs::write(
            config.path(),
            r#"<busconfig>
          <type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth>
          <policy context="default">
            <allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/>
          </policy>
        </busconfig>"#,
        )
        .unwrap();
        let mut bus = Worker(
            Command::new(binary)
                .args(["--nofork", "--print-address=1", "--config-file"])
                .arg(config.path())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let mut address = String::new();
        BufReader::new(bus.0.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        let address = address.trim().to_owned();
        assert!(
            !address.is_empty(),
            "Private D-Bus daemon did not publish an address"
        );
        let remote = address.clone();
        let (events, commands) = mpsc::channel();
        let (ready, started) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let connection = LocalConnection::new_address(&remote).unwrap();
            connection
                .request_name(SERVICE, false, false, true)
                .unwrap();
            connection
                .request_name("org.freedesktop.login1", false, false, true)
                .unwrap();
            let profile = Rc::new(RefCell::new("performance".to_owned()));
            let current = profile.clone();
            connection.start_receive(
                MatchRule::new_method_call().with_path(OBJECT),
                Box::new(move |message, connection| {
                    if message.member().is_some_and(|name| &*name == "Get") {
                        let (interface, property): (String, String) = message.read2().unwrap();
                        assert_eq!(interface, SERVICE);
                        assert_eq!(property, "ActiveProfile");
                        connection
                            .send(
                                message
                                    .method_return()
                                    .append1(Variant(current.borrow().clone())),
                            )
                            .unwrap();
                    }
                    true
                }),
            );
            ready.send(()).unwrap();
            loop {
                while let Ok(event) = commands.try_recv() {
                    match event {
                        Event::Stop => return,
                        Event::Profile(value) => {
                            *profile.borrow_mut() = value.clone();
                            let mut properties = PropMap::new();
                            properties.insert("ActiveProfile".into(), Variant(Box::new(value)));
                            connection
                                .send(
                                    Message::new_signal(
                                        OBJECT,
                                        "org.freedesktop.DBus.Properties",
                                        "PropertiesChanged",
                                    )
                                    .unwrap()
                                    .append3(
                                        SERVICE.to_owned(),
                                        properties,
                                        Vec::<String>::new(),
                                    ),
                                )
                                .unwrap();
                        }
                        Event::Resume => {
                            connection
                                .send(
                                    Message::new_signal(
                                        "/org/freedesktop/login1",
                                        "org.freedesktop.login1.Manager",
                                        "PrepareForSleep",
                                    )
                                    .unwrap()
                                    .append1(false),
                                )
                                .unwrap();
                        }
                    }
                }
                connection.process(Duration::from_millis(20)).unwrap();
            }
        });
        started.recv_timeout(Duration::from_secs(3)).unwrap();
        Self {
            events,
            worker: Some(worker),
            address,
            _bus: bus,
            _config: config,
        }
    }
}
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.events.send(Event::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn put(path: impl AsRef<Path>, value: impl AsRef<[u8]>) {
    let path = path.as_ref();
    let next = path.with_extension("next");
    fs::write(&next, value).unwrap();
    fs::rename(next, path).unwrap();
}
struct Rig {
    _root: tempfile::TempDir,
    inventory: PathBuf,
    fans: PathBuf,
    cpu: PathBuf,
    gpu: PathBuf,
    config: PathBuf,
    status: PathBuf,
    notify_path: PathBuf,
    notifications: UnixDatagram,
}
impl Rig {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let path = root.path();
        let inventory = path.join("hwmon");
        fs::create_dir(&inventory).unwrap();
        let fans = inventory.join("fans");
        let cpu = inventory.join("cpu");
        let gpu = inventory.join("gpu");
        for (directory, name) in [(&fans, "nct6798"), (&cpu, "zenpower"), (&gpu, "amdgpu")] {
            fs::create_dir(directory).unwrap();
            put(directory.join("name"), name);
        }
        for channel in 1..=4 {
            put(fans.join(format!("pwm{channel}")), "0");
            put(fans.join(format!("pwm{channel}_enable")), "5");
            put(fans.join(format!("fan{channel}_input")), "600");
            for (suffix, value) in FIRMWARE {
                put(fans.join(format!("pwm{channel}_{suffix}")), value);
            }
        }
        put(cpu.join("temp1_label"), "Tdie");
        put(cpu.join("temp1_input"), "55000");
        put(gpu.join("temp1_label"), "edge");
        put(gpu.join("temp1_input"), "45000");
        let config = path.join("config.json");
        put(&config, include_bytes!("fixtures/fan-control.json"));
        let status = path.join("status.json");
        let notify_path = path.join("notify");
        let notifications = UnixDatagram::bind(&notify_path).unwrap();
        notifications
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        Self {
            inventory,
            fans,
            cpu,
            gpu,
            config,
            status,
            notify_path,
            notifications,
            _root: root,
        }
    }
    fn command(&self, operation: &str) -> Command {
        let mut command = Command::new(BIN);
        command
            .args([
                "fan-control",
                operation,
                "--controller",
                "nct6798",
                "--channels",
                "1,2,3,4",
                "--hwmon-root",
            ])
            .arg(&self.inventory);
        command
    }
    fn start(&self, bus: &Bus) -> Worker {
        let mut command = self.command("run");
        command
            .arg(&self.config)
            .arg("--status")
            .arg(&self.status)
            .env("DBUS_SYSTEM_BUS_ADDRESS", &bus.address)
            .env("NOTIFY_SOCKET", &self.notify_path)
            .env("WATCHDOG_USEC", "10000000")
            .env_remove("WATCHDOG_PID");
        Worker(command.spawn().unwrap())
    }
    fn receive_until(&self, mut done: impl FnMut(&str) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            assert!(
                Instant::now() < deadline,
                "fan-controller notification timed out"
            );
            let mut buffer = [0; 1024];
            match self.notifications.recv(&mut buffer) {
                Ok(length) => {
                    if done(std::str::from_utf8(&buffer[..length]).unwrap()) {
                        return;
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(error) => panic!("notification: {error}"),
            }
        }
    }
    fn notification(&self, expected: &str) {
        self.receive_until(|message| message == expected);
    }
    fn state(&self) -> Value {
        serde_json::from_slice(&fs::read(&self.status).unwrap()).unwrap()
    }
    fn state_until(&self, mut done: impl FnMut(&Value) -> bool) {
        self.receive_until(|_| done(&self.state()));
    }
    fn modes(&self, expected: &str) {
        for channel in 1..=4 {
            assert_eq!(
                fs::read_to_string(self.fans.join(format!("pwm{channel}_enable")))
                    .unwrap()
                    .trim(),
                expected
            );
        }
    }
}

#[test]
fn daemon_supports_abstract_systemd_notification_sockets() {
    let bus = Bus::new();
    let mut rig = Rig::new();
    let address = SocketAddr::from_abstract_name(rig.notify_path.as_os_str().as_bytes()).unwrap();
    rig.notifications = UnixDatagram::bind_addr(&address).unwrap();
    rig.notifications
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut path = std::ffi::OsString::from("@");
    path.push(&rig.notify_path);
    rig.notify_path = path.into();
    let mut child = rig.start(&bus);
    rig.notification("READY=1");
    rig.notification("WATCHDOG=1");
    assert_eq!(unsafe { libc::kill(child.0.id() as i32, libc::SIGHUP) }, 0);
    rig.receive_until(|message| message.starts_with("RELOADING=1\nMONOTONIC_USEC="));
    rig.notification("READY=1");
    assert_eq!(unsafe { libc::kill(child.0.id() as i32, libc::SIGTERM) }, 0);
    assert!(child.exited().success());
    rig.modes("5");
}

#[test]
fn daemon_handles_profiles_reload_resume_emergencies_and_sensor_failure() {
    let bus = Bus::new();
    let mut rig = Rig::new();
    let mut child = rig.start(&bus);
    rig.notification("READY=1");
    assert_eq!(rig.state()["pwm"], json!([77, 0, 77, 77]));
    put(rig.gpu.join("temp1_input"), "65000");
    rig.state_until(|state| state["pwm"] == json!([166, 0, 166, 166]));
    put(rig.gpu.join("temp1_input"), "80000");
    rig.state_until(|state| state["gpu_temperature"] == 80000);
    assert_eq!(rig.state()["pwm"], json!([255, 0, 255, 255]));
    put(rig.gpu.join("temp1_input"), "45000");
    rig.state_until(|state| state["pwm"] == json!([77, 0, 77, 77]));
    for profile in ["balanced", "power-saver", "performance", "balanced"] {
        bus.events.send(Event::Profile(profile.into())).unwrap();
        rig.state_until(|state| state["profile"] == profile);
        assert_eq!(rig.state()["pwm"], json!([77, 0, 77, 77]));
        rig.modes("1");
    }
    let mut config: Value = serde_json::from_slice(&fs::read(&rig.config).unwrap()).unwrap();
    config["case_fans"]["minimum_duty"] = json!(40);
    put(&rig.config, config.to_string());
    assert_eq!(unsafe { libc::kill(child.0.id() as i32, libc::SIGHUP) }, 0);
    rig.receive_until(|message| message.starts_with("RELOADING=1\nMONOTONIC_USEC="));
    rig.notification("READY=1");
    assert_eq!(rig.state()["pwm"], json!([102, 0, 102, 102]));
    // Resume changes hwmon enumeration and resets controller modes.
    for directory in [&mut rig.fans, &mut rig.cpu, &mut rig.gpu] {
        let next = directory.with_extension("resumed");
        fs::rename(&*directory, &next).unwrap();
        *directory = next;
    }
    for channel in 1..=4 {
        put(rig.fans.join(format!("pwm{channel}_enable")), "5");
    }
    bus.events.send(Event::Resume).unwrap();
    rig.state_until(|_| {
        fs::read_to_string(rig.fans.join("pwm1_enable"))
            .unwrap()
            .trim()
            == "1"
    });
    rig.modes("1");
    fs::remove_file(rig.gpu.join("temp1_input")).unwrap();
    assert!(!child.exited().success());
    rig.modes("5");
}

#[test]
fn signal_stop_stall_and_invalid_start_all_restore_automatic_control() {
    let bus = Bus::new();
    let rig = Rig::new();
    let mut child = rig.start(&bus);
    rig.notification("READY=1");
    assert_eq!(unsafe { libc::kill(child.0.id() as i32, libc::SIGTERM) }, 0);
    assert!(child.exited().success());
    rig.modes("5");
    let original = fs::read(&rig.config).unwrap();
    let mut child = rig.start(&bus);
    rig.notification("READY=1");
    put(&rig.config, "invalid-json");
    assert_eq!(unsafe { libc::kill(child.0.id() as i32, libc::SIGHUP) }, 0);
    assert!(!child.exited().success());
    rig.modes("5");
    put(&rig.config, original);
    let mut child = rig.start(&bus);
    rig.notification("READY=1");
    put(rig.fans.join("fan1_input"), "0");
    assert!(!child.exited().success());
    rig.modes("5");
    put(&rig.config, "invalid-json");
    let mut child = rig.start(&bus);
    assert!(!child.exited().success());
    rig.modes("5");
}

#[test]
fn losing_the_profile_provider_restores_automatic_control() {
    let bus = Bus::new();
    let rig = Rig::new();
    let mut child = rig.start(&bus);
    rig.notification("READY=1");
    bus.events.send(Event::Stop).unwrap();
    assert!(!child.exited().success());
    rig.modes("5");
}

#[test]
fn independent_hook_recovers_a_killed_daemon_without_curve_sensors_or_dbus() {
    let bus = Bus::new();
    let rig = Rig::new();
    let mut child = rig.start(&bus);
    rig.notification("READY=1");
    // SIGKILL bypasses the Rust recovery guard, as with the systemd watchdog.
    child.0.kill().unwrap();
    assert!(!child.exited().success());
    rig.modes("1");
    drop(bus);
    put(&rig.config, "invalid-json");
    fs::remove_file(rig.cpu.join("temp1_input")).unwrap();
    fs::remove_file(rig.gpu.join("temp1_input")).unwrap();
    assert!(rig.command("automatic").status().unwrap().success());
    rig.modes("5");
    // Handover must preserve the firmware's sensor selection and curve.
    for channel in 1..=4 {
        for (suffix, expected) in FIRMWARE {
            assert_eq!(
                fs::read_to_string(rig.fans.join(format!("pwm{channel}_{suffix}"))).unwrap(),
                expected
            );
        }
    }
}
