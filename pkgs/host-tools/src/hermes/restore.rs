//! Validate private backups and replace whole state directories with rollback.
use anyhow::{Context, Result, bail, ensure};
use flate2::read::MultiGzDecoder;
use std::{
    collections::{BTreeMap, btree_map::Entry},
    fs,
    io::{self, Seek},
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

pub(super) const ROOTS: [&str; 3] = ["hermes", "camofox", "hermes-browser-control"];

fn private_directory(root: &Path, prefix: &str) -> Result<tempfile::TempDir> {
    Ok(tempfile::Builder::new()
        .prefix(prefix)
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(root)?)
}

fn approved(path: &Path) -> bool {
    let mut components = path.components();
    matches!(components.next(), Some(Component::Normal(name)) if ROOTS.iter().any(|root| name == *root))
        && components.all(|component| matches!(component, Component::Normal(_)))
}

fn validate(file: &fs::File) -> Result<()> {
    let mut archive = tar::Archive::new(MultiGzDecoder::new(file));
    let mut members = BTreeMap::new();
    for entry in archive.entries()? {
        let entry = entry?;
        let path = entry.path()?.into_owned();
        ensure!(
            approved(&path),
            "Backup member escapes approved state: {}",
            path.display()
        );
        let kind = entry.header().entry_type();
        ensure!(
            kind.is_file() || kind.is_dir() || kind.is_symlink() || kind.is_hard_link(),
            "Unsupported backup member type: {}",
            path.display()
        );
        let link = entry.link_name()?.map(|name| name.into_owned());
        match members.entry(path) {
            Entry::Vacant(member) => {
                member.insert((kind, link));
            }
            Entry::Occupied(member) => {
                bail!("Duplicate backup member: {}", member.key().display());
            }
        }
    }
    // Tar iteration stops at its end marker, before gzip's checksum/trailer.
    io::copy(&mut archive.into_inner(), &mut io::sink())?;
    for root in ROOTS {
        ensure!(
            members
                .get(Path::new(root))
                .is_some_and(|(kind, _)| kind.is_dir()),
            "Backup lacks state directory {root}"
        );
    }
    for (path, (kind, link)) in &members {
        for parent in path
            .ancestors()
            .skip(1)
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            ensure!(
                members.get(parent).is_none_or(|(kind, _)| kind.is_dir()),
                "Backup member traverses a non-directory: {}",
                path.display()
            );
        }
        if kind.is_symlink() || kind.is_hard_link() {
            let link = link.as_deref().context("Backup link has no target")?;
            let mut target = if kind.is_symlink() {
                path.parent()
                    .context("Backup link has no parent")?
                    .to_path_buf()
            } else {
                PathBuf::new()
            };
            for component in link.components() {
                match component {
                    Component::Normal(name) => target.push(name),
                    Component::CurDir => continue,
                    Component::ParentDir if target.components().count() > 1 => {
                        target.pop();
                    }
                    _ => bail!("Backup link escapes approved state: {}", path.display()),
                }
                // Check before processing '..': an intermediate symlink must
                // not redirect path resolution outside the validated tree.
                ensure!(
                    members
                        .get(&target)
                        .is_none_or(|(kind, _)| !kind.is_symlink() && !kind.is_hard_link()),
                    "Backup link traverses another link: {}",
                    path.display()
                );
            }
            ensure!(
                approved(&target),
                "Backup link escapes approved state: {}",
                path.display()
            );
            if kind.is_hard_link() {
                ensure!(
                    members.get(&target).is_some_and(|(kind, _)| kind.is_file()),
                    "Backup hard link must target a regular archived file"
                );
            }
        }
    }
    Ok(())
}

pub(super) fn stage(archive: &Path, root: &Path) -> Result<tempfile::TempDir> {
    let mut file = fs::File::open(archive)?;
    validate(&file)?;
    file.rewind()?;
    let staged = private_directory(root, ".hermes-restore-")?;
    // Use the same open file we validated. GNU tar preserves ownership, ACLs
    // and xattrs; extraction never touches the live state directories.
    ensure!(
        Command::new("tar")
            .args([
                "--extract",
                "--gzip",
                "--xattrs",
                "--acls",
                "--same-owner",
                "--file=-",
                "--directory"
            ])
            .arg(staged.path())
            .stdin(Stdio::from(file))
            .status()?
            .success(),
        "Backup extraction failed; live state is unchanged"
    );
    Ok(staged)
}

pub(super) fn validate_backup(archive: &Path) -> Result<()> {
    validate(&fs::File::open(archive)?)
}

