use std::{error::Error, fs};
type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn verify(raw: &str, nonce: &str, boot: &str) -> Result<()> {
    let expected =
        format!("PMBOUNDARY-PROBE-1\nnonce={nonce}\nboot={boot}\ncontext=awake-irqs-disabled\n");
    let (header, body) = raw.split_once('\n').ok_or("missing attribute header")?;
    if header != format!("attributes=7 bytes={}", expected.len()) {
        return Err("wrong firmware attributes or length".into());
    }
    if body != expected {
        return Err("readback differs from the declared probe identity and context".into());
    }
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: verify-probe READBACK NONCE ORIGINAL_BOOT".into());
    }
    verify(&fs::read_to_string(&args[1])?, &args[2], &args[3])?;
    println!("awake_probe_readback=PASS; reset_retention_and_suspended_context=UNPROVEN");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const BOOT: &str = "f9b45aa6-b820-4ab5-b18c-9324d8d53087";
    fn good() -> String {
        let body = format!(
            "PMBOUNDARY-PROBE-1\nnonce=awake-20261101-a\nboot={BOOT}\ncontext=awake-irqs-disabled\n"
        );
        format!("attributes=7 bytes={}\n{body}", body.len())
    }
    #[test]
    fn validates_complete_firmware_readback() {
        verify(&good(), "awake-20261101-a", BOOT).unwrap();
    }
    #[test]
    fn rejects_wrong_identity_truncation_and_context_claims() {
        let record = good();
        for n in 0..record.len() {
            assert!(verify(&record[..n], "awake-20261101-a", BOOT).is_err());
        }
        for invalid in [
            record.replace("attributes=7", "attributes=6"),
            record.replace("awake-20261101-a", "awake-20261101-b"),
            record.replace("f9b45aa6", "00000000"),
            record.replace("awake-irqs-disabled", "syscore-suspended"),
            record.clone() + "unexpected extra record\n",
        ] {
            assert!(verify(&invalid, "awake-20261101-a", BOOT).is_err());
        }
    }
}
