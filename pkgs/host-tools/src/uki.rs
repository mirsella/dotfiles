//! Refresh the already-enrolled laptop's configuration and signed UKIs.
use crate::{boot_keys::PublicKey, util};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    default: String,
    kernels: BTreeMap<String, Vec<String>>,
}

fn install_tree(source: &Path, target: &Path) -> Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let dest = target.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            fs::create_dir_all(&dest)?;
            install_tree(&entry.path(), &dest)?;
        } else {
            ensure!(kind.is_file(), "unexpected configuration symlink");
            util::run(
                "install",
                &["-m644", util::path(&entry.path())?, util::path(&dest)?],
            )?;
        }
    }
    Ok(())
}

fn kernel_image(files: &str) -> Result<&str> {
    let mut candidates = files
        .lines()
        .filter(|p| p.starts_with("/usr/lib/modules/") && p.ends_with("/vmlinuz"));
    let source = candidates.next().context("no installed kernel image")?;
    ensure!(
        candidates.next().is_none(),
        "multiple installed kernel images"
    );
    Ok(source)
}

pub fn run(config: &Path) -> Result<()> {
    ensure!(unsafe { libc::geteuid() } == 0, "run with sudo");
    unsafe { libc::umask(0o077) };
    ensure!(
        util::read("/etc/hostname")?.trim() == "laptop",
        "requires laptop"
    );
    ensure!(
        util::read("/etc/os-release")?
            .lines()
            .any(|line| line == "ID=arch"),
        "requires Arch Linux"
    );
    let manifest: Manifest = serde_json::from_slice(&fs::read(config.join("manifest.json"))?)?;
    ensure!(!manifest.kernels.is_empty(), "no configured kernels");
    for (kernel, images) in &manifest.kernels {
        ensure!(
            kernel.starts_with("linux")
                && kernel
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "invalid kernel package {kernel}"
        );
        ensure!(
            !images.is_empty()
                && images.iter().all(|image| Path::new(image)
                    .file_name()
                    .is_some_and(|name| name == image.as_str())
                    && image.ends_with(".efi")),
            "invalid UKI names for {kernel}"
        );
    }
    ensure!(
        manifest
            .kernels
            .values()
            .flatten()
            .any(|image| image == &manifest.default),
        "default UKI is not generated"
    );
    let public = PublicKey::read(Path::new("/etc/kernel/tpm2-pcr-public-key.pem"))?;
    public.validate_private(Path::new("/etc/kernel/tpm2-pcr-private-key.pem"))?;
    // Test this enrolled authority against Pluton's supported RSA sizes before
    // replacing any configuration or boot images. The RM releases the object.
    let context = tempfile::NamedTempFile::new()?;
    util::run(
        "tpm2_loadexternal",
        &[
            "--tcti=device:/dev/tpmrm0",
            "--hierarchy=o",
            "--key-algorithm=rsa",
            "--public=/etc/kernel/tpm2-pcr-public-key.pem",
            "--key-context",
            util::path(context.path())?,
        ],
    )?;
    let mut sources = Vec::new();
    for kernel in manifest.kernels.keys() {
        let files = util::output("pacman", &["-Qlq", kernel])?;
        let source = kernel_image(&files).with_context(|| format!("kernel package {kernel}"))?;
        sources.push((source.to_owned(), format!("/boot/vmlinuz-{kernel}")));
    }
    // Resolve every package first so a missing kernel does not partially install
    // the new generated configuration.
    install_tree(&config.join("etc"), Path::new("/etc"))?;
    for (source, destination) in sources {
        util::run("install", &["-m644", &source, &destination])?;
    }
    fs::create_dir_all("/boot/EFI/Linux")?;
    fs::create_dir_all("/var/lib/arch-uki/build")?;
    util::run("mkinitcpio", &["-P", "-t", "/var/lib/arch-uki/build"])?;
    for image in manifest.kernels.values().flatten() {
        util::run(
            "sbverify",
            &[
                "--cert",
                "/var/lib/sbctl/keys/db/db.pem",
                &format!("/boot/EFI/Linux/{image}"),
            ],
        )?;
    }
    // Select the generated entries only after every build and signature passes.
    install_tree(&config.join("boot"), Path::new("/boot"))?;
    util::run("bootctl", &["set-default", &manifest.default])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_selection_rejects_missing_or_ambiguous_package_payloads() {
        assert_eq!(kernel_image("/boot/vmlinuz-linux\n/usr/lib/modules/6.18/vmlinuz\n/usr/lib/modules/6.18/kernel/\n").unwrap(), "/usr/lib/modules/6.18/vmlinuz");
        assert!(kernel_image("/boot/vmlinuz-linux\n/usr/lib/modules/6.18/vmlinuz.old").is_err());
        assert!(
            kernel_image("/usr/lib/modules/6.18/vmlinuz\n/usr/lib/modules/6.19/vmlinuz").is_err()
        );
    }
}
