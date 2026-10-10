use crate::util;
use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use std::{
    fs,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

const DEVICE: &str = "/dev/disk/by-uuid/b05803cd-53dd-4796-9be2-40242e4d30bc";
const KERNELS: [&str; 3] = ["linux", "linux-cachyos", "linux-lts612"];
const STATE: &str = "/var/lib/arch-uki";

#[derive(Subcommand)]
pub enum Operation {
    /// Install the Nix-generated configuration and build signed UKIs.
    Prepare { config: PathBuf },
    /// Run from a measured UKI boot; requests the existing LUKS passphrase.
    Enroll,
    /// Retire legacy boot entries after a verified TPM-unlocked UKI boot.
    Finish,
}

fn backup(path: &Path) -> Result<()> {
    let dest = Path::new(STATE)
        .join("legacy")
        .join(path.strip_prefix("/")?);
    if path.is_file() && !dest.exists() {
        fs::create_dir_all(dest.parent().context("backup has no parent")?)?;
        fs::copy(path, dest)?;
    }
    Ok(())
}

fn install_tree(source: &Path, target: &Path) -> Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let dest = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            fs::create_dir_all(&dest)?;
            install_tree(&entry.path(), &dest)?;
        } else {
            ensure!(
                entry.file_type()?.is_file(),
                "unexpected configuration symlink"
            );
            backup(&dest)?;
            util::run(
                "install",
                &["-m644", util::path(&entry.path())?, util::path(&dest)?],
            )?;
        }
    }
    Ok(())
}

fn measured_boot() -> Result<()> {
    util::run(
        "systemd-analyze",
        &["condition", "ConditionSecurity=measured-uki"],
    )?;
    let journal = util::output(
        "journalctl",
        &["-b", "--no-pager", "-u", "systemd-pcrosseparator.service"],
    )?;
    ensure!(
        journal.contains("Finished TPM PCR OS Separator"),
        "separator did not complete in this boot"
    );
    let cmdline = util::read("/proc/cmdline")?;
    ensure!(
        !cmdline.split_whitespace().any(|v| v.starts_with("initrd=")),
        "legacy initrd boot"
    );
    Ok(())
}

fn metadata() -> Result<serde_json::Value> {
    util::json("cryptsetup", &["luksDump", "--dump-json-metadata", DEVICE])
}

fn strong_token(token: &serde_json::Value) -> bool {
    token["type"] == "systemd-tpm2"
        && token["tpm2-pcrs"] == serde_json::json!([7])
        && token["tpm2-pcr-bank"] == "sha256"
        && token["tpm2_pubkey_pcrs"] == serde_json::json!([11])
        && token["tpm2_pubkey_ref"] == "initrd"
        && token["tpm2_pubkey"]
            .as_str()
            .is_some_and(|key| !key.is_empty())
}

fn legacy_kernel_entry(contents: &str) -> bool {
    contents.lines().any(|line| {
        let mut fields = line.split_whitespace();
        fields.next() == Some("linux")
            && fields.next().is_some_and(|path| {
                path.strip_prefix("/vmlinuz-")
                    .is_some_and(|kernel| KERNELS.contains(&kernel))
            })
    })
}

