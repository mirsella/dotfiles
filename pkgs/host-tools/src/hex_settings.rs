//! Hex reloads settings.json itself. Keep its private/device-local fields in place;
//! only an explicit, recursively checked preference allowlist crosses machines.
use crate::util;
use anyhow::{Context, Result, bail, ensure};
use clap::Subcommand;
use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Subcommand)]
pub enum Operation {
    /// Export current preferences; chezmoi re-add can then save the bundle.
    Capture {
        #[arg(long)]
        if_present: bool,
    },
    /// Three-way sync. Conflicting live and repository edits require a choice.
    Sync {
        #[arg(long)]
        dry_run: bool,
        /// Do not capture live-only edits (used by chezmoi apply and login).
        #[arg(long)]
        no_capture: bool,
        /// Resolve conflicts in favor of the portable bundle; back up live data.
        #[arg(long)]
        force_apply: bool,
        #[arg(long)]
        if_present: bool,
    },
}

struct Paths {
    live: PathBuf,
    bundle: PathBuf,
    state: PathBuf,
}

fn xdg(variable: &str, fallback: PathBuf) -> PathBuf {
    env::var_os(variable)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or(fallback)
}

pub fn run(action: Operation) -> Result<()> {
    let home = PathBuf::from(env::var_os("HOME").context("HOME is unset")?);
    let paths = Paths {
        live: xdg("XDG_DATA_HOME", home.join(".local/share")).join("hex/settings.json"),
        bundle: xdg("XDG_CONFIG_HOME", home.join(".config")).join("hex/portable-settings.json"),
        state: xdg("XDG_STATE_HOME", home.join(".local/state")).join("hex-settings"),
    };
    fs::create_dir_all(&paths.state)?;
    let lock = fs::File::create(paths.state.join("lock"))?;
    lock.lock()?;
    execute(&paths, action)
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("read {}", path.display())),
    }
}

fn parse(path: &Path, bytes: &[u8]) -> Result<Value> {
    serde_json::from_slice(bytes).with_context(|| format!("parse {}", path.display()))
}

fn write(path: &Path, value: &Value) -> Result<()> {
    fs::create_dir_all(path.parent().context("settings path has no parent")?)?;
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    util::atomic_write(path, &bytes)
}

fn policy() -> Value {
    serde_json::from_str(include_str!("hex-settings-allowlist.json"))
        .expect("embedded Hex allowlist")
}

fn select(value: &Value, rule: &Value, definitions: &Value, path: &str) -> Result<Value> {
    if let Some(name) = rule.as_str() {
        if name == "hotkey" && value.is_null() {
            return Ok(Value::Null);
        }
        if name == "vocabulary" && value.is_string() {
            return Ok(value.clone());
        }
        return select(value, &definitions[name], definitions, path);
    }
    match (value, rule) {
        (Value::Object(values), Value::Object(rules)) => {
            let mut result = serde_json::Map::new();
            for (key, child) in values {
                if let Some(rule) = rules.get(key) {
                    result.insert(
                        key.clone(),
                        select(child, rule, definitions, &format!("{path}.{key}"))?,
                    );
                }
            }
            Ok(Value::Object(result))
        }
        (Value::Array(values), Value::Array(rules)) if rules.len() == 1 => {
            let result = Value::Array(
                values
                    .iter()
                    .map(|v| select(v, &rules[0], definitions, path))
                    .collect::<Result<_>>()?,
            );
            // Arrays are replaced, not recursively merged. Never erase private or
            // newly introduced fields inside an array element as a side effect.
            ensure!(&result == value, "unreviewed fields in Hex array {path}");
            Ok(result)
        }
        (_, Value::Bool(true)) if !value.is_object() && !value.is_array() => Ok(value.clone()),
        _ => bail!("unexpected Hex setting structure at {path}"),
    }
}

fn portable(value: &Value) -> Result<Value> {
    ensure!(value.is_object(), "Hex settings must be an object");
    let policy = policy();
    let result = select(
        value,
        &policy["settings"],
        &policy["definitions"],
        "settings",
    )?;
    if let Some(model) = result
        .pointer("/transcription/model")
        .and_then(Value::as_str)
    {
        ensure!(
            !model.contains(['/', '\\']) && !model.starts_with('.'),
            "local model paths are not portable"
        );
    }
    Ok(result)
}

fn explicit_vocabulary(value: &mut Value) {
    value
        .as_object_mut()
        .expect("allowlisted vocabulary owner is an object")
        .entry("vocabulary")
        .or_insert_with(|| json!([]));
}

