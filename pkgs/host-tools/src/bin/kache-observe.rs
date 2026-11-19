//! Read-only, compressed observations for prospective cache-policy reviews.
use anyhow::{Context, Result, ensure};
use clap::Parser;
use flate2::{Compression, write::GzEncoder};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    cache: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    kache: PathBuf,
    #[arg(long)]
    config: PathBuf,
}

const QUERY: &str = r#"
BEGIN;
SELECT '@entries';
SELECT json_array(cache_key,crate_name,size,created_at,last_accessed,hit_count,
  compile_time_ms,key_schema,profile) FROM entries;
SELECT '@entry_blobs';
SELECT json_array(cache_key,hash,refs) FROM entry_blobs;
SELECT '@blobs';
SELECT json_array(hash,size,refcount) FROM blobs;
SELECT '@evictions';
SELECT json_array(cache_key,evicted_at,policy,size,hit_count,idle_hours,
  compile_time_ms,demanded_at,shadow_policy,shadow_would_evict) FROM eviction_tombstones;
SELECT '@shadow';
SELECT json_array(cache_key,swept_at,shadow_policy,size,hit_count,compile_time_ms)
  FROM shadow_victims;
SELECT '@targets';
SELECT json_array(path,workspace_root,first_seen,last_seen) FROM target_roots;
COMMIT;
"#;

const MAX_BYTES: u64 = 512 << 20;
const MAX_FILES: usize = 60;
const PENDING: &str = ".observation-in-progress";

fn command(program: &str, args: &[&str]) -> Result<String> {
    let result = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("running {program}"))?;
    ensure!(
        result.status.success(),
        "{program} failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(String::from_utf8(result.stdout)?)
}

fn write_index(cache: &Path, writer: &mut impl Write) -> Result<()> {
    let db = cache.join("index.db");
    let mut child = Command::new("sqlite3")
        .args([
            "-readonly",
            db.to_str().context("non-UTF8 cache path")?,
            ".timeout 1000",
            QUERY,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let result = (|| -> Result<()> {
        let output = child.stdout.take().context("SQLite stdout missing")?;
        let mut reader = BufReader::new(output);
        let tables = [
            "entries",
            "entry_blobs",
            "blobs",
            "evictions",
            "shadow",
            "targets",
        ];
        let mut table = 0;
        let mut first = true;
        let mut line = String::new();
        writer.write_all(b"{")?;
        while reader.read_line(&mut line)? != 0 {
            let row = line.trim_end();
            if let Some(name) = row.strip_prefix('@') {
                ensure!(
                    tables.get(table) == Some(&name),
                    "unexpected SQLite table: {name}"
                );
                if table > 0 {
                    writer.write_all(b"],")?;
                }
                serde_json::to_writer(&mut *writer, name)?;
                writer.write_all(b":[")?;
                table += 1;
                first = true;
            } else {
                ensure!(
                    table > 0 && row.starts_with('[') && row.ends_with(']'),
                    "invalid SQLite row"
                );
                if !first {
                    writer.write_all(b",")?;
                }
                writer.write_all(row.as_bytes())?;
                first = false;
            }
            line.clear();
        }
        ensure!(table == tables.len(), "incomplete SQLite snapshot");
        writer.write_all(b"]}")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    result?;
    ensure!(
        status.success(),
        "read-only SQLite snapshot failed: {status}"
    );
    Ok(())
}

fn read_bounded(path: &Path, limit: u64) -> Result<String> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "{} exceeds read bound",
        path.display()
    );
    Ok(String::from_utf8(bytes)?)
}

fn observation_files(root: &Path) -> Result<Vec<(PathBuf, u64)>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let owned = name
            .strip_prefix("observation-")
            .and_then(|s| s.strip_suffix(".json.gz"))
            .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()));
        if owned && entry.file_type()?.is_file() {
            files.push((entry.path(), entry.metadata()?.len()));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

fn cache_processes() -> Result<Vec<Value>> {
    let mut processes = Vec::new();
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        // Other users' processes and processes exiting during the scan are normal.
        let executable = match fs::read_link(entry.path().join("exe")) {
            Ok(path) => path,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                ) =>
            {
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        if !matches!(
            executable.file_name().and_then(|n| n.to_str()),
            Some("kache" | "kache-engine")
        ) {
            continue;
        }
        let arguments = match fs::read(entry.path().join("cmdline")) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        let arguments: Vec<_> = arguments.split(|byte| *byte == 0).collect();
        let daemon = arguments
            .windows(2)
            .any(|pair| pair == [b"daemon".as_slice(), b"run".as_slice()]);
        processes.push(json!({"pid":pid,"executable":executable,"daemon":daemon}));
    }
    Ok(processes)
}

fn retain(root: &Path, newest: &Path, max_bytes: u64, max_files: usize) -> Result<()> {
    let files = observation_files(root)?;
    let mut bytes: u64 = files.iter().map(|(_, size)| size).sum();
    let mut count = files.len();
    for (path, size) in files {
        if bytes <= max_bytes && count <= max_files {
            break;
        }
        ensure!(path != newest, "one observation exceeds retention budget");
        fs::remove_file(&path)?;
        eprintln!("retired observation {} ({size} bytes)", path.display());
        bytes -= size;
        count -= 1;
    }
    Ok(())
}

// The caller holds the output directory lock through collection and publication.
fn pending_file(output: &Path) -> Result<tempfile::NamedTempFile> {
    match fs::remove_file(output.join(PENDING)) {
        Ok(()) => eprintln!("removed interrupted observation"),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err).context("removing interrupted observation"),
    }
    Ok(tempfile::Builder::new()
        .prefix(PENDING)
        .rand_bytes(0)
        .tempfile_in(output)?)
}

