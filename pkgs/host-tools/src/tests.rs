use crate::{activity, backup, hardware, maintenance, monitoring, setup, suspend};
use anyhow::{Result, bail};
use flate2::{Compression, write::GzEncoder};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::{PermissionsExt, symlink},
};

fn request(host: &str, uri: &str, status: u16) -> Value {
    json!({"ts":100,"duration":0,"request":{"host":host,"uri":uri,"method":"GET"},"status":status,"resp_headers":{}})
}

fn authenticated(entry: &Value) -> Result<Option<&'static str>> {
    let line = entry.to_string();
    serde_json::from_str::<activity::AccessLog<'_>>(&line)?.authenticated()
}

#[test]
fn authenticated_routes_and_public_exclusions() -> Result<()> {
    for (host, uri, app) in [
        (
            "photos.mirsella.mooo.com",
            "/api/assets/123",
            Some("Immich"),
        ),
        ("photos.mirsella.mooo.com", "/api/server/ping", None),
        ("photos.mirsella.mooo.com", "/api/socket.io/", None),
        ("photos.mirsella.mooo.com", "/photos", None),
        ("mirsella.mooo.com:4096", "/session", Some("OpenCode")),
        ("mirsella.mooo.com:14096", "/global/event", Some("OpenCode")),
        ("mirsella.mooo.com:4096", "/global/health", None),
        ("mirsella.mooo.com:4097", "/api/config", Some("OpenChamber")),
        (
            "mirsella.mooo.com:14097",
            "/auth/session",
            Some("OpenChamber"),
        ),
        ("mirsella.mooo.com:4097", "/auth/passkey/status", None),
        ("mirsella.mooo.com:4097", "/api/version", None),
        ("192.0.2.1", "/api/assets", None),
    ] {
        let mut entry = request(host, uri, 200);
        assert_eq!(authenticated(&entry)?, app, "{host}{uri}");
        entry["status"] = json!(304);
        assert_eq!(authenticated(&entry)?, app);
        for status in [101, 301, 400, 401, 403, 404, 500] {
            entry["status"] = json!(status);
            assert_eq!(authenticated(&entry)?, None);
        }
        entry["status"] = json!(200);
        entry["request"]["method"] = json!("OPTIONS");
        assert_eq!(authenticated(&entry)?, None);
    }
    Ok(())
}

#[test]
fn nextcloud_needs_response_identity_even_on_authenticated_errors() -> Result<()> {
    let mut entry = request(
        "mirsella.mooo.com",
        "/nextcloud/remote.php/dav/files/user",
        404,
    );
    entry["request"]["headers"] =
        json!({"Cookie":["fake"],"Authorization":["Bearer fake"],"X-User-Id":["fake"]});
    assert_eq!(authenticated(&entry)?, None);
    entry["resp_headers"] = json!({"x-user-id":["user"]});
    assert_eq!(authenticated(&entry)?, Some("Nextcloud"));
    entry["request"]["uri"] = json!("/.env");
    assert_eq!(authenticated(&entry)?, None);
    Ok(())
}

#[test]
fn access_logs_decode_escaped_routes_and_identity_headers() -> Result<()> {
    let line = r#"{"ts":100,"request":{"host":"MIRSELLA.MOOO.COM:443","uri":"\/nextcloud\/remote.php?quoted=\"x\"","method":"GET"},"status":404,"resp_headers":{"X-\u0055ser-Id":["mirs\u0065lla"],"Etag":["\"abc\""]}}"#;
    let log: activity::AccessLog<'_> = serde_json::from_str(line)?;
    assert_eq!(log.authenticated()?, Some("Nextcloud"));
    let line = r#"{"ts":100,"request":{"host":"PHOTOS.MIRSELLA.MOOO.COM","uri":"\/api\/assets?query=1#fragment","method":"GET"},"status":200}"#;
    let log: activity::AccessLog<'_> = serde_json::from_str(line)?;
    assert_eq!(log.authenticated()?, Some("Immich"));
    Ok(())
}