// Mode arrays are replaced whole, so omitted vocabulary means empty on both
// sides. Sparse object fields outside arrays still mean "leave unmanaged".
fn normalize_modes(value: &mut Value) {
    if let Some(modes) = value
        .pointer_mut("/dictation_processing/modes")
        .and_then(Value::as_array_mut)
    {
        for mode in modes {
            explicit_vocabulary(mode);
        }
    }
}

fn live_preferences(value: &Value) -> Result<Value> {
    let mut selected = portable(value)?;
    // Hex omits empty vocabulary on save. Export it explicitly so removing the
    // last term propagates, and a subsequent Hex save still compares equal.
    explicit_vocabulary(&mut selected);
    if let Some(mode) = selected.pointer_mut("/dictation_processing/default_mode") {
        explicit_vocabulary(mode);
    }
    normalize_modes(&mut selected);
    Ok(selected)
}

fn merge(target: &mut Value, source: &Value) {
    if let (Some(target), Some(source)) = (target.as_object_mut(), source.as_object()) {
        for (key, value) in source {
            merge(target.entry(key).or_insert(Value::Null), value);
        }
    } else {
        *target = source.clone();
    }
}

#[derive(Debug, PartialEq)]
enum Plan {
    Equal,
    Capture,
    Preserve,
    Apply,
}

// Compare only managed keys, without allocating filtered copies of either tree.
fn same_managed(left: &Value, right: &Value, template: &Value) -> bool {
    match (left, right, template) {
        (Value::Object(left), Value::Object(right), Value::Object(template)) => template
            .iter()
            .all(|(key, rule)| match (left.get(key), right.get(key)) {
                (Some(left), Some(right)) => same_managed(left, right, rule),
                (None, None) => true,
                _ => false,
            }),
        _ => left == right,
    }
}

fn plan(
    live: &Value,
    wanted: &Value,
    base: Option<&Value>,
    no_capture: bool,
    force: bool,
) -> Result<Plan> {
    if same_managed(live, wanted, wanted) {
        return Ok(Plan::Equal);
    }
    if force || base.is_none_or(|base| same_managed(base, live, wanted)) {
        return Ok(Plan::Apply);
    }
    if base.is_some_and(|base| same_managed(base, wanted, wanted)) {
        return Ok(if no_capture {
            Plan::Preserve
        } else {
            Plan::Capture
        });
    }
    bail!(
        "Hex preferences changed both locally and in the bundle; use hex-settings capture for live-wins, or sync --force-apply for bundle-wins"
    )
}

