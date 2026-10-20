use crate::util;
use anyhow::{Context, Result, bail, ensure};
use chrono::Local;
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    net::{IpAddr, UdpSocket},
    os::unix::fs::{FileExt, OpenOptionsExt},
    path::Path,
    process::{Command, Stdio},
};

mod ryzen;
pub use ryzen::ryzenadj;

pub fn lid() -> Result<()> {
    let text = util::read("/proc/acpi/button/lid/LID0/state")?;
    let state = text
        .split_whitespace()
        .nth(1)
        .context("missing lid state")?;
    let (blank, fb) = match state {
        "open" => ("poke", "0\n"),
        "closed" => ("force", "4\n"),
        _ => bail!("Unexpected lid state: {state}"),
    };
    let tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty0")?;
    let status = Command::new("setterm")
        .args(["--term", "linux", "--blank", blank])
        .stdin(tty.try_clone()?)
        .stdout(tty)
        .status()?;
    ensure!(status.success(), "setterm exited {status}");
    fs::write("/sys/class/graphics/fb0/blank", fb)?;
    println!(
        "Lid {state}: console blank {blank}, framebuffer blank {}",
        fb.trim()
    );
    Ok(())
}

pub fn keyboard_off() -> Result<()> {
    let ec = OpenOptions::new()
        .write(true)
        .open("/sys/kernel/debug/ec/ec0/io")?;
    for offset in [48, 49] {
        ensure!(
            ec.write_at(&[0], offset)? == 1,
            "short EC write at {offset}"
        );
    }
    Ok(())
}

const CARD: &str = "bluez_card.54_B7_E5_C2_75_28";
pub fn audio_profile(cards: &Value) -> Result<Option<&'static str>> {
    for card in cards.as_array().context("invalid pactl card list")? {
        if card["name"] != CARD {
            continue;
        }
        let active = card["active_profile"]
            .as_str()
            .context("missing active audio profile")?;
        if matches!(active, "a2dp-sink-sbc_xq" | "a2dp-sink") {
            return Ok(None);
        }
        let profiles = card["profiles"]
            .as_object()
            .context("missing audio profiles")?;
        for profile in ["a2dp-sink-sbc_xq", "a2dp-sink"] {
            if profiles
                .get(profile)
                .is_some_and(|v| v["available"] == "yes")
            {
                if profile == "a2dp-sink" {
                    eprintln!("Preferred SBC-XQ profile unavailable; using A2DP fallback");
                }
                return Ok(Some(profile));
            }
        }
        eprintln!("No available A2DP profile for {CARD}; retaining current profile");
    }
    Ok(None)
}

fn ensure_audio() -> Result<()> {
    if let Some(profile) =
        audio_profile(&util::json("pactl", &["--format=json", "list", "cards"])?)?
    {
        println!("Forcing {CARD} to {profile}");
        if let Err(error) = util::run("pactl", &["set-card-profile", CARD, profile]) {
            eprintln!(
                "Failed to set {profile} on {CARD}: {error:#}; continuing to watch audio events"
            );
        }
    }
    Ok(())
}

pub fn audio_watch() -> Result<()> {
    let mut child = Command::new("pactl")
        .arg("subscribe")
        .stdout(Stdio::piped())
        .spawn()?;
    let events = child.stdout.take().context("pactl stdout missing")?;
    let result = (|| -> Result<()> {
        ensure_audio()?;
        for event in BufReader::new(events).lines() {
            let event = event?;
            if event.contains("on card") || event.contains("on server") {
                ensure_audio()?;
            }
        }
        bail!("pactl event stream closed");
    })();
    let _ = child.kill();
    let _ = child.wait();
    result
}

pub fn magic_packet(mac: &str) -> Result<Vec<u8>> {
    let bytes: Vec<_> = mac
        .split(':')
        .map(|part| {
            ensure!(part.len() == 2, "invalid MAC component");
            Ok(u8::from_str_radix(part, 16)?)
        })
        .collect::<Result<_>>()?;
    ensure!(bytes.len() == 6, "MAC must have six octets");
    let mut packet = vec![255; 6];
    for _ in 0..16 {
        packet.extend_from_slice(&bytes);
    }
    Ok(packet)
}

pub fn wol(mac: &str, destination: &str) -> Result<()> {
    let packet = magic_packet(mac)?;
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_broadcast(true)?;
    ensure!(
        socket.send_to(&packet, destination)? == packet.len(),
        "short WoL send"
    );
    println!("WOL sent to {mac} via {destination}");
    Ok(())
}

pub fn netconsole(bind: &str, sender: IpAddr, output: &Path) -> Result<()> {
    let socket = UdpSocket::bind(bind)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(output)?;
    println!(
        "Listening on {bind}, accepting {sender}, appending {}",
        output.display()
    );
    let mut packet = vec![0u8; 65536];
    loop {
        let (length, peer) = socket.recv_from(&mut packet)?;
        if peer.ip() != sender {
            continue;
        }
        writeln!(
            file,
            "[{}] UDP {peer}",
            Local::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, false)
        )?;
        file.write_all(&packet[..length])?;
        if !packet[..length].ends_with(b"\n") {
            file.write_all(b"\n")?;
        }
    }
}
