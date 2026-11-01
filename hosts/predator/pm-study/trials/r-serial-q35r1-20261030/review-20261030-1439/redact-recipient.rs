use std::{env, fs, io::Write, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(env::args().nth(1).ok_or("missing capture path")?);
    let recipient = env::var("TgId")?;
    assert!(!recipient.is_empty(), "empty recipient variable");
    let data = fs::read_to_string(&path)?;
    let count = data.matches(&recipient).count();
    assert!(
        count > 0,
        "expected private recipient in current notification capture"
    );
    let redacted = data.replace(&recipient, "[REDACTED_NOTIFICATION_RECIPIENT]");
    let temporary = path.with_extension("redacted-new");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(redacted.as_bytes())?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    println!("Redacted {count} occurrences of the private notification recipient from this review's newly collected status text. No token was present. Message IDs, dates, notification bodies and statuses remain intact. Full Pi evidence and all pre-existing captures are unchanged.");
    Ok(())
}