#[test]
fn logs_use_completion_time_and_survive_scanner_floods() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let mut file = fs::File::create(tmp.path().join("access-test.log"))?;
    let mut valid = request("photos.mirsella.mooo.com", "/api/assets", 201);
    valid["ts"] = json!(0);
    valid["duration"] = json!(7190);
    writeln!(file, "{valid}")?;
    for _ in 0..10000 {
        writeln!(file, "{}", request("192.0.2.1", "/.env", 404))?;
    }
    assert!(
        activity::recent_logs(tmp.path(), 7200.)?
            .unwrap()
            .contains("Immich")
    );
    assert!(activity::recent_logs(tmp.path(), 9000.)?.is_none());
    Ok(())
}

#[test]
fn compressed_rotations_require_real_activity() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("access-test.log.gz");
    for (uri, expected) in [("/api/server/ping", false), ("/api/assets", true)] {
        let mut gzip = GzEncoder::new(fs::File::create(&path)?, Compression::default());
        writeln!(gzip, "{}", request("photos.mirsella.mooo.com", uri, 200))?;
        gzip.finish()?;
        assert_eq!(activity::recent_logs(tmp.path(), 120.)?.is_some(), expected);
    }
    Ok(())
}

#[test]
fn malformed_activity_does_not_report_idle() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    for line in [
        "not JSON",
        r#"{"ts":100,"request":42,"status":200}"#,
        r#"{"ts":100,"duration":null,"request":{},"status":200}"#,
        r#"{"ts":1e308,"duration":1e308,"request":{},"status":200}"#,
    ] {
        fs::write(tmp.path().join("access-broken.log"), format!("{line}\n"))?;
        assert!(activity::recent_logs(tmp.path(), 120.).is_err(), "{line}");
    }
    assert!(activity::recent_logs(&tmp.path().join("missing"), 120.).is_err());
    Ok(())
}

#[test]
fn preauth_ssh_and_spoofed_usernames_are_not_logins() {
    for title in [
        "sshd-session: [accepted]",
        "sshd: root [preauth]",
        "sshd: root@notty [preauth]",
        "sshd-auth: user [net]",
        "sshd: /nix/store/sshd -D [listener]",
    ] {
        assert!(!activity::authenticated_ssh(title), "{title}");
    }
    for title in [
        "sshd-session: mirsella [postauth]",
        "sshd-session: mirsella@notty",
        "sshd: user@pts/3",
    ] {
        assert!(activity::authenticated_ssh(title));
    }
}

#[test]
fn only_caddy_backend_connections_guard_transfers() -> Result<()> {
    assert!(activity::upstream("0 0 192.168.1.19:443 192.0.2.1:12345")?.is_none());
    assert!(
        activity::upstream("0 0 127.0.0.1:12345 127.0.0.1:2283 users:((\"curl\",pid=2,fd=3))")?
            .is_none()
    );
    assert!(activity::upstream("0 0 127.0.0.1:12345 127.0.0.1:2283").is_err());
    for (peer, app) in [
        ("127.0.0.1:2283", "Immich"),
        ("127.0.0.1:8080", "Nextcloud"),
        ("192.168.1.131:4096", "OpenCode"),
        ("192.168.1.131:4097", "OpenChamber"),
    ] {
        let text = format!("0 0 127.0.0.1:12345 {peer} users:((\"caddy\",pid=2,fd=3))");
        assert!(activity::upstream(&text)?.unwrap().contains(app));
    }
    Ok(())
}

fn pool() -> Value {
    json!({"name":"tank","state":"ONLINE","error_count":0,"vdevs":{"tank":{"state":"ONLINE","read_errors":0,"write_errors":0,"checksum_errors":2}}})
}

#[test]
fn only_corrected_checksum_errors_are_candidates_for_clear() -> Result<()> {
    assert!(suspend::pool_refusal(&pool())?.is_none());
    for (field, value) in [
        ("read_errors", json!(1)),
        ("write_errors", json!(1)),
        ("state", json!("FAULTED")),
        ("checksum_errors", json!(0)),
    ] {
        let mut p = pool();
        p["vdevs"]["tank"][field] = value;
        assert!(suspend::pool_refusal(&p)?.is_some());
    }
    let mut p = pool();
    p["error_count"] = json!(1);
    assert!(suspend::pool_refusal(&p)?.is_some());
    let mut p = pool();
    p["scan_stats"] = json!({"state":"SCANNING"});
    assert!(suspend::pool_refusal(&p)?.is_some());
    assert!(suspend::pool_refusal(&json!({})).is_err());
    Ok(())
}

