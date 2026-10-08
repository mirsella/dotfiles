#!/usr/bin/env -S cargo +nightly -Zscript
---
[package]
edition = "2024"

[dependencies]
serde_json = "=1.0.145"
---

//! Warm decode, AMD DRM counters, and executable coding/tool-call checks.
//! Run against a direct llama-server with --alias benchmark on an unused port:
//! CARGO_TARGET_DIR=$HOME/dev/llama-benchmarks/target cargo +nightly -Zscript \
//!   tests/llama-benchmark.rs LABEL http://127.0.0.1:8081 PID RESULTS_DIRECTORY
//! LABEL must be new: each run owns its responses, provenance, and compiled checks.

use serde_json::{Value, json};
use std::{
    env,
    error::Error,
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::Instant,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn request(base: &str, prompt: &str, tokens: u32, extra: Value) -> Result<Value> {
    let mut body = json!({
        "model": "benchmark", "messages": [{"role": "user", "content": prompt}],
        "temperature": 0, "max_tokens": tokens, "cache_prompt": false,
        "reasoning_format": "deepseek", "chat_template_kwargs": {"enable_thinking": false}
    });
    let Value::Object(extra) = extra else {
        return Err("request overrides must be an object".into());
    };
    body.as_object_mut().expect("request object").extend(extra);
    let payload = serde_json::to_vec(&body)?;
    let mut child = Command::new("curl")
        .args([
            "--fail-with-body",
            "--silent",
            "--show-error",
            "--max-time",
            "600",
            "-H",
            "Content-Type: application/json",
            "--data-binary",
            "@-",
            &format!("{base}/v1/chat/completions"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let sent = child
        .stdin
        .take()
        .expect("piped request")
        .write_all(&payload);
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(format!(
            "HTTP request failed: {} {}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        )
        .into());
    }
    sent?;
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn content(response: &Value) -> Result<&str> {
    response["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "missing assistant content".into())
}

// A bad model answer fails its check; transport and compiler-launch errors abort the run.
fn model_check(name: &str, result: Result<bool>) -> Value {
    match result {
        Ok(passed) => json!({"name": name, "passed": passed}),
        Err(error) => json!({"name": name, "passed": false, "error": error.to_string()}),
    }
}

fn tool_check(response: &Value) -> Value {
    model_check(
        "tool-call",
        (|| {
            let calls = response["choices"][0]["message"]["tool_calls"]
                .as_array()
                .ok_or("missing tool calls")?;
            let [call] = calls.as_slice() else {
                return Err("expected exactly one tool call".into());
            };
            let tool = &call["function"];
            let arguments: Value =
                serde_json::from_str(tool["arguments"].as_str().ok_or("missing tool arguments")?)?;
            Ok(tool["name"] == "lookup_timezone" && arguments == json!({"city": "Paris"}))
        })(),
    )
}

fn rust_source(text: &str) -> Result<&str> {
    let text = text.trim();
    match text
        .strip_prefix("```rust")
        .or_else(|| text.strip_prefix("```"))
    {
        Some(fenced) => Ok(fenced
            .trim_end()
            .strip_suffix("```")
            .ok_or("unclosed Rust source fence")?
            .trim()),
        None => Ok(text),
    }
}

fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn field<'a>(text: &'a str, name: &str) -> Result<&'a str> {
    text.lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key == name).then(|| value.trim())
        })
        .ok_or_else(|| format!("missing counter {name}").into())
}

fn number(text: &str, name: &str) -> Result<u64> {
    Ok(field(text, name)?
        .split_whitespace()
        .next()
        .ok_or("empty counter")?
        .parse()?)
}

fn read_number(path: impl AsRef<Path>) -> Result<u64> {
    Ok(fs::read_to_string(path)?.trim().parse()?)
}

fn cpu_ticks(stat: &str) -> Result<u64> {
    // comm may contain spaces and parentheses. After its final ')', fields 14/15
    // (utime/stime) are the 12th/13th tokens; no other fields need traversing.
    let mut fields = stat
        .rsplit_once(')')
        .ok_or("malformed /proc stat")?
        .1
        .split_whitespace()
        .skip(11);
    let user = fields.next().ok_or("missing utime")?.parse::<u64>()?;
    let system = fields.next().ok_or("missing stime")?.parse::<u64>()?;
    user.checked_add(system)
        .ok_or_else(|| "CPU counter overflow".into())
}

struct Counters {
    cpu_ticks: u64,
    compute_ns: u64,
    vram: u64,
    gtt: u64,
    device_used: u64,
}

struct Sensors {
    stat: String,
    drm: String,
    device: String,
    ticks_per_second: f64,
}

impl Sensors {
    fn new(pid: u32) -> Result<Self> {
        let clock = Command::new("getconf").arg("CLK_TCK").output()?;
        if !clock.status.success() {
            return Err("getconf CLK_TCK failed".into());
        }
        let ticks_per_second = String::from_utf8(clock.stdout)?.trim().parse()?;
        for entry in fs::read_dir(format!("/proc/{pid}/fdinfo"))? {
            let path = entry?.path();
            let text = match fs::read_to_string(&path) {
                Ok(text) => text,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    eprintln!(
                        "fd closed while locating the AMD device: {}",
                        path.display()
                    );
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            if text
                .lines()
                .any(|line| line.starts_with("drm-driver:") && line.ends_with("amdgpu"))
            {
                return Ok(Self {
                    stat: format!("/proc/{pid}/stat"),
                    drm: path.to_string_lossy().into_owned(),
                    device: format!("/sys/bus/pci/devices/{}", field(&text, "drm-pdev")?),
                    ticks_per_second,
                });
            }
        }
        Err("llama-server has no AMD DRM client; GPU execution is not established".into())
    }

    fn read(&self) -> Result<Counters> {
        let stat = fs::read_to_string(&self.stat)?;
        let drm = fs::read_to_string(&self.drm)?;
        Ok(Counters {
            cpu_ticks: cpu_ticks(&stat)?,
            compute_ns: number(&drm, "drm-engine-compute")?,
            vram: number(&drm, "drm-memory-vram")? * 1024,
            gtt: number(&drm, "drm-memory-gtt")? * 1024,
            device_used: read_number(format!("{}/mem_info_vram_used", self.device))?,
        })
    }
}

fn rust_check(
    base: &str,
    directory: &Path,
    name: &str,
    specification: &str,
    tests: &str,
) -> Result<Value> {
    let prompt = format!(
        "Implement the following in Rust 2024 using only the standard library. Return only Rust source, without explanations, main, or tests. {specification}"
    );
    let response = request(base, &prompt, 1536, json!({}))?;
    save(&directory.join(format!("{name}-response.json")), &response)?;
    let source = match content(&response).and_then(rust_source) {
        Ok(source) => source,
        Err(error) => return Ok(model_check(name, Err(error))),
    };
    let file = directory.join(format!("{name}.rs"));
    let binary = directory.join(name);
    fs::write(&file, format!("{source}\n{tests}"))?;
    let compiled = Command::new("rustc")
        .args(["--edition=2024", "--test", "--crate-name", "generated_test"])
        .arg(&file)
        .arg("-o")
        .arg(&binary)
        .output()?;
    fs::write(
        directory.join(format!("{name}-compile.log")),
        &compiled.stderr,
    )?;
    if !compiled.status.success() {
        return Ok(json!({"name": name, "passed": false, "stage": "compilation"}));
    }
    let tested = Command::new("timeout").arg("5s").arg(&binary).output()?;
    fs::write(
        directory.join(format!("{name}-test.log")),
        [&tested.stdout[..], &tested.stderr[..]].concat(),
    )?;
    Ok(json!({"name": name, "passed": tested.status.success(), "stage": "execution"}))
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 5 {
        return Err("usage: LABEL BASE_URL SERVER_PID RESULTS_DIRECTORY".into());
    }
    let label = &args[1];
    if label.is_empty()
        || label == "."
        || label == ".."
        || !label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    {
        return Err("label must be a safe filename".into());
    }
    let base = &args[2];
    let pid: u32 = args[3].parse()?;
    let directory = Path::new(&args[4]).join(label);
    fs::create_dir_all(&args[4])?;
    fs::create_dir(&directory).map_err(|error| {
        format!(
            "cannot create new benchmark run {}: {error}",
            directory.display()
        )
    })?;
    let warmup = request(base, "Reply with OK.", 8, json!({}))?;
    save(&directory.join("warmup.json"), &warmup)?;
    let command = String::from_utf8(fs::read(format!("/proc/{pid}/cmdline"))?)?;
    save(
        &directory.join("server.json"),
        &json!({
            "pid": pid, "base_url": base,
            "command": command.split_terminator('\0').collect::<Vec<_>>(),
            "system_fingerprint": warmup["system_fingerprint"],
        }),
    )?;
    let sensors = Sensors::new(pid)?;
    let mut decode = Vec::new();
    for run in 0..3 {
        let before = sensors.read()?;
        let started = Instant::now();
        let response = request(
            base,
            "List the integers from 1 to 500, separated by commas, without explanations.",
            128,
            json!({}),
        )?;
        let seconds = started.elapsed().as_secs_f64();
        let after = sensors.read()?;
        let gpu_seconds = after
            .compute_ns
            .checked_sub(before.compute_ns)
            .ok_or("GPU counter reset")? as f64
            / 1e9;
        let cpu_seconds = after
            .cpu_ticks
            .checked_sub(before.cpu_ticks)
            .ok_or("CPU counter reset")? as f64
            / sensors.ticks_per_second;
        let tps = response["timings"]["predicted_per_second"]
            .as_f64()
            .ok_or("missing decode timing")?;
        save(&directory.join(format!("decode-{run}.json")), &response)?;
        let sample = json!({
            "tokens_per_second": tps, "wall_seconds": seconds, "cpu_cores": cpu_seconds / seconds,
            "gpu_compute_duty": gpu_seconds / seconds, "vram_bytes": after.vram,
            "gtt_bytes": after.gtt, "device_vram_used_bytes": after.device_used,
            "timings": response["timings"],
        });
        eprintln!(
            "{label} decode {run}: {tps:.2} tokens/s, {:.2} CPU cores, {:.1}% GPU compute duty",
            cpu_seconds / seconds,
            gpu_seconds / seconds * 100.0
        );
        decode.push(sample);
    }
    // Preserve speed evidence even if a later quality check or the backend fails.
    save(
        &directory.join("decode-summary.json"),
        &json!({"label": label, "samples": decode}),
    )?;
    let mut quality = Vec::new();
    for (name, specification, tests) in [
        (
            "intervals",
            "pub fn merge_intervals(intervals: Vec<(i64,i64)>) -> Vec<(i64,i64)>. Input ranges have start <= end. Return sorted, merged overlapping or endpoint-touching ranges. Handle empty input and extreme i64 values without overflow.",
            r#"
#[test] fn cases() {
 assert_eq!(merge_intervals(vec![]), vec![]);
 assert_eq!(merge_intervals(vec![(8,10),(1,4),(4,6),(2,3)]), vec![(1,6),(8,10)]);
 assert_eq!(merge_intervals(vec![(i64::MAX,i64::MAX),(i64::MIN,i64::MIN)]), vec![(i64::MIN,i64::MIN),(i64::MAX,i64::MAX)]);
 assert_eq!(merge_intervals(vec![(0,0),(0,0),(0,1)]), vec![(0,1)]);
 assert_eq!(merge_intervals(vec![(1,2),(3,4)]), vec![(1,2),(3,4)]);
}"#,
        ),
        (
            "shell-words",
            "pub fn split_shell_words(input: &str) -> Result<Vec<String>, &'static str>. Unicode whitespace separates words outside quotes. Single quotes preserve all characters literally. Double quotes preserve whitespace and allow backslash to escape any next character. Outside quotes, backslash also escapes any next character. Adjacent quoted and unquoted fragments join one word; empty quoted words count. Return Err for an unclosed quote or trailing escape. Do not expand variables or implement other shell syntax.",
            r##"
#[test] fn cases() {
 assert_eq!(split_shell_words(""), Ok(vec![]));
 assert_eq!(split_shell_words("a\u{2003}b"), Ok(vec!["a".into(),"b".into()]));
 assert_eq!(split_shell_words("a\\ b c"), Ok(vec!["a b".into(),"c".into()]));
 assert_eq!(split_shell_words("'' \"\""), Ok(vec!["".into(),"".into()]));
 assert_eq!(split_shell_words("a''b 'c d'\"e f\""), Ok(vec!["ab".into(),"c de f".into()]));
 assert_eq!(split_shell_words(r#"'a\b' "c\"d""#), Ok(vec![r#"a\b"#.into(),"c\"d".into()]));
 assert!(split_shell_words("'x").is_err());
 assert!(split_shell_words("\"x").is_err());
 assert!(split_shell_words("x\\").is_err());
 assert!(split_shell_words("\"x\\").is_err());
}"##,
        ),
        (
            "topological-sort",
            "pub fn topo_sort(node_count: usize, edges: &[(usize,usize)]) -> Result<Vec<usize>, &'static str>. Nodes are 0..node_count. Edge (u,v) requires u before v. Return the lexicographically smallest valid topological order, or Err for a cycle or an out-of-range node. Duplicate edges are allowed.",
            r#"
#[test] fn cases() {
 assert_eq!(topo_sort(0,&[]), Ok(vec![]));
 assert_eq!(topo_sort(4,&[(2,3),(0,3)]), Ok(vec![0,1,2,3]));
 assert_eq!(topo_sort(4,&[(2,0),(0,1)]), Ok(vec![2,0,1,3]));
 assert_eq!(topo_sort(3,&[(0,1),(0,1)]), Ok(vec![0,1,2]));
 assert!(topo_sort(2,&[(0,1),(1,0)]).is_err());
 assert!(topo_sort(2,&[(1,1)]).is_err());
 assert!(topo_sort(2,&[(2,0)]).is_err());
 assert!(topo_sort(0,&[(0,0)]).is_err());
}"#,
        ),
    ] {
        eprintln!("{label} quality: {name}");
        quality.push(rust_check(base, &directory, name, specification, tests)?);
    }
    let response = request(
        base,
        "Return only JSON, no markdown: an object with host='127.0.0.1', port=8080, tags=['local','gpu'], and enabled=true.",
        128,
        json!({}),
    )?;
    save(&directory.join("json-response.json"), &response)?;
    quality.push(model_check(
        "json",
        content(&response).and_then(|text| {
            Ok(serde_json::from_str::<Value>(text)?
                == json!({
                    "host":"127.0.0.1","port":8080,"tags":["local","gpu"],"enabled":true
                }))
        }),
    ));
    let response = request(
        base,
        "Call lookup_timezone for Paris. Use the tool instead of answering directly.",
        128,
        json!({
            "tools": [{"type":"function","function":{"name":"lookup_timezone","description":"Get a city's timezone","parameters":{"type":"object","properties":{"city":{"type":"string"}},"required":["city"],"additionalProperties":false}}}],
            "tool_choice":"required"
        }),
    )?;
    save(&directory.join("tool-response.json"), &response)?;
    quality.push(tool_check(&response));
    let mut prompt = String::from("Read these records and answer the question after them.\n");
    for i in 0..512 {
        prompt.push_str(&format!("record-{i:04} = {}\n", (i * 37 + 11) % 9973));
    }
    prompt.push_str("\nWhat is the value of record-0353? Return only its integer value.");
    let response = request(base, &prompt, 16, json!({}))?;
    save(&directory.join("retrieval-response.json"), &response)?;
    let mut retrieval = model_check(
        "context-retrieval",
        content(&response)
            .and_then(|text| Ok(text.trim().parse::<u64>()? == (353 * 37 + 11) % 9973)),
    );
    retrieval["timings"] = response["timings"].clone();
    quality.push(retrieval);
    let mut speeds: Vec<_> = decode
        .iter()
        .map(|value| value["tokens_per_second"].as_f64().expect("recorded speed"))
        .collect();
    speeds.sort_by(f64::total_cmp);
    let passed = quality
        .iter()
        .filter(|check| check["passed"] == true)
        .count();
    let summary = json!({"label":label,"decode_median_tokens_per_second":speeds[1],"decode":decode,"quality_passed":passed,"quality_total":quality.len(),"quality":quality});
    save(&directory.join("summary.json"), &summary)?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_cpu_fields_ignore_the_command_name_and_trailing_fields() {
        assert_eq!(
            cpu_ticks("123 (name with ) parentheses) R 1 2 3 4 5 6 7 8 9 10 41 59 ignored")
                .unwrap(),
            100
        );
        assert!(cpu_ticks("123 (short) R 1").is_err());
        assert!(cpu_ticks("no command delimiter").is_err());
        assert!(cpu_ticks("123 (overflow) R 1 2 3 4 5 6 7 8 9 10 18446744073709551615 1").is_err());
    }

    #[test]
    fn rust_source_accepts_plain_or_complete_fences_and_rejects_incomplete_ones() {
        for text in [
            " fn f() {} ",
            "```rust\nfn f() {}\n```",
            "```\nfn f() {}\n```",
        ] {
            assert_eq!(rust_source(text).unwrap(), "fn f() {}");
        }
        assert!(rust_source("```rust\nfn f() {}").is_err());
    }

    #[test]
    fn bad_model_answers_are_failures_with_diagnostics_not_run_errors() {
        for response in [
            json!({}),
            json!({"choices": [{"message": {"tool_calls": [
                {"function": {"name": "lookup_timezone", "arguments": "not JSON"}}
            ]}}]}),
        ] {
            let check = tool_check(&response);
            assert_eq!(check["passed"], false);
            assert!(check["error"].is_string());
        }
        let call =
            json!({"function": {"name": "lookup_timezone", "arguments": "{\"city\":\"Paris\"}"}});
        assert_eq!(
            tool_check(&json!({"choices": [{"message": {"tool_calls": [call.clone()]}}]}))["passed"],
            true
        );
        assert_eq!(
            tool_check(&json!({"choices": [{"message": {"tool_calls": [call.clone(), call]}}]}))["passed"],
            false
        );
    }
}
