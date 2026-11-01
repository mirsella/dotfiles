use std::{collections::BTreeMap, error::Error, fs};
type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320u32 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}
fn unhex(s: &str) -> Result<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return Err("invalid hex length".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| Ok(u8::from_str_radix(&s[i..i + 2], 16)?))
        .collect()
}
fn expected(nonce: &str, boot: &str) -> Result<[u8; 64]> {
    if !(8..=15).contains(&nonce.len())
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err("invalid nonce".into());
    }
    if boot.len() != 36
        || [8, 13, 18, 23]
            .into_iter()
            .any(|n| boot.as_bytes()[n] != b'-')
    {
        return Err("invalid boot UUID".into());
    }
    let uuid = unhex(&boot.replace('-', ""))?;
    if uuid.len() != 16 {
        return Err("invalid boot UUID bytes".into());
    }
    let mut bytes = [0u8; 64];
    bytes[..8].copy_from_slice(b"PMS3BND2");
    bytes[8..10].copy_from_slice(&2u16.to_le_bytes());
    bytes[10..12].copy_from_slice(&64u16.to_le_bytes());
    bytes[12..16].copy_from_slice(&1u32.to_le_bytes());
    bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
    bytes[24..40].copy_from_slice(&uuid);
    bytes[40..40 + nonce.len()].copy_from_slice(nonce.as_bytes());
    let crc = crc32(&bytes[..60]);
    bytes[60..].copy_from_slice(&crc.to_le_bytes());
    Ok(bytes)
}
fn verify(readback: &str, status: &str, nonce: &str, boot: &str) -> Result<(u64, u64, u64)> {
    let hex = readback
        .strip_prefix("attributes=7 bytes=64\nhex=")
        .and_then(|s| s.strip_suffix('\n'))
        .ok_or("wrong attributes/readback framing")?;
    let bytes = unhex(hex)?;
    if bytes != expected(nonce, boot)? {
        return Err("wrong record identity, stage, sequence, reserved fields or checksum".into());
    }
    let mut fields = BTreeMap::new();
    for line in status.lines() {
        let (key, value) = line.split_once('=').ok_or("invalid status field")?;
        if fields.insert(key, value).is_some() {
            return Err("duplicate status field".into());
        }
    }
    let keys = [
        "schema",
        "attempted",
        "write_errno",
        "audit_errno",
        "efi_status",
        "reserve_bytes",
        "margin_bytes",
        "allocation_budget_bytes",
        "charged_bytes",
        "before_storage_bytes",
        "before_remaining_bytes",
        "before_max_bytes",
        "after_storage_bytes",
        "after_remaining_bytes",
        "after_max_bytes",
        "write_duration_ns",
    ];
    if fields.len() != keys.len() || keys.iter().any(|key| !fields.contains_key(key)) {
        return Err("unexpected/missing status fields".into());
    }
    for (key, value) in [
        ("schema", "compact-efi-probe-1"),
        ("attempted", "1"),
        ("write_errno", "0"),
        ("audit_errno", "0"),
        ("efi_status", "0"),
        ("reserve_bytes", "5120"),
        ("margin_bytes", "1024"),
        ("allocation_budget_bytes", "1024"),
    ] {
        if fields[key] != value {
            return Err(format!("invalid {key}: {}", fields[key]).into());
        }
    }
    let n = |key| -> Result<u64> { Ok(fields[key].parse()?) };
    let charged = 64 + 2 * ("PMS3-".len() + nonce.len() + 1) as u64;
    if n("charged_bytes")? != charged {
        return Err("incorrect data/name accounting".into());
    }
    let before = n("before_remaining_bytes")?;
    let after = n("after_remaining_bytes")?;
    if before < 7168 || after < 6144 || after > before || before - after > 1024 {
        return Err("capacity audit violated".into());
    }
    if n("before_storage_bytes")? != n("after_storage_bytes")?
        || n("before_max_bytes")? != n("after_max_bytes")?
        || n("before_max_bytes")? < charged
    {
        return Err("capacity identity/max changed".into());
    }
    let duration = n("write_duration_ns")?;
    if duration == 0 {
        return Err("missing duration".into());
    }
    Ok((before, after, duration))
}
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 5 {
        return Err("usage: verify READBACK STATUS NONCE BOOT".into());
    }
    let (before, after, duration) = verify(
        &fs::read_to_string(&a[1])?,
        &fs::read_to_string(&a[2])?,
        &a[3],
        &a[4],
    )?;
    println!(
        "{{\"verification\":\"PASS\",\"phase\":\"AWAKE_TRANSPORT\",\"sequence\":1,\"recordBytes\":64,\"freeBefore\":{before},\"freeAfter\":{after},\"netStoreDecrease\":{},\"kernelReserve\":5120,\"extraMargin\":1024,\"writeDurationNs\":{duration},\"retentionAcrossResetVerified\":false,\"suspendedContextVerified\":false}}",
        before - after
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const NONCE: &str = "20261101-a";
    const BOOT: &str = "bf51c3ac-164e-4bfa-b56c-7b71df526b7f";
    fn input() -> (String, String) {
        let hex = expected(NONCE, BOOT)
            .unwrap()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        (format!("attributes=7 bytes=64\nhex={hex}\n"), "schema=compact-efi-probe-1\nattempted=1\nwrite_errno=0\naudit_errno=0\nefi_status=0\nreserve_bytes=5120\nmargin_bytes=1024\nallocation_budget_bytes=1024\ncharged_bytes=96\nbefore_storage_bytes=163752\nbefore_remaining_bytes=7498\nbefore_max_bytes=8192\nafter_storage_bytes=163752\nafter_remaining_bytes=7306\nafter_max_bytes=8192\nwrite_duration_ns=12345\n".into())
    }
    #[test]
    fn independent_crc_known_vector() {
        assert_eq!(crc32(b"123456789"), 0xcbf43926);
    }
    #[test]
    fn complete_readback_and_budget() {
        let (r, s) = input();
        assert_eq!(verify(&r, &s, NONCE, BOOT).unwrap(), (7498, 7306, 12345));
    }
    #[test]
    fn rejects_corruption_truncation_wrong_identity() {
        let (r, s) = input();
        for n in 0..r.len() {
            assert!(verify(&r[..n], &s, NONCE, BOOT).is_err());
        }
        for broken in [
            r.replace("attributes=7", "attributes=6"),
            r.replace("504d5333", "514d5333"),
            r.clone() + "extra\n",
        ] {
            assert!(verify(&broken, &s, NONCE, BOOT).is_err());
        }
        assert!(verify(&r, &s, "20261101-b", BOOT).is_err());
        assert!(verify(&r, &s, NONCE, "bf51c3ac-164e-4bfa-b56c-7b71df526b70").is_err());
    }
    #[test]
    fn rejects_unsafe_or_unmeasurable_capacity() {
        let (r, s) = input();
        for broken in [
            s.replace("7498", "7167"),
            s.replace("7306", "6143"),
            s.replace("7306", "6473"),
            s.replace("7306", "7499"),
            s.replace("charged_bytes=96", "charged_bytes=64"),
            s.replace("reserve_bytes=5120", "reserve_bytes=0"),
            s.replace("attempted=1", "attempted=2"),
            s.replace("write_errno=0", "write_errno=-5"),
            s.replace("audit_errno=0", "audit_errno=-116"),
            s.replace("after_max_bytes=8192", "after_max_bytes=8193"),
            s.clone() + "attempted=1\n",
        ] {
            assert!(verify(&r, &broken, NONCE, BOOT).is_err());
        }
    }
}