#[test]
fn shared_storage_fault_patterns_cover_all_usb_branches() {
    for text in [
        "usb 2-2: USB disconnect, device number 3",
        "usb 2-3.1: reset SuperSpeed USB device",
        "usb 2-4: device descriptor read/64, error -71",
        "uas_eh_abort_handler tag 8",
        "sd 0:0:0:0: [sdc] Synchronize Cache failed",
        "I/O error, dev dm-3, sector 0",
        "xhci_hcd 0000:00:14.0: HC died; cleaning up",
    ] {
        assert!(monitoring::storage_event(text), "{text}");
    }
    for text in [
        "usb 1-1: new device",
        "ACPI: Low-level resume complete",
        "all pools are healthy",
    ] {
        assert!(!monitoring::storage_event(text));
    }
}

fn entry(cursor: &str, message: Value) -> Value {
    json!({"__CURSOR":cursor,"__REALTIME_TIMESTAMP":"1791302000000000","MESSAGE":message})
}

fn parse_entry(entry: Value) -> Result<monitoring::JournalEntry> {
    Ok(serde_json::from_value(entry)?)
}

#[test]
fn alert_failure_preserves_cursor_then_retry_acknowledges() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let cursor = tmp.path().join("cursor");
    fs::write(&cursor, "old\n")?;
    let records = vec![entry("next", json!("uas_eh_abort_handler tag 8"))];
    let update = monitoring::collect_entries(
        records.clone().into_iter().map(parse_entry),
        false,
        "predator",
    )?
    .unwrap();
    assert!(update.deliver(&cursor, |_| bail!("offline")).is_err());
    assert_eq!(fs::read_to_string(&cursor)?, "old\n");
    monitoring::collect_entries(records.into_iter().map(parse_entry), false, "predator")?
        .unwrap()
        .deliver(&cursor, |text| {
            assert!(text.contains("Matching kernel messages: 1"));
            Ok(())
        })?;
    assert_eq!(fs::read_to_string(cursor)?, "next\n");
    Ok(())
}

#[test]
fn new_alert_install_starts_at_tail_without_notifying() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let cursor = tmp.path().join("cursor");
    monitoring::collect_entries(
        [
            parse_entry(entry("old", json!("uas_eh_abort_handler"))),
            parse_entry(json!({"__CURSOR":"tail"})),
        ],
        true,
        "predator",
    )?
    .unwrap()
    .deliver(&cursor, |_| panic!("historical alert"))?;
    assert_eq!(fs::read_to_string(cursor)?, "tail\n");
    Ok(())
}

#[test]
fn incomplete_journal_stream_never_acknowledges_or_sends() -> Result<()> {
    let items = [
        parse_entry(entry("new", json!("uas_eh_abort_handler"))),
        Err(anyhow::anyhow!("journal failed")),
    ];
    assert!(monitoring::collect_entries(items, false, "predator").is_err());
    // Valid JSON from a failed producer must not yield a commit.
    let record = entry("new", json!("uas_eh_abort_handler")).to_string();
    let read = |exit: &str| {
        monitoring::read_journal(
            std::process::Command::new("sh").args([
                "-c",
                "printf '%s\\n' \"$1\"; exit \"$2\"",
                "journal",
                &record,
                exit,
            ]),
            false,
            "predator",
        )
    };
    assert!(read("1").is_err());
    assert!(read("0")?.is_some());
    assert!(monitoring::collect_entries([], false, "predator")?.is_none());
    assert!(monitoring::collect_entries([], true, "predator").is_err());
    for value in [
        json!([]),
        json!({"MESSAGE":"missing cursor"}),
        entry("new", json!([256])),
        entry("new", json!([-1])),
        entry("new", json!(false)),
    ] {
        assert!(monitoring::collect_entries([parse_entry(value)], false, "predator").is_err());
    }
    Ok(())
}

