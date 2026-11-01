//! One bounded AF_PACKET receive on Predator, with no promiscuous-mode change.
use std::{
    ffi::{CString, c_void},
    io::{self, Write},
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[repr(C)]
struct Link {
    family: u16,
    protocol: u16,
    index: i32,
    hatype: u16,
    kind: u8,
    len: u8,
    addr: [u8; 8],
}
#[repr(C)]
struct Poll {
    fd: i32,
    events: i16,
    revents: i16,
}
unsafe extern "C" {
    fn socket(domain: i32, kind: i32, protocol: i32) -> i32;
    fn bind(fd: i32, addr: *const c_void, len: u32) -> i32;
    fn if_nametoindex(name: *const i8) -> u32;
    fn poll(fds: *mut Poll, count: usize, timeout: i32) -> i32;
    fn recv(fd: i32, buf: *mut c_void, len: usize, flags: i32) -> isize;
}
const MAC: [u8; 6] = [0x98, 0x29, 0xa6, 0x3a, 0xa7, 0x50];
fn packet(b: &[u8]) -> bool {
    if b.len() < 42 || b[..6] != [255; 6] || b[12..14] != [8, 0] || b[14] >> 4 != 4 || b[23] != 17 {
        return false;
    }
    let ihl = usize::from(b[14] & 15) * 4;
    if ihl < 20
        || b.len() < 14 + ihl + 8
        || b[26..30] != [192, 168, 1, 166]
        || b[30..34] != [192, 168, 1, 255]
    {
        return false;
    }
    if u16::from_be_bytes([b[20], b[21]]) & 0x3fff != 0 {
        return false;
    }
    let u = 14 + ihl;
    let size = usize::from(u16::from_be_bytes([b[16], b[17]]));
    if size != ihl + 110
        || b.len() < 14 + size
        || b[u + 2..u + 4] != [0, 9]
        || b[u + 4..u + 6] != [0, 110]
    {
        return false;
    }
    let p = &b[u + 8..u + 110];
    p[..6] == [255; 6] && p[6..].chunks_exact(6).all(|m| m == MAC)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    assert_eq!(a.len(), 2, "expected unique capture label");
    assert!(a[1].bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'));
    assert_eq!(
        std::fs::read_to_string("/proc/sys/kernel/random/boot_id")?.trim(),
        "f9b45aa6-b820-4ab5-b18c-9324d8d53087"
    );
    assert_eq!(
        std::fs::read_to_string("/sys/class/net/enp3s0f1/address")?.trim(),
        "98:29:a6:3a:a7:50"
    );
    let name = CString::new("enp3s0f1")?;
    let idx = unsafe { if_nametoindex(name.as_ptr()) };
    assert!(idx > 0);
    let fd = unsafe { socket(17, 3 | 0x80000, i32::from(0x0800u16.to_be())) };
    if fd < 0 {
        return Err(io::Error::last_os_error().into());
    }
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let addr = Link {
        family: 17,
        protocol: 0x0800u16.to_be(),
        index: idx as i32,
        hatype: 0,
        kind: 0,
        len: 0,
        addr: [0; 8],
    };
    if unsafe {
        bind(
            fd.as_raw_fd(),
            (&addr as *const Link).cast(),
            std::mem::size_of::<Link>() as u32,
        )
    } != 0
    {
        return Err(io::Error::last_os_error().into());
    }
    println!(
        "READY label={} interface=enp3s0f1 ifindex={} max_seconds=25",
        a[1], idx
    );
    io::stdout().flush()?;
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("no matching packet before deadline".into());
        }
        let mut p = Poll {
            fd: fd.as_raw_fd(),
            events: 1,
            revents: 0,
        };
        let rc = unsafe {
            poll(
                &mut p,
                1,
                remaining.as_millis().min(i32::MAX as u128) as i32,
            )
        };
        if rc < 0 {
            return Err(io::Error::last_os_error().into());
        }
        if rc == 0 {
            continue;
        }
        if p.revents & 1 == 0 {
            return Err(format!("unexpected poll events {}", p.revents).into());
        }
        let mut b = [0u8; 2048];
        let n = unsafe { recv(fd.as_raw_fd(), b.as_mut_ptr().cast(), b.len(), 0) };
        if n < 0 {
            return Err(io::Error::last_os_error().into());
        }
        let b = &b[..n as usize];
        if packet(b) {
            let t = SystemTime::now().duration_since(UNIX_EPOCH)?;
            println!(
                "PASS label={} receipt_realtime_ns={} interface=enp3s0f1 ifindex={} bytes={} source=192.168.1.166 destination=192.168.1.255:9 target_mac=98:29:a6:3a:a7:50 magic_copies=16",
                a[1],
                t.as_nanos(),
                idx,
                b.len()
            );
            println!(
                "ethernet_frame_hex={}",
                b.iter().map(|v| format!("{v:02x}")).collect::<String>()
            );
            return Ok(());
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn valid() -> Vec<u8> {
        let mut b = vec![0; 144];
        b[..6].fill(255);
        b[12..14].copy_from_slice(&[8, 0]);
        b[14] = 0x45;
        b[16..18].copy_from_slice(&130u16.to_be_bytes());
        b[23] = 17;
        b[26..30].copy_from_slice(&[192, 168, 1, 166]);
        b[30..34].copy_from_slice(&[192, 168, 1, 255]);
        b[36..38].copy_from_slice(&[0, 9]);
        b[38..40].copy_from_slice(&[0, 110]);
        b[42..48].fill(255);
        for m in b[48..].chunks_exact_mut(6) {
            m.copy_from_slice(&MAC);
        }
        b
    }
    #[test]
    fn accepts_canonical_and_padding() {
        let mut b = valid();
        assert!(packet(&b));
        b.extend_from_slice(&[0; 8]);
        assert!(packet(&b));
    }
    #[test]
    fn rejects_truncation_wrong_target_fragment_and_wrong_interface_payload() {
        let b = valid();
        for i in 0..b.len() {
            assert!(!packet(&b[..i]));
        }
        for i in [0, 12, 23, 26, 30, 37, 39, 42, 143] {
            let mut b = valid();
            b[i] ^= 1;
            assert!(!packet(&b), "byte {i}");
        }
        let mut b = valid();
        b[20] = 0x20;
        assert!(!packet(&b));
    }
}
