use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
struct CommandMetadata {
    description: String,
}

#[derive(Serialize)]
struct SkillMetadata<'a> {
    name: &'a str,
    description: &'a str,
}

struct Adapter {
    name: String,
    contents: String,
}

fn adapter(command: &Path) -> Result<Adapter> {
    let name = command
        .file_stem()
        .and_then(|name| name.to_str())
        .context("Command name is not UTF-8")?;
    ensure!(
        name.len() <= 64
            && name.split('-').all(|part| !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())),
        "Command name is not a valid skill name: {name}"
    );
    let source = crate::util::read(command)?;
    let mut lines = source.lines();
    ensure!(
        lines.next() == Some("---") && lines.any(|line| line == "---"),
        "Command needs YAML frontmatter with a description"
    );
    let metadata = CommandMetadata::deserialize(
        serde_yaml::Deserializer::from_str(&source)
            .next()
            .context("Command frontmatter is missing")?,
    )?;
    ensure!(
        !metadata.description.trim().is_empty(),
        "Command description is empty"
    );
    let header = serde_yaml::to_string(&SkillMetadata {
        name,
        description: &metadata.description,
    })?;
    let contents = format!(
        "---\n{header}---\n\nOnly run this command when the user explicitly requests the operation.\n\n\
         Read and follow `{}`.\n\n\
         The model and agent frontmatter configures OpenCode, not this agent.\n\
         Treat $ARGUMENTS as the instructions supplied with this skill invocation.\n",
        crate::util::path(command)?
    );
    Ok(Adapter {
        name: name.to_owned(),
        contents,
    })
}

fn sync(home: &Path, config: &Path) -> Result<usize> {
    fn output(base: &Path, sources: &[PathBuf]) -> Result<PathBuf> {
        fs::create_dir_all(base)?;
        let destination = fs::canonicalize(base)?.join("skills");
        for source in sources {
            ensure!(
                !destination.starts_with(source) && !source.starts_with(&destination),
                "Generated skills overlap OpenCode configuration: {}",
                source.display()
            );
        }
        Ok(destination)
    }

    fn exists_as(path: &Path, kind: fn(&fs::Metadata) -> bool) -> Result<bool> {
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                ensure!(
                    kind(&metadata),
                    "Unexpected output type at {}",
                    path.display()
                );
                Ok(true)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error).with_context(|| format!("Inspect {}", path.display())),
        }
    }

    ensure!(
        home.is_absolute() && config.is_absolute(),
        "Home and config paths must be absolute"
    );
    let opencode = config.join("opencode");
    let commands = opencode.join("commands");
    let skills = opencode.join("skills");
    let mut sources = [&opencode, &commands, &skills]
        .map(fs::canonicalize)
        .into_iter()
        .collect::<std::io::Result<Vec<_>>>()
        .context("OpenCode configuration is missing; apply the dotfiles first")?;
    let mut adapters = Vec::new();
    for entry in fs::read_dir(&commands)? {
        let entry = entry?;
        let command = entry.path();
        if entry.file_type()?.is_dir() || command.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        ensure!(
            fs::metadata(&command)
                .with_context(|| format!("Inspect command {}", command.display()))?
                .is_file(),
            "OpenCode command must be a regular file: {}",
            command.display()
        );
        sources.push(
            fs::canonicalize(&command)
                .with_context(|| format!("Resolve command {}", command.display()))?,
        );
        adapters.push(
            adapter(&command).with_context(|| format!("Export command {}", command.display()))?,
        );
    }
    let count = adapters.len();
    let destination = output(&home.join(".codex"), &sources)?;
    exists_as(&destination, fs::Metadata::is_dir)?;
    let mut updates = Vec::new();
    for Adapter { name, contents } in adapters {
        let directory = destination.join(name);
        exists_as(&directory, fs::Metadata::is_dir)?;
        let file = directory.join("SKILL.md");
        if exists_as(&file, fs::Metadata::is_file)?
            && fs::read(&file).with_context(|| format!("Read {}", file.display()))?
                == contents.as_bytes()
        {
            continue;
        }
        updates.push((directory, contents));
    }
    let shared = output(&home.join(".agents"), &sources)?;
    let update_link = match fs::symlink_metadata(&shared) {
        Ok(metadata) => {
            ensure!(
                metadata.file_type().is_symlink(),
                "Shared skill path is not a symbolic link: {}",
                shared.display()
            );
            fs::read_link(&shared)? != skills
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(error) => return Err(error.into()),
    };
    if update_link {
        let staged =
            tempfile::tempdir_in(shared.parent().context("Shared skills have no parent")?)?;
        let link = staged.path().join("skills");
        symlink(&skills, &link)?;
        fs::rename(link, shared)?;
    }
    for (directory, contents) in updates {
        fs::create_dir_all(&directory)?;
        crate::util::atomic_write(&directory.join("SKILL.md"), contents.as_bytes())?;
    }
    Ok(count)
}