#[test]
fn telegram_limit_handles_astral_unicode_and_binary_journal_messages() -> Result<()> {
    let bounded = monitoring::bounded_message("😀".repeat(10000));
    assert!(bounded.encode_utf16().count() <= 4096);
    assert!(bounded.ends_with("journal.]"));
    let tmp = tempfile::tempdir()?;
    let mut bytes = b"uas_eh_abort_handler ".to_vec();
    bytes.push(255);
    monitoring::collect_entries([parse_entry(entry("new", json!(bytes)))], false, "predator")?
        .unwrap()
        .deliver(&tmp.path().join("cursor"), |text| {
            assert!(text.contains('\u{fffd}'));
            Ok(())
        })?;
    let entries = (0..200).map(|n| {
        parse_entry(entry(
            &n.to_string(),
            json!(format!("uas_eh_abort_handler {}", "😀".repeat(1000))),
        ))
    });
    monitoring::collect_entries(entries, false, "predator")?
        .unwrap()
        .deliver(&tmp.path().join("cursor"), |text| {
            assert!(text.contains("Matching kernel messages: 200"));
            assert!(text.encode_utf16().count() <= 4096);
            Ok(())
        })?;
    assert_eq!(fs::read_to_string(tmp.path().join("cursor"))?, "199\n");
    Ok(())
}

#[test]
fn backup_failure_keeps_previous_latest_and_cleans_staging() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let out = tmp.path();
    backup::publish(out, "backup-001", |p| {
        fs::write(p.join("data"), "good")?;
        Ok(())
    })?;
    assert!(
        backup::publish(out, "backup-002", |p| {
            fs::write(p.join("partial"), "bad")?;
            bail!("dump failed")
        })
        .is_err()
    );
    assert_eq!(
        fs::read_link(out.join("latest"))?,
        std::path::Path::new("backup-001")
    );
    assert_eq!(fs::read_to_string(out.join("latest/data"))?, "good");
    assert!(!out.read_dir()?.any(|p| {
        p.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".incomplete")
    }));
    assert_eq!(out.metadata()?.permissions().mode() & 0o777, 0o700);
    Ok(())
}

#[test]
fn backup_retains_fourteen_and_current_when_clock_moves_backwards() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    for n in 1..=16 {
        backup::publish(tmp.path(), &format!("backup-{n:03}"), |_| Ok(()))?;
    }
    backup::publish(tmp.path(), "backup-000", |_| Ok(()))?;
    let directories = tmp
        .path()
        .read_dir()?
        .filter(|p| p.as_ref().unwrap().file_type().unwrap().is_dir())
        .count();
    assert_eq!(directories, 14);
    assert!(tmp.path().join("backup-000").is_dir());
    assert!(tmp.path().join("backup-016").is_dir());
    assert!(!tmp.path().join("backup-003").exists());
    Ok(())
}

#[test]
fn backup_commands_use_staging_as_working_directory() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let source = tmp.path().join("source");
    fs::create_dir(&source)?;
    fs::write(source.join("config"), "saved configuration")?;
    let out = tmp.path().join("bundles");
    backup::publish(&out, "backup-001", |staging| {
        backup::Task {
            program: "sh".into(),
            args: vec![
                "-c".into(),
                "pwd -P; printf '%s' \"$1\"".into(),
                "command".into(),
                "literal @STAGING@".into(),
            ],
            stdout: Some("stdout".into()),
        }
        .run(staging)?;
        let expected = format!("{}\nliteral @STAGING@", staging.canonicalize()?.display());
        assert_eq!(fs::read_to_string(staging.join("stdout"))?, expected);
        backup::Task {
            program: "tar".into(),
            args: vec![
                "--create".into(),
                "--file=config.tar".into(),
                format!("--directory={}", source.display()),
                "config".into(),
            ],
            stdout: None,
        }
        .run(staging)?;
        Ok(())
    })?;
    let archive = out.join("latest/config.tar");
    let contents = crate::util::output("tar", &["-xOf", crate::util::path(&archive)?, "config"])?;
    assert_eq!(contents, "saved configuration");
    Ok(())
}

