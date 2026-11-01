use std::{env, fs};
fn main() {
    let dir = env::args().nth(1).expect("capture directory");
    let export = fs::read_to_string(format!("{dir}/kernel-export.txt")).unwrap();
    let mut records = 0;
    let mut messages = String::new();
    for record in export.split("\n\n").filter(|r| !r.trim().is_empty()) {
        assert!(record
            .lines()
            .any(|l| l == "_BOOT_ID=f8eb6e4485ae4e56bc00587876699945"));
        records += 1;
        for line in record.lines().filter_map(|l| l.strip_prefix("MESSAGE=")) {
            messages.push_str(line);
            messages.push('\n');
        }
    }
    assert!(records > 1000);
    for marker in [
        "DIAG BEGIN",
        "PM: suspend entry",
        "suspend debug: Waiting",
        "PM: suspend exit",
        "Low-level resume complete",
        "Timekeeping suspended for",
        "DIAG VERIFIED",
    ] {
        let count = messages.matches(marker).count();
        assert_eq!(count, 0, "unexpected {marker}");
        println!("{marker}={count}");
    }
    assert_eq!(
        messages
            .matches("DIAG CLEANUP id=d-freezer-01-20261030")
            .count(),
        1
    );
    let plan = fs::read_to_string(format!(
        "{dir}/retained-artifacts/d-freezer-01-20261030-plan.conf"
    ))
    .unwrap();
    let requested = fs::read_to_string(format!(
        "{dir}/retained-artifacts/d-freezer-01-20261030-requested.txt"
    ))
    .unwrap();
    assert_eq!(plan.trim(), requested);
    println!("validated_expected_boot_records={records}\nrequested_is_trimmed_plan=true\nclassification=ABORTED_BEFORE_DIAGNOSTIC_SELF_JOB_GUARD\nnew_cycles=0\nphase_exercised=false\ncompletion=false");
}