fn execute(paths: &Paths, action: Operation) -> Result<()> {
    let (required, if_present) = match &action {
        Operation::Capture { if_present } => (&paths.live, *if_present),
        Operation::Sync { if_present, .. } => (&paths.bundle, *if_present),
    };
    let Some(input) = read_optional(required)? else {
        ensure!(
            if_present,
            "missing Hex settings file {}",
            required.display()
        );
        return Ok(());
    };
    let input = parse(required, &input)?;
    let checkpoint = paths.state.join("baseline.json");
    match action {
        Operation::Capture { .. } => {
            let live = live_preferences(&input)?;
            eprintln!("hex-settings: Capture");
            write(&paths.bundle, &live)?;
            write(&checkpoint, &live)
        }
        Operation::Sync {
            dry_run,
            no_capture,
            force_apply,
            ..
        } => {
            let mut wanted = input;
            ensure!(
                portable(&wanted)? == wanted,
                "Hex portable bundle contains unapproved fields"
            );
            normalize_modes(&mut wanted);
            let original = read_optional(&paths.live)?;
            let mut raw = original
                .as_deref()
                .map(|bytes| parse(&paths.live, bytes))
                .transpose()?
                .unwrap_or_else(|| json!({}));
            let mut live = live_preferences(&raw)?;
            let base = read_optional(&checkpoint)?
                .map(|bytes| live_preferences(&parse(&checkpoint, &bytes)?))
                .transpose()?;
            let decision = plan(&live, &wanted, base.as_ref(), no_capture, force_apply)?;
            eprintln!(
                "hex-settings: {decision:?}{}",
                if dry_run { " (dry run)" } else { "" }
            );
            if dry_run || decision == Plan::Preserve {
                return Ok(());
            }
            match decision {
                Plan::Apply => {
                    merge(&mut raw, &wanted);
                    // Hex also saves settings: reject a stale snapshot before writing.
                    ensure!(
                        read_optional(&paths.live)? == original,
                        "Hex settings changed during sync; retry"
                    );
                    if let Some(bytes) = original {
                        util::atomic_write(&paths.state.join("before-apply.json"), &bytes)?;
                    }
                    write(&paths.live, &raw)?;
                    merge(&mut live, &wanted);
                }
                Plan::Capture => write(&paths.bundle, &live)?,
                Plan::Equal => {}
                Plan::Preserve => unreachable!(),
            }
            if base.as_ref() != Some(&live) {
                write(&checkpoint, &live)?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    fn read(path: &Path) -> Result<Value> {
        parse(path, &fs::read(path)?)
    }

    #[test]
    fn excludes_credentials_paths_and_unknown_nested_fields() -> Result<()> {
        let value = json!({"ui_language":"en", "microphone":"private mic", "migrations":["local"],
            "meetings":{"enabled":false,"output_folder":"/private","calendar":{"connected":true,"remind":true}},
            "capture":{"routing":{"jevApiKey":"secret"},"enabled":true},
            "labs":{"live_text":true,"future_secret":"secret"}});
        assert_eq!(
            portable(&value)?,
            json!({"ui_language":"en", "meetings":{"enabled":false,"calendar":{"remind":true}},
            "capture":{"enabled":true},"labs":{"live_text":true}})
        );
        assert!(portable(&json!({"transcription":{"model":"/private/model"}})).is_err());
        assert!(portable(&json!({"labs":{"live_text":{"secret":"x"}}})).is_err());
        assert!(portable(&json!({"capture":null})).is_err());
        assert!(portable(&json!({"meetings":{"calendar":null}})).is_err());
        assert_eq!(
            portable(&json!({"raw_dictation_hotkey":null}))?,
            json!({"raw_dictation_hotkey":null})
        );
        assert!(
            portable(&json!({"dictation_processing":{"modes":[{"name":"x","api_key":"secret"}]}}))
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn decisions_preserve_local_edits_and_require_conflict_resolution() -> Result<()> {
        let base = json!({"sound_effects":true});
        let local = json!({"sound_effects":false});
        let remote = json!({"sound_effects":true,"ui_language":"en"});
        assert_eq!(
            plan(&local, &base, Some(&base), true, false)?,
            Plan::Preserve
        );
        assert_eq!(
            plan(&local, &base, Some(&base), false, false)?,
            Plan::Capture
        );
        assert_eq!(plan(&base, &remote, Some(&base), true, false)?, Plan::Apply);
        assert!(plan(&local, &remote, Some(&base), false, false).is_err());
        assert_eq!(plan(&local, &remote, Some(&base), true, true)?, Plan::Apply);
        assert_eq!(plan(&local, &local, Some(&base), true, false)?, Plan::Equal);
        assert_eq!(
            plan(&remote, &base, Some(&remote), true, false)?,
            Plan::Equal
        );
        let template = json!({"labs":{"live_text":true},"vocabulary":["Hex"]});
        let extra = json!({"labs":{"live_text":true,"unmanaged":false},"vocabulary":["Hex"]});
        assert!(same_managed(&extra, &template, &template));
        assert!(!same_managed(&json!({"labs":null}), &template, &template));
        assert!(!same_managed(&json!({"labs":{}}), &template, &template));
        assert!(!same_managed(
            &json!({"labs":{"live_text":true},"vocabulary":[]}),
            &template,
            &template
        ));
        Ok(())
    }

    #[test]
    fn capture_requires_live_settings_and_repairs_a_corrupt_baseline() -> Result<()> {
        let root = tempfile::tempdir()?;
        let paths = Paths {
            live: root.path().join("settings.json"),
            bundle: root.path().join("portable.json"),
            state: root.path().to_path_buf(),
        };
        execute(&paths, Operation::Capture { if_present: true })?;
        assert!(!paths.bundle.exists());
        assert!(execute(&paths, Operation::Capture { if_present: false }).is_err());
        assert!(!paths.bundle.exists());
        let live = json!({"ui_language":"en"});
        write(&paths.live, &live)?;
        let checkpoint = paths.state.join("baseline.json");
        fs::write(&checkpoint, b"corrupt")?;
        execute(&paths, Operation::Capture { if_present: false })?;
        assert_eq!(read(&paths.bundle)?, live_preferences(&live)?);
        assert_eq!(read(&checkpoint)?, live_preferences(&live)?);
        assert!(read_optional(root.path()).is_err());
        Ok(())
    }

    #[test]
    fn round_trip_preserves_private_state_and_dry_run_writes_nothing() -> Result<()> {
        let root = tempfile::tempdir()?;
        let paths = Paths {
            live: root.path().join("settings.json"),
            bundle: root.path().join("portable.json"),
            state: root.path().to_path_buf(),
        };
        let raw = json!({"ui_language":"fr","microphone":"local", "capture":{"routing":{"secret":"keep"}}});
        write(&paths.live, &raw)?;
        execute(&paths, Operation::Capture { if_present: false })?;
        write(&paths.bundle, &json!({"ui_language":"en","capture":{}}))?;
        let sync = |dry_run| Operation::Sync {
            dry_run,
            no_capture: true,
            force_apply: false,
            if_present: false,
        };
        execute(&paths, sync(true))?;
        assert_eq!(read(&paths.live)?, raw);
        execute(&paths, sync(false))?;
        assert_eq!(read(&paths.live)?["microphone"], "local");
        assert_eq!(read(&paths.live)?["capture"]["routing"]["secret"], "keep");
        assert_eq!(read(&paths.live)?["ui_language"], "en");
        assert_eq!(read(&paths.state.join("before-apply.json"))?, raw);
        let checkpoint = paths.state.join("baseline.json");
        let inode = fs::metadata(&checkpoint)?.ino();
        execute(&paths, sync(false))?;
        assert_eq!(fs::metadata(&checkpoint)?.ino(), inode);
        assert_eq!(
            fs::metadata(&paths.live)?.permissions().mode() & 0o777,
            0o600
        );
        Ok(())
    }

    #[test]
    fn cleared_vocabulary_syncs_and_survives_hex_omitting_empty_fields() -> Result<()> {
        let root = tempfile::tempdir()?;
        let paths = Paths {
            live: root.path().join("settings.json"),
            bundle: root.path().join("portable.json"),
            state: root.path().to_path_buf(),
        };
        let original = json!({"vocabulary":["shared"],"dictation_processing":{
            "default_mode":{"name":"Global","vocabulary":["global"]},
            "modes":[{"name":"App","vocabulary":["app"]}]}});
        let cleared = json!({"dictation_processing":{
            "default_mode":{"name":"Global"},"modes":[{"name":"App"}]}});
        write(&paths.live, &cleared)?;
        execute(&paths, Operation::Capture { if_present: false })?;
        let bundle = read(&paths.bundle)?;
        for path in [
            "/vocabulary",
            "/dictation_processing/default_mode/vocabulary",
            "/dictation_processing/modes/0/vocabulary",
        ] {
            assert_eq!(bundle.pointer(path), Some(&json!([])));
        }
        // Another machine still has the old vocabulary and its old baseline.
        write(&paths.live, &original)?;
        let checkpoint = paths.state.join("baseline.json");
        write(&checkpoint, &live_preferences(&original)?)?;
        let sync = || Operation::Sync {
            dry_run: false,
            no_capture: true,
            force_apply: false,
            if_present: false,
        };
        execute(&paths, sync())?;
        assert_eq!(read(&paths.live)?, bundle);
        // Hex serializes all those empty lists back to absent fields.
        write(&paths.live, &cleared)?;
        let inode = fs::metadata(&checkpoint)?.ino();
        execute(&paths, sync())?;
        assert_eq!(read(&paths.live)?, cleared);
        assert_eq!(fs::metadata(&checkpoint)?.ino(), inode);
        Ok(())
    }

    #[test]
    fn sparse_mode_bundles_and_baselines_do_not_create_false_conflicts() -> Result<()> {
        let root = tempfile::tempdir()?;
        let paths = Paths {
            live: root.path().join("settings.json"),
            bundle: root.path().join("portable.json"),
            state: root.path().to_path_buf(),
        };
        let bundle = |language| {
            json!({"ui_language":language,
            "dictation_processing":{"modes":[{"name":"App"}]}})
        };
        let mut live = bundle("fr");
        live["vocabulary"] = json!(["local"]);
        write(&paths.live, &live)?;
        write(&paths.bundle, &bundle("en"))?;
        let sync = || Operation::Sync {
            dry_run: false,
            no_capture: true,
            force_apply: false,
            if_present: false,
        };
        execute(&paths, sync())?;
        let checkpoint = paths.state.join("baseline.json");
        let mut base = read(&checkpoint)?;
        base.pointer_mut("/dictation_processing/modes/0")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("vocabulary");
        write(&checkpoint, &base)?;
        // Only the bundle changes; absent empty lists must not look like local edits.
        write(&paths.bundle, &bundle("fr"))?;
        execute(&paths, sync())?;
        assert_eq!(read(&paths.live)?["ui_language"], "fr");
        assert_eq!(read(&paths.live)?["vocabulary"], json!(["local"]));
        write(&paths.live, &live)?;
        let inode = fs::metadata(&checkpoint)?.ino();
        execute(&paths, sync())?;
        assert_eq!(read(&paths.live)?, live);
        assert_eq!(fs::metadata(&checkpoint)?.ino(), inode);
        Ok(())
    }
}