#[test]
fn backup_refuses_symlink_outputs_and_unexpected_latest() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let real = tmp.path().join("real");
    fs::create_dir(&real)?;
    symlink(&real, tmp.path().join("link"))?;
    assert!(
        backup::publish(&tmp.path().join("link"), "backup-001", |_| panic!(
            "followed link"
        ))
        .is_err()
    );
    fs::write(real.join("latest"), "keep")?;
    assert!(backup::publish(&real, "backup-001", |_| panic!("overwrote latest")).is_err());
    assert_eq!(fs::read_to_string(real.join("latest"))?, "keep");
    Ok(())
}

#[test]
fn nextcloud_refuses_mismatched_external_mounts() -> Result<()> {
    let mut mount = json!({"storage":"\\OC\\Files\\Storage\\Local","authentication_type":"null::null","configuration":{"datadir":"/srv/data/fast"}});
    setup::validate_mount(&mount, "/srv/data/fast")?;
    assert!(setup::validate_mount(&mount, "/wrong").is_err());
    mount["storage"] = json!("smb");
    assert!(setup::validate_mount(&mount, "/srv/data/fast").is_err());
    Ok(())
}

#[test]
fn btrfs_thresholds_and_bounded_reclaim() -> Result<()> {
    const G: u64 = 1 << 30;
    assert_eq!(
        maintenance::unallocated("Device unallocated: 8589934592\n")?,
        8 * G
    );
    assert!(maintenance::unallocated("broken").is_err());
    assert!(!maintenance::pressure(89., 8 * G));
    assert!(maintenance::pressure(90., 8 * G));
    assert!(maintenance::pressure(10., 7 * G));
    let mut values = [5 * G, 9 * G, 12 * G].into_iter();
    let mut passes = 0;
    maintenance::reclaim_space(
        || Ok(values.next().unwrap()),
        || {
            passes += 1;
            Ok(())
        },
    )?;
    assert_eq!(passes, 2);
    maintenance::reclaim_space(|| Ok(8 * G), || panic!("unneeded balance"))?;
    let mut passes = 0;
    assert!(
        maintenance::reclaim_space(
            || Ok(5 * G),
            || {
                passes += 1;
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(passes, 1);
    let mut values = [G, 2 * G, 3 * G, 4 * G, 5 * G].into_iter();
    let mut passes = 0;
    assert!(
        maintenance::reclaim_space(
            || Ok(values.next().unwrap()),
            || {
                passes += 1;
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(passes, 4);
    Ok(())
}

#[test]
fn audio_preserves_working_profile_and_selects_available_fallback() -> Result<()> {
    let mut cards = json!([{"name":"bluez_card.54_B7_E5_C2_75_28","active_profile":"headset-head-unit","profiles":{"a2dp-sink-sbc_xq":{"available":"no"},"a2dp-sink":{"available":"yes"}}}]);
    assert_eq!(hardware::audio_profile(&cards)?, Some("a2dp-sink"));
    cards[0]["profiles"]["a2dp-sink-sbc_xq"]["available"] = json!("yes");
    assert_eq!(hardware::audio_profile(&cards)?, Some("a2dp-sink-sbc_xq"));
    cards[0]["active_profile"] = json!("a2dp-sink");
    assert!(hardware::audio_profile(&cards)?.is_none());
    Ok(())
}

#[test]
fn wol_packet_has_exact_hardware_address_repetitions() -> Result<()> {
    let packet = hardware::magic_packet("98:29:a6:3a:a7:50")?;
    assert_eq!(packet.len(), 102);
    assert_eq!(&packet[..6], &[255; 6]);
    for mac in packet[6..].chunks(6) {
        assert_eq!(mac, &[0x98, 0x29, 0xa6, 0x3a, 0xa7, 0x50]);
    }
    assert!(hardware::magic_packet("not:a:mac").is_err());
    Ok(())
}