pub fn run(action: Operation) -> Result<()> {
    ensure!(unsafe { libc::geteuid() } == 0, "run with sudo");
    ensure!(
        util::output("hostname", &[])?.trim() == "laptop",
        "this configuration is laptop-specific"
    );
    ensure!(
        util::read("/etc/os-release")?
            .lines()
            .any(|line| line == "ID=arch"),
        "requires Arch Linux"
    );
    match action {
        Operation::Prepare { config } => {
            util::run(
                "pacman",
                &[
                    "-S",
                    "--needed",
                    "--noconfirm",
                    "systemd-ukify",
                    "sbsigntools",
                    "tpm2-tools",
                ],
            )?;
            fs::create_dir_all(STATE)?;
            fs::set_permissions(STATE, fs::Permissions::from_mode(0o700))?;
            backup(Path::new("/boot/loader/loader.conf"))?;
            fs::create_dir_all("/etc/kernel")?;
            let private = Path::new("/etc/kernel/tpm2-pcr-private-key.pem");
            let public = Path::new("/etc/kernel/tpm2-pcr-public-key.pem");
            ensure!(
                private.exists() == public.exists(),
                "incomplete PCR signing key pair"
            );
            if !private.exists() {
                // Establish private permissions before OpenSSL writes key material.
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(private)?;
                util::run(
                    "openssl",
                    &[
                        "genpkey",
                        "-algorithm",
                        "RSA",
                        "-pkeyopt",
                        "rsa_keygen_bits:2048",
                        "-out",
                        util::path(private)?,
                    ],
                )?;
                fs::set_permissions(private, fs::Permissions::from_mode(0o600))?;
                util::run(
                    "openssl",
                    &[
                        "pkey",
                        "-in",
                        util::path(private)?,
                        "-pubout",
                        "-out",
                        util::path(public)?,
                    ],
                )?;
                fs::set_permissions(public, fs::Permissions::from_mode(0o644))?;
            }
            // Pluton rejects unsupported RSA sizes when loading policy keys.
            // Check the actual public key before changing any boot images.
            let key_context = format!("{STATE}/pcr-key-test.ctx");
            util::run(
                "tpm2_loadexternal",
                &[
                    "--tcti=device:/dev/tpmrm0",
                    "--hierarchy=o",
                    "--key-algorithm=rsa",
                    &format!("--public={}", util::path(public)?),
                    &format!("--key-context={key_context}"),
                ],
            )?;
            // /dev/tpmrm0 releases this transient object when the tool exits.
            fs::remove_file(key_context)?;
            install_tree(&config.join("etc"), Path::new("/etc"))?;
            fs::create_dir_all("/boot/EFI/Linux")?;
            // Refresh stale kernel copies from package-owned files before building.
            for kernel in KERNELS {
                let files = util::output("pacman", &["-Qlq", kernel])?;
                let sources: Vec<_> = files
                    .lines()
                    .filter(|p| p.starts_with("/usr/lib/modules/") && p.ends_with("/vmlinuz"))
                    .collect();
                ensure!(
                    sources.len() == 1,
                    "expected one installed kernel for {kernel}"
                );
                let destination = format!("/boot/vmlinuz-{kernel}");
                util::run("install", &["-m644", sources[0], &destination])?;
            }
            fs::create_dir_all(format!("{STATE}/build"))?;
            util::run("mkinitcpio", &["-P", "-t", &format!("{STATE}/build")])?;
            for name in ["linux", "linux-fallback", "linux-cachyos", "linux-lts612"] {
                util::run(
                    "sbverify",
                    &[
                        "--cert",
                        "/var/lib/sbctl/keys/db/db.pem",
                        &format!("/boot/EFI/Linux/arch-{name}.efi"),
                    ],
                )?;
            }
            // Select UKIs only after every build and signature check succeeds.
            install_tree(&config.join("boot"), Path::new("/boot"))?;
            util::run("bootctl", &["set-default", "arch-linux-cachyos.efi"])?;
            println!("UKIs prepared. Boot one, then run: sudo host-tools arch-uki enroll");
        }
        Operation::Enroll => {
            measured_boot()?;
            let before = metadata()?;
            let slots = before["keyslots"].as_object().context("missing keyslots")?;
            let tokens = before["tokens"].as_object().context("missing tokens")?;
            ensure!(
                slots
                    .keys()
                    .any(|slot| !tokens.values().any(|token| token["keyslots"]
                        .as_array()
                        .is_some_and(|keys| keys.iter().any(|key| key.as_str() == Some(slot))))),
                "no independent passphrase/recovery slot"
            );
            let header = format!("{STATE}/luks-header-before-policy.img");
            if !Path::new(&header).exists() {
                util::run(
                    "cryptsetup",
                    &["luksHeaderBackup", DEVICE, "--header-backup-file", &header],
                )?;
                fs::set_permissions(&header, fs::Permissions::from_mode(0o600))?;
            }
            // The signed policy deliberately authorizes only enter-initrd, not
            // the current host phase. Omit current-phase signature validation;
            // the following boot tests the initrd policy.
            util::run(
                "systemd-cryptenroll",
                &[
                    "--tpm2-device=auto",
                    "--tpm2-pcrs=7:sha256",
                    "--tpm2-public-key=/etc/kernel/tpm2-pcr-public-key.pem",
                    // Signed PCRs inherit the bank from the static selection.
                    "--tpm2-public-key-pcrs=11",
                    "--tpm2-public-key-policyref=initrd",
                    "--tpm2-pcrlock=",
                    "--wipe-slot=tpm2",
                    DEVICE,
                ],
            )?;
            let after = metadata()?;
            let tokens = after["tokens"].as_object().context("missing tokens")?;
            ensure!(
                tokens.values().any(strong_token),
                "enrollment lacks expected signed initrd policy"
            );
            util::atomic_write(
                Path::new(&format!("{STATE}/enrollment-boot-id")),
                util::read("/proc/sys/kernel/random/boot_id")?.as_bytes(),
            )?;
            println!(
                "TPM enrollment replaced. Reboot to test automatic unlocking before retiring legacy entries."
            );
        }
        Operation::Finish => {
            measured_boot()?;
            ensure!(
                util::read(format!("{STATE}/enrollment-boot-id"))?
                    != util::read("/proc/sys/kernel/random/boot_id")?,
                "reboot after enrollment before retiring legacy entries"
            );
            let data = metadata()?;
            let tokens = data["tokens"].as_object().context("missing tokens")?;
            ensure!(
                tokens.len() == 1 && tokens.values().all(strong_token),
                "unexpected TPM policy or remaining weak token"
            );
            // Preserve old configuration on the encrypted root, not on the ESP.
            for entry in fs::read_dir("/boot/loader/entries")? {
                let entry = entry?;
                if entry.path().extension().is_some_and(|ext| ext == "conf") {
                    let contents = util::read(entry.path())?;
                    if legacy_kernel_entry(&contents) {
                        backup(&entry.path())?;
                        fs::remove_file(entry.path())?;
                    }
                }
            }
            for name in ["linux", "linux-fallback", "linux-cachyos", "linux-lts612"] {
                let image = PathBuf::from(format!("/boot/initramfs-{name}.img"));
                if image.exists() {
                    fs::remove_file(image)?;
                }
            }
            println!(
                "Legacy entries retired; signed UKIs and the passphrase recovery slot remain."
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_pcr7_only_and_runtime_authorized_tokens() {
        let mut token = serde_json::json!({
            "type": "systemd-tpm2", "tpm2-pcrs": [7], "tpm2-pcr-bank": "sha256",
            "tpm2_pubkey_pcrs": [11], "tpm2_pubkey_ref": "initrd", "tpm2_pubkey": "public key"
        });
        assert!(strong_token(&token));
        token.as_object_mut().unwrap().remove("tpm2_pubkey_pcrs");
        assert!(!strong_token(&token));
        token["tpm2_pubkey_pcrs"] = serde_json::json!([11]);
        token["tpm2_pubkey_ref"] = serde_json::json!("runtime");
        assert!(!strong_token(&token));
    }

    #[test]
    fn retires_aligned_legacy_entries_without_selecting_other_kernels() {
        for kernel in KERNELS {
            assert!(legacy_kernel_entry(&format!(
                "title Arch\nlinux   /vmlinuz-{kernel}\ninitrd /old.img"
            )));
            assert!(legacy_kernel_entry(&format!("\tlinux\t/vmlinuz-{kernel}")));
        }
        assert!(!legacy_kernel_entry("linux /vmlinuz-other"));
        assert!(!legacy_kernel_entry("efi /EFI/Linux/arch-linux.efi"));
    }
}
