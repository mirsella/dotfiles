//! Codex plugin content fingerprint, as recorded in `trusted_content_hash`.
//!
//! Mirrors `codex-rs/hooks/src/engine/plugin_fingerprint.rs` so the pinned hash
//! for a released plugin can be checked when it is built.

use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

pub fn run(root: &Path) -> Result<()> {
    println!("{}", content_hash(root)?);
    Ok(())
}

pub fn content_hash(root: &Path) -> Result<String> {
    ensure!(
        fs::symlink_metadata(root)
            .with_context(|| format!("reading {}", root.display()))?
            .is_dir(),
        "plugin root {} is not a directory",
        root.display()
    );
    let mut hash = Sha256::new();
    hash.update(b"codex-plugin-bundle-v1");
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let relative = path.strip_prefix(root)?;
        if relative
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                matches!(name, "__pycache__" | ".venv")
                    || name.ends_with(".pyc")
                    || name.ends_with(".egg-info")
            })
        {
            continue;
        }
        let name = relative.as_os_str().as_encoded_bytes();
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name);
        let metadata =
            fs::symlink_metadata(&path).with_context(|| format!("reading {}", path.display()))?;
        if metadata.is_dir() {
            hash.update(b"directory");
            let mut entries = fs::read_dir(&path)?
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<Result<Vec<_>, _>>()?;
            entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
            pending.extend(entries.into_iter().rev());
        } else if metadata.is_file() {
            let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
            ensure!(
                bytes.len() as u64 == metadata.len(),
                "{} changed while fingerprinting",
                path.display()
            );
            hash.update(b"file");
            hash.update(metadata.len().to_le_bytes());
            hash.update(&bytes);
        } else {
            bail!("unsupported plugin entry type: {}", path.display());
        }
    }
    Ok(format!("sha256:{:x}", hash.finalize()))
}

#[cfg(test)]
mod tests {
    use super::content_hash;
    use std::fs;

    #[test]
    fn matches_codex_fingerprint() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("hooks/__pycache__")).unwrap();
        fs::write(root.path().join("hooks/hooks.json"), "{}\n").unwrap();
        fs::write(root.path().join("hooks/__pycache__/x.pyc"), "ignored").unwrap();
        fs::write(root.path().join("README.md"), "browser\n").unwrap();
        // Reference value from Codex's algorithm over the same tree.
        assert_eq!(
            content_hash(root.path()).unwrap(),
            "sha256:772c24a730ac0be7229ea419ec0ad7bae78ccbd619f979082f2c9ae650859c1f"
        );
    }

    #[test]
    fn rejects_symlinks() {
        let root = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("missing", root.path().join("link")).unwrap();
        assert!(content_hash(root.path()).is_err());
    }
}
