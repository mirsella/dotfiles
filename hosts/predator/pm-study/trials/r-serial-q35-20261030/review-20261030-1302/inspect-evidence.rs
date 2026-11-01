use std::{collections::HashMap, error::Error, fs, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let root = std::env::args()
        .nth(1)
        .ok_or("capture directory required")?;
    let root = Path::new(&root);
    let early =
        fs::read_to_string(root.join("retained-artifacts/r-serial-q35-20261030-after-kernel.txt"))?;
    let export = fs::read_to_string(root.join("kernel-export.txt"))?;
    let boot = "8b7a173d84064072aebcc85c8b4989e7";
    let mut messages = Vec::new();
    let mut physical = Vec::new();
    for record in export.split("\n\n").filter(|r| !r.trim().is_empty()) {
        let fields: HashMap<_, _> = record
            .lines()
            .filter_map(|line| line.split_once('='))
            .collect();
        if fields.get("_BOOT_ID") != Some(&boot) {
            return Err("journal record belongs to an unexpected boot".into());
        }
        let message = *fields
            .get("MESSAGE")
            .ok_or("journal record missing MESSAGE")?;
        messages.push(message);
        if message.contains("PM: suspend entry")
            || message.contains("Low-level resume complete")
            || message.contains("PM: suspend exit")
            || message.contains("Timekeeping suspended for ")
        {
            println!("event={message}\n  received_realtime_us={}\n  journal_monotonic_us={}\n  source_monotonic_us={}",
                fields.get("__REALTIME_TIMESTAMP").ok_or("missing realtime timestamp")?,
                fields.get("__MONOTONIC_TIMESTAMP").ok_or("missing journal monotonic timestamp")?,
                fields.get("_SOURCE_MONOTONIC_TIMESTAMP").unwrap_or(&"not recorded"));
        }
        if let Some(duration) = message
            .strip_prefix("Timekeeping suspended for ")
            .and_then(|s| s.strip_suffix(" seconds"))
        {
            physical.push(duration.parse::<f64>()?);
        }
    }
    let events = [
        "PM: suspend entry (deep)",
        "Low-level resume complete",
        "PM: suspend exit",
    ];
    let positions: Vec<_> = events
        .iter()
        .map(|event| {
            messages
                .iter()
                .enumerate()
                .filter(|(_, message)| message.contains(event))
                .map(|(i, _)| i)
                .collect::<Vec<_>>()
        })
        .collect();
    for (event, found) in events.iter().zip(&positions) {
        println!(
            "early_count[{event}]={} final_count={}",
            early.lines().filter(|line| line.contains(event)).count(),
            found.len()
        );
    }
    if positions.iter().any(|p| p.len() != 1)
        || !(positions[0][0] < positions[1][0] && positions[1][0] < positions[2][0])
    {
        return Err("expected exactly one ordered entry/resume/exit in this boot".into());
    }
    if physical.len() != 1 || !physical[0].is_finite() || physical[0] < 900.0 {
        return Err("expected one finite current-cycle duration >=900s".into());
    }
    println!("measured_sleep_seconds={:.3}\nordered_current_boot_cycles=1\nearly_snapshot_lines={}\nearly_snapshot_tail:", physical[0], early.lines().count());
    let lines: Vec<_> = early.lines().collect();
    for line in lines.iter().skip(lines.len().saturating_sub(8)) {
        println!("{line}");
    }
    println!("classification=RESUMED_VERIFIER_ERROR_PHYSICAL_EVIDENCE_PENDING\nfull_pass=false\ncompletion_record_not_synthesized=true");
    Ok(())
}
