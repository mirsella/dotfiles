use std::{env, fs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = env::args().nth(1).ok_or("missing journal export path")?;
    let data = fs::read_to_string(file)?;
    let expected = "3c1873e4b86641e1b411702168ef90cf";
    let mut records = 0;
    let mut messages = Vec::new();
    for record in data
        .split("\n\n")
        .filter(|record| !record.trim().is_empty())
    {
        let boot = record
            .lines()
            .find_map(|line| line.strip_prefix("_BOOT_ID="));
        assert_eq!(boot, Some(expected), "unexpected boot in journal export");
        records += 1;
        if let Some(message) = record
            .lines()
            .find_map(|line| line.strip_prefix("MESSAGE="))
        {
            messages.push(message);
        }
    }
    assert!(records > 1000, "unexpectedly short journal export");
    println!("expected_boot={expected}\nvalidated_export_records={records}");
    for needle in [
        "BEGIN trial=r-serial-q35r1-20261030",
        "SUSPEND REQUEST",
        "PM: suspend entry (deep)",
        "Low-level resume complete",
        "PM: suspend exit",
        "Timekeeping suspended for",
        "VERIFIED",
    ] {
        let count = messages
            .iter()
            .filter(|message| message.contains(needle))
            .count();
        println!("{needle}: count={count}");
        assert_eq!(count, 0, "unexpected suspend evidence");
    }
    let capture = "CAPTURE CHECK boot=3c1873e4-b866-41e1-b411-702168ef90cf active_generation=/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363 nonce=1793365973";
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.contains(capture))
            .count(),
        1
    );
    assert_eq!(
        messages
            .iter()
            .filter(|message| message
                .contains("TRIAL CLEANUP id=r-serial-q35r1-20261030 pm_async=1 timesyncd=started"))
            .count(),
        1
    );
    println!("classification=ABORTED_BEFORE_SUSPEND_MAINTENANCE\nnew_resume_verifier_exercised=false\ncompletion_available=false\nfull_pass=false");
    Ok(())
}