fn write_extent_result(writer: &mut impl Write, extents: Result<String>) -> Result<()> {
    writer.write_all(b",\"cache_extents\":")?;
    match extents {
        Ok(extents) => serde_json::to_writer(writer, &extents)?,
        Err(error) => {
            let error = format!("{error:#}");
            eprintln!("skipping physical extent sample: {error}");
            writer.write_all(b"null,\"cache_extents_error\":")?;
            serde_json::to_writer(writer, &error)?;
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let started = Instant::now();
    let timestamp_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let cache = args.cache.canonicalize()?;
    let binary = args.kache.canonicalize()?;
    fs::create_dir_all(&args.output)?;
    let output = args.output.canonicalize()?;
    ensure!(
        !output.starts_with(&cache),
        "observations must live outside the cache"
    );
    let lock = fs::File::open(&output)?;
    lock.try_lock().context("locking observation directory")?;
    let mut temporary = pending_file(&output)?;
    let mut inventory = Vec::new();
    for entry in fs::read_dir(&cache)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        inventory.push(json!({
            "name": entry.file_name().to_string_lossy(),
            "bytes": metadata.len(),
            "directory": metadata.is_dir(),
            "modified": metadata.modified()?.duration_since(UNIX_EPOCH)?.as_secs(),
        }));
    }
    let cache_text = cache.to_str().context("non-UTF8 cache path")?;
    let mut digest = Sha256::new();
    std::io::copy(&mut fs::File::open(&binary)?, &mut digest)?;
    let observation = json!({
        "schema": 1,
        "timestamp_ms": timestamp_ms,
        "hostname": read_bounded(Path::new("/proc/sys/kernel/hostname"), 1024)?.trim(),
        "boot_id": read_bounded(Path::new("/proc/sys/kernel/random/boot_id"), 1024)?.trim(),
        "binary": binary,
        "binary_sha256": format!("{:x}", digest.finalize()),
        "version": command(args.kache.to_str().context("non-UTF8 binary path")?, &["--version"])?,
        "config": read_bounded(&args.config, 65536)?,
        "daemon": command("systemctl", &["--user","show","kache","-p","MainPID","-p","ExecMainStartTimestamp","-p","ActiveState"])?,
        "cache_processes": cache_processes()?,
        "df": command("df", &["-B1", cache_text])?,
        "gc_stats": serde_json::from_str::<Value>(&read_bounded(&cache.join("gc_stats.json"), 1 << 20)?)?,
        "files": inventory,
        "columns": {
            "entries": ["key","crate","logical_bytes","created_at","last_accessed","hits","compile_ms","key_schema","profile"],
            "entry_blobs": ["key","hash","refs"],
            "blobs": ["hash","bytes","refs"],
            "evictions": ["key","evicted_at","policy","logical_bytes","hits","idle_hours","compile_ms","demanded_at","shadow_policy","shadow_would_evict"],
            "shadow": ["key","swept_at","policy","logical_bytes","hits","compile_ms"],
            "targets": ["path","workspace","first_seen","last_seen"],
        },
        "semantics": "Immutable periodic snapshots, not a complete eviction ledger. Demand between snapshots may be censored. Index bytes are not filesystem-exclusive bytes. Compare equal-budget policies using future observations; inventory scores alone do not demonstrate a win.",
    });
    let metadata_ms = started.elapsed().as_millis();
    eprintln!("collected metadata in {metadata_ms}ms; sampling cache index");
    let name = output.join(format!("observation-{timestamp_ms}.json.gz"));
    {
        let mut encoder = GzEncoder::new(&mut temporary, Compression::fast());
        let mut metadata = serde_json::to_vec(&observation)?;
        ensure!(
            metadata.pop() == Some(b'}'),
            "observation metadata must be an object"
        );
        encoder.write_all(&metadata)?;
        encoder.write_all(b",\"index\":")?;
        let index_started = Instant::now();
        write_index(&cache, &mut encoder)?;
        let index_ms = index_started.elapsed().as_millis();
        eprintln!("collected cache index in {index_ms}ms; sampling Btrfs extents");
        // Don't hold a SQLite read transaction during a potentially slow disk walk.
        let extent_started = Instant::now();
        let extents = command(
            "timeout",
            &[
                "--verbose",
                "--kill-after=5s",
                "45s",
                "btrfs",
                "filesystem",
                "du",
                "--raw",
                "-s",
                cache_text,
            ],
        );
        write_extent_result(&mut encoder, extents)?;
        let extent_ms = extent_started.elapsed().as_millis();
        write!(
            encoder,
            ",\"stages_ms\":{{\"metadata\":{metadata_ms},\"index\":{index_ms},\"cache_extents\":{extent_ms}}},\"collection_ms\":{}}}",
            started.elapsed().as_millis()
        )?;
        encoder.finish()?.flush()?;
    }
    ensure!(
        temporary.as_file().metadata()?.len() <= MAX_BYTES,
        "one observation exceeds retention budget"
    );
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(&name)?;
    retain(&output, &name, MAX_BYTES, MAX_FILES)?;
    eprintln!(
        "published observation in {}ms",
        started.elapsed().as_millis()
    );
    println!("{}", name.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_open_never_creates_a_missing_index() {
        let dir = tempfile::tempdir().unwrap();
        assert!(write_index(dir.path(), &mut Vec::new()).is_err());
        assert!(!dir.path().join("index.db").exists());
    }

    #[test]
    fn failed_extent_scan_keeps_index_with_explicit_unavailable_sample() {
        let mut bytes = b"{\"index\":{\"entries\":[]}".to_vec();
        write_extent_result(&mut bytes, Err(anyhow::anyhow!("test scan timeout"))).unwrap();
        bytes.push(b'}');
        let parsed: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed["index"]["entries"], json!([]));
        assert_eq!(parsed["cache_extents"], Value::Null);
        assert_eq!(parsed["cache_extents_error"], "test scan timeout");
    }

    #[test]
    fn streamed_snapshot_preserves_empty_tables_and_escaped_values() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("index.db");
        command("sqlite3", &[db.to_str().unwrap(), r#"
CREATE TABLE entries(cache_key,crate_name,size,created_at,last_accessed,hit_count,compile_time_ms,key_schema,profile);
CREATE TABLE entry_blobs(cache_key,hash,refs);
CREATE TABLE blobs(hash,size,refcount);
CREATE TABLE eviction_tombstones(cache_key,evicted_at,policy,size,hit_count,idle_hours,compile_time_ms,demanded_at,shadow_policy,shadow_would_evict);
CREATE TABLE shadow_victims(cache_key,swept_at,shadow_policy,size,hit_count,compile_time_ms);
CREATE TABLE target_roots(path,workspace_root,first_seen,last_seen);
INSERT INTO blobs VALUES ('a"b', 42, 2);
INSERT INTO blobs VALUES ('c', 7, 1);
"#]).unwrap();
        let mut bytes = Vec::new();
        write_index(dir.path(), &mut bytes).unwrap();
        let parsed: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed["blobs"], json!([["a\"b", 42, 2], ["c", 7, 1]]));
        assert_eq!(parsed["entries"], json!([]));
        assert_eq!(parsed["targets"], json!([]));
        assert_eq!(parsed.as_object().unwrap().len(), 6);
    }

    #[test]
    fn retention_only_removes_old_owned_regular_files() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("observation-100.json.gz");
        let newest = dir.path().join("observation-200.json.gz");
        let unrelated = dir.path().join("notes.json.gz");
        for path in [&old, &newest, &unrelated] {
            fs::write(path, [0; 8]).unwrap();
        }
        let link = dir.path().join("observation-000.json.gz");
        std::os::unix::fs::symlink(&unrelated, &link).unwrap();
        retain(dir.path(), &newest, 8, 120).unwrap();
        assert!(!old.exists());
        assert!(newest.exists());
        assert!(unrelated.exists());
        assert!(link.is_symlink());
    }

    #[test]
    fn restarted_collection_replaces_only_its_interrupted_write() {
        let dir = tempfile::tempdir().unwrap();
        let saved = dir.path().join("observation-100.json.gz");
        fs::write(&saved, b"keep").unwrap();
        fs::write(dir.path().join(PENDING), b"partial").unwrap();
        let pending = pending_file(dir.path()).unwrap();
        assert_eq!(pending.path(), dir.path().join(PENDING));
        assert_eq!(pending.as_file().metadata().unwrap().len(), 0);
        assert_eq!(fs::read(saved).unwrap(), b"keep");
    }
}
