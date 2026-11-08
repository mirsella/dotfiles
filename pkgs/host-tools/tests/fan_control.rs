use std::{fs, process::Command};

const BIN: &str = env!("CARGO_BIN_EXE_host-tools");

#[test]
fn case_temperature_follows_cpu_or_gpu_demand_and_rejects_sensor_failures() {
    let root = tempfile::tempdir().unwrap();
    let cpu = root.path().join("cpu");
    let gpu = root.path().join("gpu");
    for cpu_maximum in [80, 85] {
        let spec = serde_json::json!({
            "cpu": cpu, "gpu": gpu,
            "cpu_minimum": 65, "cpu_maximum": cpu_maximum,
            "gpu_minimum": 50, "gpu_maximum": 80,
        })
        .to_string();
        let sample = || {
            Command::new(BIN)
                .args(["fan-control", "case-temperature", &spec])
                .output()
                .unwrap()
        };
        for (cpu_value, gpu_value, expected) in [
            (55000, 45000, 65000),
            (55000, 65000, (65000 + cpu_maximum * 1000) / 2),
            (79000, 45000, 79000),
            (55000, 80000, cpu_maximum * 1000),
            (55000, 100000, cpu_maximum * 1000),
            (90000, 45000, 90000),
        ] {
            fs::write(&cpu, cpu_value.to_string()).unwrap();
            fs::write(&gpu, gpu_value.to_string()).unwrap();
            let output = sample();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                String::from_utf8(output.stdout).unwrap().trim(),
                expected.to_string()
            );
        }
        for bad_value in ["garbage", "0", "-1", "121000"] {
            fs::write(&gpu, bad_value).unwrap();
            let output = sample();
            assert!(!output.status.success());
            assert!(output.stdout.is_empty());
        }
        fs::remove_file(&gpu).unwrap();
        assert!(!sample().status.success());
        fs::write(&gpu, "65000").unwrap();
        fs::remove_file(&cpu).unwrap();
        assert!(!sample().status.success());
    }
}