fn replace_with(
    root: &Path,
    staged: &Path,
    previous: &Path,
    rename: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<()> {
    // Validate all roots before the first rename; never replace a symlink or
    // treat a missing staged tree as an empty backup.
    let mut roots = Vec::with_capacity(ROOTS.len());
    for name in ROOTS {
        ensure!(
            fs::symlink_metadata(staged.join(name))?.is_dir(),
            "Staged state root is not a directory: {name}"
        );
        let present = match fs::symlink_metadata(root.join(name)) {
            Ok(metadata) => {
                ensure!(
                    metadata.is_dir(),
                    "Live state root is not a directory: {name}"
                );
                true
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        roots.push((name, present));
    }
    enum Change {
        Saved(&'static str),
        Installed(&'static str),
    }
    let mut changes = Vec::new();
    let result = (|| -> Result<()> {
        for (name, present) in roots {
            if present {
                rename(&root.join(name), &previous.join(name))?;
                changes.push(Change::Saved(name));
            }
            rename(&staged.join(name), &root.join(name))?;
            changes.push(Change::Installed(name));
        }
        Ok(())
    })();
    if let Err(error) = result {
        let mut failures = Vec::new();
        for change in changes.into_iter().rev() {
            let (name, from, to, action) = match change {
                Change::Installed(name) => (name, root, staged, "unpublish"),
                Change::Saved(name) => (name, previous, root, "recover"),
            };
            if let Err(error) = rename(&from.join(name), &to.join(name)) {
                failures.push(format!("{action} {name}: {error}"));
            }
        }
        return Err(error.context(if failures.is_empty() {
            "Restore publication failed; previous live state recovered".to_string()
        } else {
            format!(
                "Restore publication failed; recovery errors: {}",
                failures.join("; ")
            )
        }));
    }
    Ok(())
}

pub(super) fn replace(root: &Path, staged: tempfile::TempDir) -> Result<PathBuf> {
    // Keep both containers once publication starts: failed rollback must never
    // cause TempDir's destructor to delete the only remaining copy of a tree.
    let previous = private_directory(root, ".hermes-before-restore-")?.keep();
    let staged = staged.keep();
    replace_with(root, &staged, &previous, &mut |from, to| {
        fs::rename(from, to)
    })
    .with_context(|| {
        format!(
            "Services remain stopped; recovery trees: {} and {}",
            previous.display(),
            staged.display()
        )
    })?;
    fs::remove_dir(staged)?;
    Ok(previous)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};
    use std::{
        io::Write,
        os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    };

    fn archive(source: &Path, destination: &Path) {
        let gzip = GzEncoder::new(fs::File::create(destination).unwrap(), Compression::fast());
        let mut tar = tar::Builder::new(gzip);
        tar.follow_symlinks(false);
        for name in ROOTS {
            tar.append_dir_all(name, source.join(name)).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap();
    }

    fn state(root: &Path, value: &[u8]) {
        for name in ROOTS {
            fs::create_dir_all(root.join(name)).unwrap();
            fs::write(root.join(name).join("state"), value).unwrap();
        }
    }

    #[test]
    fn restore_replaces_state_preserves_metadata_and_retains_previous_tree() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        let live = directory.path().join("live");
        state(&source, b"saved");
        state(&live, b"current");
        fs::write(live.join("hermes/stale"), b"must disappear").unwrap();
        fs::set_permissions(
            source.join("hermes/state"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        symlink("state", source.join("hermes/link")).unwrap();
        let backup = directory.path().join("backup.tar.gz");
        archive(&source, &backup);
        let staged = stage(&backup, &live).unwrap();
        assert_eq!(fs::metadata(staged.path()).unwrap().mode() & 0o777, 0o700);
        let previous = replace(&live, staged).unwrap();
        assert!(!live.join("hermes/stale").exists());
        assert_eq!(fs::read(live.join("hermes/state")).unwrap(), b"saved");
        assert_eq!(fs::read(live.join("hermes/link")).unwrap(), b"saved");
        assert_eq!(
            fs::read(previous.join("hermes/stale")).unwrap(),
            b"must disappear"
        );
        for name in ROOTS {
            assert_eq!(
                fs::read(previous.join(name).join("state")).unwrap(),
                b"current"
            );
        }
        let restored = fs::metadata(live.join("hermes/state")).unwrap();
        let original = fs::metadata(source.join("hermes/state")).unwrap();
        assert_eq!(restored.mode() & 0o777, 0o600);
        assert_eq!(
            (restored.uid(), restored.gid()),
            (original.uid(), original.gid())
        );
        assert_eq!(fs::metadata(previous).unwrap().mode() & 0o777, 0o700);
    }

    #[test]
    fn partial_publication_rolls_back_every_state_directory() {
        let directory = tempfile::tempdir().unwrap();
        let live = directory.path().join("live");
        let staged = directory.path().join("staged");
        let previous = directory.path().join("previous");
        state(&live, b"current");
        state(&staged, b"saved");
        fs::create_dir(&previous).unwrap();
        let mut calls = 0;
        let error = replace_with(&live, &staged, &previous, &mut |from, to| {
            calls += 1;
            if calls == 4 {
                return Err(io::Error::other("injected publication failure"));
            }
            fs::rename(from, to)
        })
        .unwrap_err();
        assert!(format!("{error:#}").contains("previous live state recovered"));
        for name in ROOTS {
            assert_eq!(fs::read(live.join(name).join("state")).unwrap(), b"current");
            assert_eq!(fs::read(staged.join(name).join("state")).unwrap(), b"saved");
        }
        assert_eq!(fs::read_dir(previous).unwrap().count(), 0);
    }

    #[test]
    fn failed_rollback_retains_both_saved_and_restored_state() {
        let directory = tempfile::tempdir().unwrap();
        let live = directory.path().join("live");
        let staged = directory.path().join("staged");
        let previous = directory.path().join("previous");
        state(&live, b"current");
        state(&staged, b"saved");
        fs::create_dir(&previous).unwrap();
        let mut calls = 0;
        let error = replace_with(&live, &staged, &previous, &mut |from, to| {
            calls += 1;
            if matches!(calls, 4 | 5) {
                return Err(io::Error::other("injected rename failure"));
            }
            fs::rename(from, to)
        })
        .unwrap_err();
        let diagnostic = format!("{error:#}");
        assert!(diagnostic.contains("recovery errors: recover camofox"));
        assert!(!live.join("camofox").exists());
        assert_eq!(
            fs::read(previous.join("camofox/state")).unwrap(),
            b"current"
        );
        assert_eq!(fs::read(live.join("hermes/state")).unwrap(), b"current");
        for name in ROOTS {
            assert_eq!(fs::read(staged.join(name).join("state")).unwrap(), b"saved");
        }
    }

    #[test]
    fn corrupt_or_truncated_gzip_trailers_are_rejected_before_staging() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        state(&source, b"saved");
        let backup = directory.path().join("backup.tar.gz");
        archive(&source, &backup);
        let bytes = fs::read(&backup).unwrap();
        let staged = directory.path().join("staged");
        fs::create_dir(&staged).unwrap();
        let mut corrupt = bytes.clone();
        corrupt[bytes.len() - 8] ^= 0xff;
        for invalid in [&corrupt[..], &bytes[..bytes.len() - 4]] {
            fs::write(&backup, invalid).unwrap();
            assert!(validate_backup(&backup).is_err());
            assert!(stage(&backup, &staged).is_err());
            assert_eq!(fs::read_dir(&staged).unwrap().count(), 0);
        }
    }

    #[test]
    fn concatenated_gzip_members_share_one_archive_validation() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        state(&source, b"saved");
        fs::write(source.join("extra"), b"second gzip member").unwrap();
        let backup = directory.path().join("backup.tar.gz");
        for name in ["hermes/extra", "outside/extra"] {
            let mut tar = tar::Builder::new(Vec::new());
            for root in ROOTS {
                tar.append_dir_all(root, source.join(root)).unwrap();
            }
            let boundary = tar.get_ref().len();
            tar.append_path_with_name(source.join("extra"), name)
                .unwrap();
            let bytes = tar.into_inner().unwrap();
            let mut file = fs::File::create(&backup).unwrap();
            for part in [&bytes[..boundary], &bytes[boundary..]] {
                let mut gzip = GzEncoder::new(file, Compression::fast());
                gzip.write_all(part).unwrap();
                file = gzip.finish().unwrap();
            }
            drop(file);
            if name == "hermes/extra" {
                validate_backup(&backup).unwrap();
                let staged = stage(&backup, directory.path()).unwrap();
                assert_eq!(
                    fs::read(staged.path().join(name)).unwrap(),
                    b"second gzip member"
                );
            } else {
                assert!(validate_backup(&backup).is_err());
                assert!(stage(&backup, directory.path()).is_err());
            }
        }
    }

    #[test]
    fn archive_link_escape_and_non_directory_ancestors_are_rejected_before_extraction() {
        for target in ["/etc/passwd", "../../outside", "state/../state"] {
            let directory = tempfile::tempdir().unwrap();
            let source = directory.path().join("source");
            state(&source, b"saved");
            // The final case traverses another link before '..'.
            if target == "state/../state" {
                fs::remove_file(source.join("hermes/state")).unwrap();
                symlink("other", source.join("hermes/state")).unwrap();
            }
            symlink(target, source.join("hermes/link")).unwrap();
            let backup = directory.path().join("backup.tar.gz");
            archive(&source, &backup);
            assert!(
                stage(&backup, directory.path()).is_err(),
                "accepted {target}"
            );
        }
    }

    #[test]
    fn archived_member_below_symlink_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let backup = directory.path().join("backup.tar.gz");
        let mut tar = tar::Builder::new(GzEncoder::new(
            fs::File::create(&backup).unwrap(),
            Compression::fast(),
        ));
        for name in ROOTS {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Directory);
            header.set_size(0);
            header.set_mode(0o700);
            header.set_cksum();
            tar.append_data(&mut header, name, io::empty()).unwrap();
        }
        let mut link = tar::Header::new_gnu();
        link.set_entry_type(tar::EntryType::Symlink);
        link.set_link_name("target").unwrap();
        link.set_size(0);
        link.set_cksum();
        tar.append_data(&mut link, "hermes/link", io::empty())
            .unwrap();
        let mut file = tar::Header::new_gnu();
        file.set_size(1);
        file.set_cksum();
        tar.append_data(&mut file, "hermes/link/file", &b"x"[..])
            .unwrap();
        tar.into_inner().unwrap().finish().unwrap();
        assert!(stage(&backup, directory.path()).is_err());
    }
}