pub fn run() -> Result<()> {
    let home = env::var_os("HOME").context("HOME is missing")?;
    let home = Path::new(&home);
    let config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    let count = sync(home, &config)?;
    println!("Synced {count} local command adapters; shared skills use OpenCode's directory");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    fn source(config: &Path, command: &str, contents: &str) {
        fs::create_dir_all(config.join("opencode/skills")).unwrap();
        fs::create_dir_all(config.join("opencode/commands")).unwrap();
        fs::write(config.join("opencode/commands").join(command), contents).unwrap();
    }

    #[test]
    fn adapters_reference_live_prompts_without_copying_harness_settings() {
        let home = tempfile::tempdir().unwrap();
        let config = home.path().join("custom-config");
        let original = "---\ndescription: Commit on explicit request\nmodel: opencode-only/model\nagent: build\n---\n\nCanonical prompt body\n";
        source(&config, "commit.md", original);
        let bundled = home.path().join(".codex/skills/.system/owned.txt");
        fs::create_dir_all(bundled.parent().unwrap()).unwrap();
        fs::write(&bundled, "Codex-owned").unwrap();
        assert_eq!(sync(home.path(), &config).unwrap(), 1);
        let exported =
            fs::read_to_string(home.path().join(".codex/skills/commit/SKILL.md")).unwrap();
        let (header, body) = exported
            .strip_prefix("---\n")
            .unwrap()
            .split_once("\n---\n")
            .unwrap();
        let metadata: serde_yaml::Value = serde_yaml::from_str(header).unwrap();
        assert_eq!(metadata["name"], "commit");
        assert_eq!(metadata["description"], "Commit on explicit request");
        assert!(metadata.get("model").is_none());
        assert!(metadata.get("agent").is_none());
        assert!(
            body.contains("Only run this command when the user explicitly requests the operation.")
        );
        assert!(
            body.contains(crate::util::path(&config.join("opencode/commands/commit.md")).unwrap())
        );
        assert!(!body.contains("Canonical prompt body"));
        assert_eq!(
            fs::read_to_string(config.join("opencode/commands/commit.md")).unwrap(),
            original
        );
        assert_eq!(fs::read_to_string(bundled).unwrap(), "Codex-owned");
        assert_eq!(
            fs::canonicalize(home.path().join(".agents/skills")).unwrap(),
            config.join("opencode/skills")
        );
        let adapter_file =
            fs::File::open(home.path().join(".codex/skills/commit/SKILL.md")).unwrap();
        let shared_link = fs::symlink_metadata(home.path().join(".agents/skills")).unwrap();
        assert_eq!(sync(home.path(), &config).unwrap(), 1);
        assert_eq!(
            adapter_file.metadata().unwrap().ino(),
            fs::metadata(home.path().join(".codex/skills/commit/SKILL.md"))
                .unwrap()
                .ino()
        );
        assert_eq!(
            shared_link.ino(),
            fs::symlink_metadata(home.path().join(".agents/skills"))
                .unwrap()
                .ino()
        );
    }

    #[test]
    fn live_config_links_follow_source_changes_without_regenerating_adapters() {
        let home = tempfile::tempdir().unwrap();
        let config = home.path().join(".config");
        fs::create_dir(&config).unwrap();
        let first = home.path().join("first");
        let second = home.path().join("second");
        source(
            &first,
            "commit.md",
            "---\ndescription: Commit\n---\nFirst prompt\n",
        );
        source(
            &second,
            "commit.md",
            "---\ndescription: Commit\n---\nSecond prompt\n",
        );
        symlink(first.join("opencode"), config.join("opencode")).unwrap();
        assert_eq!(sync(home.path(), &config).unwrap(), 1);
        let logical_command = config.join("opencode/commands/commit.md");
        let exported =
            fs::read_to_string(home.path().join(".codex/skills/commit/SKILL.md")).unwrap();
        assert!(exported.contains(crate::util::path(&logical_command).unwrap()));
        assert!(!exported.contains(crate::util::path(&first).unwrap()));
        fs::remove_file(config.join("opencode")).unwrap();
        symlink(second.join("opencode"), config.join("opencode")).unwrap();
        assert!(
            fs::read_to_string(logical_command)
                .unwrap()
                .contains("Second prompt")
        );
        assert_eq!(
            fs::canonicalize(home.path().join(".agents/skills")).unwrap(),
            second.join("opencode/skills")
        );
    }

    #[test]
    fn command_metadata_accepts_crlf_without_parsing_the_markdown_body() {
        let home = tempfile::tempdir().unwrap();
        let config = home.path().join(".config");
        let path = config.join("opencode/commands/commit.md");
        for newline in ["\n", "\r\n"] {
            source(
                &config,
                "commit.md",
                &format!(
                    "---{newline}description: 'Commit: on request'{newline}---{newline}: [}} not YAML{newline}"
                ),
            );
            let exported = adapter(&path).unwrap().contents;
            assert!(exported.contains("Commit: on request"));
            assert!(!exported.contains("not YAML"));
        }
        source(
            &config,
            "commit.md",
            "---\ndescription: Missing closing marker\n",
        );
        assert!(adapter(&path).is_err());
    }

    #[test]
    fn output_aliases_cannot_modify_canonical_prompts() {
        for alias in [
            ".agents",
            ".codex",
            ".codex/skills",
            ".codex/skills/commit",
            ".codex/skills/commit/SKILL.md",
        ] {
            let home = tempfile::tempdir().unwrap();
            let config = home.path().join(".config");
            let original = "---\ndescription: Commit\n---\nCanonical prompt\n";
            source(&config, "commit.md", original);
            let target = config.join("opencode/skills");
            let file = target.join("SKILL.md");
            fs::write(&file, "Canonical skill").unwrap();
            let output = home.path().join(alias);
            fs::create_dir_all(output.parent().unwrap()).unwrap();
            symlink(
                if alias.ends_with("SKILL.md") {
                    &file
                } else {
                    &target
                },
                &output,
            )
            .unwrap();
            assert!(sync(home.path(), &config).is_err(), "accepted {alias}");
            assert_eq!(fs::read_to_string(file).unwrap(), "Canonical skill");
            assert_eq!(
                fs::read_to_string(config.join("opencode/commands/commit.md")).unwrap(),
                original
            );
            if alias == ".agents" {
                assert!(!home.path().join(".codex/skills").exists());
            } else {
                assert!(!home.path().join(".agents").exists());
            }
        }
    }

    #[test]
    fn invalid_commands_are_reported_before_publishing_any_adapters() {
        for (name, contents) in [
            ("good.md", "---\ndescription: []\n---\nbody\n"),
            ("good.md", "---\ndescription: ''\n---\nbody\n"),
            ("good.md", "Missing frontmatter\n"),
            ("Bad-name.md", "---\ndescription: Valid\n---\nbody\n"),
        ] {
            let home = tempfile::tempdir().unwrap();
            let config = home.path().join(".config");
            source(&config, "commit.md", "---\ndescription: Valid\n---\nbody\n");
            source(&config, name, contents);
            let error = sync(home.path(), &config).unwrap_err();
            assert!(format!("{error:#}").contains(name));
            assert!(!home.path().join(".codex").exists());
            assert!(!home.path().join(".agents").exists());
        }
    }

    #[test]
    fn command_symlinks_cannot_point_back_into_generated_adapters() {
        let home = tempfile::tempdir().unwrap();
        let config = home.path().join(".config");
        let original = "---\ndescription: Commit\n---\nCanonical prompt\n";
        source(&config, "commit.md", original);
        let command = config.join("opencode/commands/commit.md");
        let generated = home.path().join(".codex/skills/commit/SKILL.md");
        fs::create_dir_all(generated.parent().unwrap()).unwrap();
        fs::rename(&command, &generated).unwrap();
        symlink(&generated, &command).unwrap();

        let error = sync(home.path(), &config).unwrap_err();
        assert!(error.to_string().contains("overlap OpenCode"));
        assert_eq!(fs::read_to_string(&command).unwrap(), original);
        assert_eq!(fs::read_to_string(&generated).unwrap(), original);
        assert!(!home.path().join(".agents").exists());
    }

    #[test]
    fn regular_command_symlinks_work_but_special_files_are_rejected() {
        let home = tempfile::tempdir().unwrap();
        let config = home.path().join(".config");
        let original = "---\ndescription: Commit\n---\nCanonical prompt\n";
        source(&config, "commit.md", original);
        let command = config.join("opencode/commands/commit.md");
        let external = home.path().join("external.md");
        fs::rename(&command, &external).unwrap();
        symlink(&external, &command).unwrap();
        assert_eq!(sync(home.path(), &config).unwrap(), 1);
        assert_eq!(fs::read_to_string(&external).unwrap(), original);

        fs::remove_file(&command).unwrap();
        let _socket = std::os::unix::net::UnixListener::bind(&command).unwrap();
        let before = fs::read(home.path().join(".codex/skills/commit/SKILL.md")).unwrap();
        let error = sync(home.path(), &config).unwrap_err();
        assert!(error.to_string().contains("regular file"));
        assert_eq!(
            fs::read(home.path().join(".codex/skills/commit/SKILL.md")).unwrap(),
            before
        );
    }
}
