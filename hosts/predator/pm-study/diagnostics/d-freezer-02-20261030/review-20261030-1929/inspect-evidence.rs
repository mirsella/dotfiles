use std::{env, fs, path::Path};

fn cycle(text: &str) {
    let mut phase = 0;
    let mut cycles = 0;
    let mut markers = 0;
    for line in text.lines() {
        assert!(!line.contains("Low-level resume complete"));
        assert!(!line.contains("Timekeeping suspended for"));
        if line.contains("PM: suspend entry (deep)") {
            assert_eq!(phase, 0);
            phase = 1;
        } else if line.contains("PM: suspend debug:") {
            assert!(line.contains("Waiting for 5 second(s)."));
            assert_eq!(phase, 1);
            markers += 1;
            phase = 2;
        } else if line.contains("PM: suspend exit") {
            assert_eq!(phase, 2);
            phase = 0;
            cycles += 1;
        }
    }
    assert_eq!((phase, cycles, markers), (0, 1, 1));
}

fn main() {
    let root = env::args().nth(1).expect("capture directory");
    let root = Path::new(&root);
    let export = fs::read_to_string(root.join("kernel-export.txt")).unwrap();
    let mut records = 0;
    let mut messages = String::new();
    for block in export.split("\n\n").filter(|b| !b.trim().is_empty()) {
        assert!(block
            .lines()
            .any(|l| l == "_BOOT_ID=f8eb6e4485ae4e56bc00587876699945"));
        records += 1;
        if let Some(msg) = block.lines().find_map(|l| l.strip_prefix("MESSAGE=")) {
            messages.push_str(msg);
            messages.push('\n');
            if msg.contains("PM: suspend entry")
                || msg.contains("PM: suspend debug:")
                || msg.contains("PM: suspend exit")
            {
                for field in [
                    "__REALTIME_TIMESTAMP=",
                    "__MONOTONIC_TIMESTAMP=",
                    "_SOURCE_MONOTONIC_TIMESTAMP=",
                ] {
                    if let Some(value) = block.lines().find(|l| l.starts_with(field)) {
                        print!("{value} ");
                    }
                }
                println!("MESSAGE={msg}");
            }
        }
    }
    assert!(records > 1000);
    cycle(&messages);
    let artifacts = root.join("retained-artifacts/predator-pm-diagnostic");
    let file = |suffix: &str| artifacts.join(format!("d-freezer-02-20261030-{suffix}"));
    let observed = fs::read_to_string(file("observed-journal.txt")).unwrap();
    cycle(&observed);
    assert!(observed.contains("System returned from sleep operation 'suspend'"));
    assert!(observed.contains("Finished System Suspend."));
    cycle(&fs::read_to_string(file("after-kernel.txt")).unwrap());
    let returned = fs::read_to_string(file("return.txt")).unwrap();
    for expected in [
        "boot=f8eb6e44-85ae-4e56-bc00-587876699945",
        "request_and_return=Ok(())",
        "stop=Ok(())",
        "reaped=Ok(ExitStatus(unix_wait_status(9)))",
        "reader_joined=true",
    ] {
        assert!(returned.lines().any(|l| l == expected));
    }
    let outcome = fs::read_to_string(file("outcome.txt")).unwrap();
    assert!(outcome.contains("operation=Ok(())\ncleanup=Ok(())"));
    assert!(!file("completed.txt").exists());
    let plan = fs::read_to_string(file("plan.conf")).unwrap();
    let requested = fs::read_to_string(file("requested.txt")).unwrap();
    assert_eq!(plan.trim(), requested);
    println!("validated_boot_records={records}");
    println!("ordered_diagnostic_cycles=1\nfive_second_markers=1\nactual_s3_resume_messages=0\ntimekeeping_sleep_measurements=0");
    println!("raw_return_and_reader_cleanup=verified\nmode_cleanup=Ok\ncompleted_artifact=absent");
    println!("classification=DIAGNOSTIC_RETURNED_POSTCHECK_FAILED\nprotocol_completion=false\npredecessor_eligible=false");
}
