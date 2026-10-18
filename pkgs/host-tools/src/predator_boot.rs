//! Public credential companions carry phase signatures without changing initrd.
use crate::{boot_keys::PublicKey, util};
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use clap::Subcommand;
use object::{Architecture, Object, ObjectSection, read::pe::PeFile64};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs, io,
    path::{Component, Path, PathBuf},
    process::Command,
};
use tempfile::{NamedTempFile, TempDir};

const STATE: &str = "/var/lib/predator-boot";
const MARKER: &str = "predator.authenticated-boot=1";
const SIGNATURE: &str = "/run/credentials/@encrypted/predator-pcr-signature";
const CREDENTIAL: &str = "predator-pcr-signature.cred";
const MEASURED: &[&str] = &[
    ".linux", ".osrel", ".cmdline", ".initrd", ".ucode", ".splash", ".dtb", ".uname", ".sbat",
    ".pcrpkey",
];
const UNSUPPORTED: &[&str] = &[".dtbauto", ".hwids", ".profile", ".efifw"];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    esp: PathBuf,
    certificate: PathBuf,
    token_directory: PathBuf,
    installer: PathBuf,
    recovery_identity: RecoveryIdentity,
    disks: Disks,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Disks {
    fast: PathBuf,
    root: PathBuf,
    tank1: PathBuf,
    tank2: PathBuf,
    tank3: PathBuf,
}

impl Disks {
    fn devices(&self) -> [(&'static str, &Path); 5] {
        [
            ("fast", &self.fast),
            ("root", &self.root),
            ("tank1", &self.tank1),
            ("tank2", &self.tank2),
            ("tank3", &self.tank3),
        ]
    }
}

#[derive(Deserialize)]
#[serde(transparent)]
struct RecoveryIdentity(PathBuf);

#[derive(Clone, Copy)]
enum Authority {
    Root,
    Data,
}

impl Authority {
    const ALL: [Self; 2] = [Self::Root, Self::Data];

    fn name(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Data => "data",
        }
    }

    fn phase(self) -> &'static str {
        match self {
            Self::Root => "enter-initrd",
            Self::Data => "enter-initrd:leave-initrd",
        }
    }

    fn key(self, suffix: &str) -> PathBuf {
        Path::new(STATE).join(format!("{}-{suffix}.pem", self.name()))
    }
}

fn public_keys() -> Result<[PublicKey; 2]> {
    let root = PublicKey::read(&Authority::Root.key("public"))?;
    let data = PublicKey::read(&Authority::Data.key("public"))?;
    ensure!(root != data, "root/data signing authorities must differ");
    Ok([root, data])
}

#[derive(Subcommand)]
pub enum Operation {
    Install,
    Audit,
}

pub fn run(configuration: &Path, action: Operation) -> Result<()> {
    ensure!(unsafe { libc::geteuid() } == 0, "requires root");
    ensure!(
        util::read("/etc/hostname")?.trim() == "predator",
        "requires Predator"
    );
    unsafe { libc::umask(0o077) };
    let config: Config = serde_json::from_slice(&fs::read(configuration)?)?;
    ensure!(
        config
            .disks
            .devices()
            .iter()
            .all(|(_, p)| p.parent() == Some(Path::new("/dev/disk/by-id"))),
        "disks must use persistent device identities"
    );
    match action {
        Operation::Audit => audit(&config),
        Operation::Install => {
            // Serialize upstream GC, signature regeneration and rollback.
            let lock = fs::File::options()
                .write(true)
                .create(true)
                .truncate(false)
                .open(Path::new(STATE).join("install.lock"))?;
            lock.lock()
                .context("serialize Predator boot installation")?;
            let keys = public_keys()?;
            for (authority, key) in Authority::ALL.into_iter().zip(&keys) {
                key.validate_private(&authority.key("private"))?;
                println!(
                    "{} authority SHA256={}",
                    authority.name(),
                    key.fingerprint()
                );
            }
            config.recovery_identity.prepare(&keys[0])?;
            install(&config, &keys)
        }
    }
}

impl RecoveryIdentity {
    fn credential(&self) -> PathBuf {
        self.0.join("host-key.cred")
    }

    fn prepare(&self, authority: &PublicKey) -> Result<()> {
        let directory = &self.0;
        if !fs::exists(directory)? {
            ensure!(
                util::read("/proc/cmdline")?
                    .split_whitespace()
                    .any(|v| v == MARKER),
                "provision recovery identity from an authenticated normal boot"
            );
            BootPhases::current()?.require_separator()?;
            let work = tempfile::tempdir_in(directory.parent().context("recovery state parent")?)?;
            let private = work.path().join("host-key");
            util::run(
                "ssh-keygen",
                &["-q", "-t", "ed25519", "-N", "", "-f", util::path(&private)?],
            )?;
            let encrypted = work.path().join("host-key.cred");
            util::run(
                "systemd-creds",
                &[
                    "encrypt",
                    "--with-key=tpm2-with-public-key",
                    "--tpm2-device=/dev/tpmrm0",
                    "--tpm2-pcrs=7",
                    "--tpm2-public-key-pcrs=11",
                    &format!(
                        "--tpm2-public-key={}",
                        Authority::Root.key("public").display()
                    ),
                    "--name=predator-recovery-ssh",
                    "--refuse-null",
                    util::path(&private)?,
                    util::path(&encrypted)?,
                ],
            )?;
            fs::write(work.path().join("root-authority"), authority.fingerprint())?;
            fs::rename(work.path(), directory).context("publish complete recovery identity")?;
        }
        ensure!(
            fs::read_to_string(directory.join("root-authority"))? == authority.fingerprint(),
            "recovery credential uses a different signing authority; restore its matching backup"
        );
        ensure!(
            fs::metadata(self.credential())?.len() > 0,
            "empty recovery credential"
        );
        let public = fs::read_to_string(directory.join("host-key.pub"))?;
        let derived = util::output(
            "ssh-keygen",
            &["-y", "-f", util::path(&directory.join("host-key"))?],
        )?;
        ensure!(
            public
                .split_whitespace()
                .take(2)
                .eq(derived.split_whitespace().take(2)),
            "mismatched recovery SSH identity"
        );
        println!("Recovery SSH identity: {}", public.trim());
        Ok(())
    }

    fn audit(&self) -> Result<()> {
        let public = self.0.join("host-key.pub");
        let fingerprint =
            util::output("ssh-keygen", &["-lf", util::path(&public)?, "-E", "sha256"])?;
        let fingerprint = fingerprint
            .split_whitespace()
            .nth(1)
            .context("SSH fingerprint")?;
        ensure!(
            recovery_key_proof(&boot_log("predator-recovery-key.service")?, fingerprint),
            "no proof that initrd unsealed the pinned recovery identity"
        );
        let plaintext = NamedTempFile::new_in("/run")?;
        let late = Command::new("systemd-creds")
            .env("SYSTEMD_LOG_LEVEL", "debug")
            .args([
                "decrypt",
                "--name=predator-recovery-ssh",
                "--refuse-null",
                "--no-ask-password",
            ])
            .arg(format!("--tpm2-signature={SIGNATURE}"))
            .arg(self.credential())
            .arg(plaintext.path())
            .output()?;
        ensure!(
            !late.status.success(),
            "recovery SSH identity still unseals after initrd"
        );
        let diagnostic = String::from_utf8_lossy(&late.stderr);
        ensure!(
            diagnostic
                .contains("Couldn't find signature for this PCR bank, PCR index and public key."),
            "recovery identity rejection was not a phase failure: {diagnostic}"
        );
        Ok(())
    }
}

fn boot_log(unit: &str) -> Result<String> {
    util::output("journalctl", &["-b", "-u", unit, "-o", "cat", "--no-pager"])
}

// PE parsing is read-only. Measurements use VirtualSize, not file alignment
// padding, and must reject sections whose virtual payload is not in the file.
fn sections(image: &[u8]) -> Result<BTreeMap<&str, &[u8]>> {
    let pe = PeFile64::parse(image).context("parse PE32+ image")?;
    ensure!(pe.architecture() == Architecture::X86_64, "not x86_64 PE");
    let mut result = BTreeMap::new();
    for section in pe.sections() {
        let name = section.name()?;
        if !(MEASURED.contains(&name)
            || UNSUPPORTED.contains(&name)
            || matches!(name, ".linuxh" | ".initrdh"))
        {
            continue;
        }
        let payload = section
            .data()?
            .get(..usize::try_from(section.size())?)
            .with_context(|| format!("PE section {name} exceeds its raw payload"))?;
        ensure!(
            result.insert(name, payload).is_none(),
            "duplicate PE section {name}"
        );
    }
    Ok(result)
}

fn efi_path(esp: &Path, data: &[u8]) -> Result<PathBuf> {
    let text = std::str::from_utf8(data)?.trim_end_matches('\0');
    ensure!(
        !text.contains('/'),
        "EFI paths must use firmware separators"
    );
    let relative = text
        .strip_prefix('\\')
        .context("not an absolute EFI path")?
        .replace('\\', "/");
    let path = Path::new(&relative);
    ensure!(
        path.components().all(|c| matches!(c, Component::Normal(_)))
            && path.starts_with("EFI/nixos"),
        "unsafe EFI payload path"
    );
    Ok(esp.join(path))
}

fn payload_hash(path: &Path) -> Result<[u8; 32]> {
    let mut hash = Sha256::new();
    io::copy(&mut fs::File::open(path)?, &mut hash)?;
    Ok(hash.finalize().into())
}

fn authenticated_payloads(sections: &BTreeMap<&str, &[u8]>, esp: &Path) -> Result<[PathBuf; 2]> {
    for name in UNSUPPORTED {
        ensure!(
            !sections.contains_key(name),
            "unsupported Lanzaboote measurement section {name}"
        );
    }
    for name in [".linux", ".osrel", ".cmdline", ".initrd"] {
        ensure!(sections.contains_key(name), "missing {name}");
    }
    let payload = |name: &str, hash: &str| -> Result<PathBuf> {
        let expected = *sections
            .get(hash)
            .context("missing authenticated payload hash")?;
        let path = efi_path(esp, sections[name])?;
        ensure!(
            payload_hash(&path)?.as_slice() == expected,
            "{name} payload hash mismatch"
        );
        Ok(path)
    };
    Ok([
        payload(".linux", ".linuxh")?,
        payload(".initrd", ".initrdh")?,
    ])
}

fn inputs(sections: &BTreeMap<&str, &[u8]>, esp: &Path, work: &Path) -> Result<Vec<OsString>> {
    let [kernel, initrd] = authenticated_payloads(sections, esp)?;
    let mut args = Vec::new();
    for &name in MEASURED {
        let Some(data) = sections.get(name) else {
            continue;
        };
        let staged;
        let path = match name {
            ".linux" => &kernel,
            ".initrd" => &initrd,
            _ => {
                staged = work.join(name);
                fs::write(&staged, data)?;
                &staged
            }
        };
        args.push(format!("--{}={}", &name[1..], util::path(path)?).into());
    }
    Ok(args)
}

fn measure(args: &[OsString]) -> Command {
    let mut command = Command::new("systemd-measure");
    command.args(args).args(["--bank=sha256", "--json=short"]);
    command
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PolicySignature {
    pcrs: [u8; 1],
    pkfp: String,
    pol: String,
    sig: String,
}

impl PolicySignature {
    fn validate(&self, key: &PublicKey) -> Result<()> {
        ensure!(
            self.pcrs == [11] && self.pkfp == key.policy_fingerprint(),
            "signature does not authorize the expected PCR11 authority"
        );
        ensure!(
            self.pol.len() == 64
                && self.pol.bytes().all(|b| b.is_ascii_hexdigit())
                && !STANDARD.decode(&self.sig)?.is_empty(),
            "invalid PCR policy/signature encoding"
        );
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SignatureSet<T> {
    sha256: T,
}

fn companion(image: &Path) -> PathBuf {
    let mut directory = image.as_os_str().to_os_string();
    directory.push(".extra");
    PathBuf::from(directory).join(CREDENTIAL)
}

fn managed_image(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.as_encoded_bytes().starts_with(b"nixos-"))
        && path.extension().is_some_and(|s| s == "efi")
}

fn managed_files(directory: &Path, matches: impl Fn(&Path) -> bool) -> Result<BTreeSet<PathBuf>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        // A failed installation can remove a directory that rollback recreates.
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(error) => return Err(error.into()),
    };
    let mut files = BTreeSet::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_file() && matches(&path) {
            files.insert(path);
        }
    }
    Ok(files)
}

fn installed_images(esp: &Path) -> Result<BTreeSet<PathBuf>> {
    managed_files(&esp.join("EFI/Linux"), managed_image)
}

fn sign_esp(config: &Config, keys: &[PublicKey; 2]) -> Result<()> {
    let mut count = 0;
    for image in installed_images(&config.esp)? {
        let image_bytes = fs::read(&image)?;
        let sections = sections(&image_bytes)?;
        let cmdline = std::str::from_utf8(sections.get(".cmdline").context("UKI cmdline")?)?
            .trim_end_matches('\0');
        if !cmdline.split_whitespace().any(|v| v == MARKER)
            || cmdline.contains("initcall_blacklist=crb_acpi_driver_init")
        {
            continue;
        }
        verify_image(config, &image)?;
        let work = tempfile::tempdir()?;
        let args = inputs(&sections, &config.esp, work.path())?;
        let sign = |authority: Authority, key: &PublicKey| -> Result<PolicySignature> {
            let signed: SignatureSet<[PolicySignature; 1]> =
                serde_json::from_slice(&util::command_output(
                    measure(&args)
                        .arg("sign")
                        .arg(format!(
                            "--private-key={}",
                            authority.key("private").display()
                        ))
                        .arg(format!(
                            "--public-key={}",
                            authority.key("public").display()
                        ))
                        .arg(format!("--phase={}", authority.phase())),
                )?)?;
            let [signature] = signed.sha256;
            signature.validate(key)?;
            Ok(signature)
        };
        let [root, data] = keys;
        let signatures = [sign(Authority::Root, root)?, sign(Authority::Data, data)?];
        let credential = companion(&image);
        fs::create_dir_all(credential.parent().context("credential parent")?)?;
        util::atomic_write(
            &credential,
            &serde_json::to_vec(&SignatureSet { sha256: signatures })?,
        )?;
        println!("Authorized {} for initrd/early-host", image.display());
        count += 1;
    }
    ensure!(count > 0, "no authenticated Predator generations installed");
    util::run("sync", &["-f", util::path(&config.esp)?])
}

fn verify_image(config: &Config, image: &Path) -> Result<()> {
    util::run(
        "sbverify",
        &[
            "--cert",
            util::path(&config.certificate)?,
            util::path(image)?,
        ],
    )
}

struct BootSnapshot {
    default: String,
    artifacts: BTreeSet<PathBuf>,
    work: TempDir,
}

#[derive(Deserialize, Debug)]
struct BootEntry {
    id: String,
    path: Option<PathBuf>,
    #[serde(rename = "isDefault")]
    is_default: bool,
}

/// The image to preserve: the one the next boot would actually use.
///
/// `bootctl list` also reports the stale efivar entries of the last boot, so a
/// flagged entry can point at a generation whose image was already garbage
/// collected, and a wildcard `default` in `loader.conf` flags nothing on this
/// ESP at all. Prefer a flagged image that is really installed, and otherwise
/// fall back to the newest plain generation, the image a `nixos-*` wildcard
/// default boots.
fn boot_image<'a>(entries: &'a [BootEntry], esp: &Path) -> Result<&'a BootEntry> {
    let images = esp.join("EFI/Linux");
    let installed = |entry: &BootEntry| {
        entry
            .path
            .as_deref()
            .is_some_and(|path| path.starts_with(&images))
    };
    if let Some(entry) = entries
        .iter()
        .find(|entry| entry.is_default && installed(entry))
    {
        return Ok(entry);
    }
    entries
        .iter()
        .filter(|entry| installed(entry))
        .filter_map(|entry| Some((generation(&entry.id)?, entry)))
        .max_by_key(|(number, _)| *number)
        .map(|(_, entry)| entry)
        .context("no managed Predator generation is installed under EFI/Linux")
}

/// The generation number in `nixos-generation-<number>-<hash>.efi`, or `None`
/// for specialisation images. A plain image is what a wildcard default picks in
/// practice, since only its hex hash follows the generation number.
fn generation(id: &str) -> Option<u32> {
    let rest = id.strip_prefix("nixos-generation-")?;
    let (number, _) = rest.split_once('-')?;
    if rest.contains("-specialisation-") {
        return None;
    }
    number.parse().ok()
}

impl BootSnapshot {
    fn capture(config: &Config) -> Result<Self> {
        let entries: Vec<BootEntry> = serde_json::from_slice(&util::command_output(
            Command::new("bootctl")
                .args(["list", "--json=short", "--esp-path"])
                .arg(&config.esp),
        )?)?;
        let default = boot_image(&entries, &config.esp)?;
        let image = default
            .path
            .as_deref()
            .context("default entry has no image to preserve")?;
        let image_bytes = fs::read(image)
            .with_context(|| format!("the image to preserve is missing: {}", image.display()))?;
        verify_image(config, image).context("previous default image is not authenticated")?;
        let sections = sections(&image_bytes)?;
        let [kernel, initrd] = authenticated_payloads(&sections, &config.esp)
            .context("previous default payloads are not authenticated")?;
        let work = tempfile::tempdir_in(STATE)?;
        let save = |target: &Path| -> Result<()> {
            let relative = target.strip_prefix(&config.esp)?;
            copy_file(target, &work.path().join(relative))
        };
        for path in [kernel.as_path(), initrd.as_path(), image] {
            save(path)?;
        }
        let artifacts = boot_artifacts(&config.esp)?;
        for image in artifacts.iter().filter(|p| managed_image(p)) {
            let credential = companion(image);
            match fs::read(&credential) {
                Ok(bytes) => {
                    let backup = work.path().join(credential.strip_prefix(&config.esp)?);
                    fs::create_dir_all(backup.parent().context("snapshot credential parent")?)?;
                    fs::write(backup, bytes)?;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        save(&config.esp.join("loader/loader.conf"))?;
        fs::write(work.path().join("default-entry"), &default.id)?;
        Ok(Self {
            default: default.id.clone(),
            artifacts,
            work,
        })
    }

    fn restore_files(&self, esp: &Path) -> Result<()> {
        // Preserve the actual default image and payloads, even if repeated failed
        // updates moved its generation outside Lanzaboote's retention window.
        let mut companions = Vec::new();
        for directory in ["EFI/nixos", "EFI/Linux"] {
            for entry in fs::read_dir(self.work.path().join(directory))? {
                let entry = entry?;
                let source = entry.path();
                let target = esp.join(source.strip_prefix(self.work.path())?);
                let kind = entry.file_type()?;
                if kind.is_file() {
                    copy_file(&source, &target)?;
                } else {
                    ensure!(
                        directory == "EFI/Linux"
                            && kind.is_dir()
                            && source.extension().is_some_and(|e| e == "extra"),
                        "unexpected boot snapshot entry {}",
                        source.display()
                    );
                    companions.push((source, target));
                }
            }
        }
        for (source, target) in companions {
            if fs::exists(target.with_extension(""))? {
                fs::create_dir_all(&target)?;
                util::atomic_write(
                    &target.join(CREDENTIAL),
                    &fs::read(source.join(CREDENTIAL))?,
                )?;
            }
        }
        Ok(())
    }

    fn restore(&self, config: &Config) -> Result<()> {
        // Reclaim the failed update before copying the protected default back.
        // Otherwise upstream GC can replace its payloads and exhaust the ESP.
        self.discard_new_artifacts(&config.esp)?;
        self.restore_files(&config.esp)?;
        fs::create_dir_all(config.esp.join("loader"))?;
        util::atomic_write(
            &config.esp.join("loader/loader.conf"),
            &fs::read(self.work.path().join("loader/loader.conf"))?,
        )?;
        boot_default(config, &self.default)?;
        util::atomic_write(
            &Path::new(STATE).join("failed-install-default.json"),
            &serde_json::to_vec(&self.default)?,
        )?;
        util::run("sync", &["-f", util::path(&config.esp)?])
    }

    fn discard_new_artifacts(&self, esp: &Path) -> Result<()> {
        for artifact in boot_artifacts(esp)?.difference(&self.artifacts) {
            if managed_image(artifact) {
                let directory = companion(artifact)
                    .parent()
                    .context("companion directory")?
                    .to_owned();
                if fs::exists(&directory)? {
                    fs::remove_dir_all(directory)?;
                }
            }
            fs::remove_file(artifact)?;
        }
        Ok(())
    }
}

fn boot_artifacts(esp: &Path) -> Result<BTreeSet<PathBuf>> {
    let mut artifacts = installed_images(esp)?;
    artifacts.extend(managed_files(&esp.join("EFI/nixos"), |path| {
        path.file_name().is_some_and(|name| {
            let name = name.as_encoded_bytes();
            name.starts_with(b"kernel-") || name.starts_with(b"initrd-")
        })
    })?);
    Ok(artifacts)
}

fn copy_file(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target.parent().context("boot backup parent")?)?;
    fs::copy(source, target)
        .with_context(|| format!("copy {} to {}", source.display(), target.display()))?;
    Ok(())
}

fn boot_default(config: &Config, entry: &str) -> Result<()> {
    util::run(
        "bootctl",
        &["--esp-path", util::path(&config.esp)?, "set-default", entry],
    )
}

fn install(config: &Config, keys: &[PublicKey; 2]) -> Result<()> {
    let snapshot = BootSnapshot::capture(config)?;
    let result =
        util::run(util::path(&config.installer)?, &[]).and_then(|()| sign_esp(config, keys));
    if let Err(error) = result {
        if let Err(restore) = snapshot.restore(config) {
            let retained = snapshot.work.keep();
            return Err(restore.context(format!(
                "installation failed: {error:#}; restoring boot also failed; backup retained at {}",
                retained.display()
            )));
        }
        snapshot
            .work
            .close()
            .context("remove restored boot snapshot")?;
        return Err(error.context("restored previous boot image, payloads and signatures"));
    }
    let pin = Path::new(STATE).join("failed-install-default.json");
    match fs::read(&pin) {
        Ok(bytes) => {
            let pinned: String = serde_json::from_slice(&bytes)?;
            if pinned == snapshot.default {
                boot_default(config, "")?;
            }
            fs::remove_file(pin)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    snapshot.work.close()?;
    Ok(())
}

#[derive(Deserialize)]
struct LuksHeader {
    tokens: BTreeMap<String, Token>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Token {
    #[serde(rename = "systemd-tpm2")]
    Tpm(TpmToken),
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct TpmToken {
    keyslots: [String; 1],
    #[serde(rename = "tpm2-pcrs")]
    pcrs: [u8; 1],
    #[serde(rename = "tpm2-pcr-bank")]
    bank: String,
    #[serde(rename = "tpm2_pubkey_pcrs")]
    public_pcrs: [u8; 1],
    #[serde(rename = "tpm2_pubkey")]
    public_key: String,
    #[serde(rename = "tpm2_pubkey_ref")]
    policy_ref: Option<String>,
}

impl TpmToken {
    fn slot(&self, key: &PublicKey) -> Result<&str> {
        ensure!(
            self.pcrs == [7]
                && self.bank == "sha256"
                && self.public_pcrs == [11]
                && self.policy_ref.as_deref().is_none_or(str::is_empty),
            "unexpected TPM authorization policy"
        );
        ensure!(
            key.matches_token_key(&self.public_key)?,
            "wrong TPM signing authority"
        );
        Ok(&self.keyslots[0])
    }
}

fn mapper(name: &str) -> String {
    if name == "root" {
        "crypt-root".into()
    } else {
        format!("{name}-crypt")
    }
}

fn boot_slot_proof(log: &str, token: &str, slot: &str, mapper: &str) -> bool {
    // Digest verification is only an attempt. Require its successful mapping
    // activation without crossing into another token/passphrase/service attempt.
    let steps = [
        "Completed TPM2 key unsealing".to_owned(),
        format!("Trying to open keyslot {slot} with token {token} (type systemd-tpm2)."),
        format!("Trying to open LUKS2 keyslot {slot}."),
        format!("Verifying key from keyslot {slot}, digest"),
        "Loading key (type logon,".to_owned(),
        format!("dm resume {mapper} "),
        format!("Finished Cryptography Setup for {mapper}."),
    ];
    let mut next = 0;
    for line in log.lines() {
        if line.contains(&steps[next]) {
            next += 1;
            if next == steps.len() {
                return true;
            }
        } else if line.contains(&steps[0]) {
            next = 1;
        } else if line.contains(" failed")
            || line.contains("Failed ")
            || line.contains(" unsuccessful")
            || line.contains("Beginning attempt ")
            || line.contains("Starting Cryptography Setup")
            || line.contains("Activating volume ")
            || line.contains("Trying to open keyslot ")
            || line.contains("Trying to open LUKS2 keyslot ")
            || line.contains("Verifying key from keyslot ")
            || line.contains("Key slot ")
        {
            next = 0;
        }
    }
    false
}

fn phase_policy_rejected(diagnostic: &str) -> bool {
    diagnostic.contains("No signature for current PCR policy in TPM2 signature JSON")
        && diagnostic.contains("token does not apply to current boot state")
}

fn recovery_key_proof(log: &str, fingerprint: &str) -> bool {
    let Some((_, attempt)) = log.rsplit_once("Starting Unseal initrd recovery SSH identity") else {
        return false;
    };
    let Some((completed, _)) = attempt.split_once("Finished Unseal initrd recovery SSH identity")
    else {
        return false;
    };
    completed.contains(fingerprint)
        && !completed.contains("Failed ")
        && !completed.contains(" failed")
}

struct BootPhases {
    separator: bool,
    remaining: &'static [&'static str],
}

impl BootPhases {
    fn current() -> Result<Self> {
        // Short-lived senders can exit before journald resolves their unit.
        // Native event fields survive; trust only root's current-boot events.
        Self::parse(&util::output(
            "journalctl",
            &[
                "-b",
                "_UID=0",
                "MESSAGE_ID=3f7d5ef3e54f4302b4f0b143bb270cab",
                "--output=json",
                "--output-fields=PCR,MEASURING",
                "--no-pager",
                "--quiet",
            ],
        )?)
    }

    fn parse(log: &str) -> Result<Self> {
        #[derive(Deserialize)]
        struct Measurement<'a> {
            #[serde(borrow, rename = "PCR")]
            pcr: Cow<'a, str>,
            #[serde(borrow, rename = "MEASURING")]
            word: Cow<'a, str>,
        }
        let mut phases = Self {
            separator: false,
            remaining: &["enter-initrd", "leave-initrd", "sysinit", "ready"],
        };
        for line in log.lines() {
            let event: Measurement<'_> = serde_json::from_str(line)?;
            if event.pcr == "7" && event.word == "os-separator" {
                phases.separator = true;
            }
            if event.pcr == "11" && phases.remaining.first() == Some(&event.word.as_ref()) {
                phases.remaining = &phases.remaining[1..];
            }
        }
        Ok(phases)
    }

    fn require_separator(&self) -> Result<()> {
        ensure!(self.separator, "missing PCR7 OS separator measurement");
        Ok(())
    }

    fn require_ready(&self) -> Result<()> {
        self.require_separator()?;
        ensure!(
            self.remaining.is_empty(),
            "missing or out-of-order PCR11 phase measurement: {}",
            self.remaining[0]
        );
        Ok(())
    }
}

fn audit(config: &Config) -> Result<()> {
    ensure!(
        util::read("/proc/cmdline")?
            .split_whitespace()
            .any(|v| v == MARKER),
        "not booted into authenticated generation"
    );
    let system = fs::canonicalize("/run/booted-system")?;
    ensure!(
        system == fs::canonicalize("/run/current-system")?,
        "activated generation not booted"
    );
    let keys = public_keys()?;
    let signatures = fs::read(SIGNATURE)?;
    let signed: SignatureSet<[PolicySignature; 2]> = serde_json::from_slice(&signatures)?;
    for (signature, key) in signed.sha256.iter().zip(&keys) {
        signature.validate(key)?;
    }
    fs::create_dir_all("/run/systemd")?;
    util::atomic_write(
        Path::new("/run/systemd/tpm2-pcr-signature.json"),
        &signatures,
    )?;
    BootPhases::current()?.require_ready()?;
    config.recovery_identity.audit()?;
    for (name, device) in config.disks.devices() {
        let metadata: LuksHeader = serde_json::from_str(&util::output(
            "cryptsetup",
            &["luksDump", "--dump-json-metadata", util::path(device)?],
        )?)
        .with_context(|| format!("{name}: invalid LUKS authorization metadata"))?;
        let mut tokens = metadata
            .tokens
            .iter()
            .filter_map(|(id, token)| match token {
                Token::Tpm(token) => Some((id, token)),
                Token::Other => None,
            });
        let (id, token) = tokens
            .next()
            .with_context(|| format!("{name}: missing TPM authorization"))?;
        ensure!(
            tokens.next().is_none(),
            "{name}: additional TPM authorization"
        );
        let key = if name == "root" { &keys[0] } else { &keys[1] };
        let slot = token
            .slot(key)
            .with_context(|| format!("{name}: invalid TPM policy"))?;
        let mapper = mapper(name);
        let unit = format!(
            "systemd-cryptsetup@{}.service",
            mapper.replace('-', "\\x2d")
        );
        let log = boot_log(&unit)?;
        ensure!(
            boot_slot_proof(&log, id, slot, &mapper),
            "{name}: no proof of boot unlock with authenticated slot {slot}"
        );
        let late = Command::new("cryptsetup")
            // test-passphrase also dlopens handlers by their ELF basename.
            .env("LD_LIBRARY_PATH", &config.token_directory)
            .arg("--external-tokens-path")
            .arg(&config.token_directory)
            .args([
                "--debug",
                "open",
                "--test-passphrase",
                "--token-only",
                "--token-id",
                id,
            ])
            .arg(device)
            .output()?;
        ensure!(
            !late.status.success(),
            "{name}: TPM release still succeeds after boot phase"
        );
        let diagnostic = format!(
            "{}\n{}",
            String::from_utf8_lossy(&late.stdout),
            String::from_utf8_lossy(&late.stderr)
        );
        ensure!(
            phase_policy_rejected(&diagnostic),
            "{name}: rejection was not a phase policy failure: {diagnostic}"
        );
        println!("{name}: boot used authenticated slot {slot}; late-phase token {id} rejected");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn phase_measurements_use_native_fields_without_unit_attribution() {
        let events = [
            ("11", "enter-initrd"),
            ("7", "os-separator"),
            ("11", "leave-initrd"),
            ("11", "sysinit"),
            ("11", "ready"),
        ];
        let log = |events: &[(&str, &str)]| {
            events
                .iter()
                .map(|(pcr, word)| json!({"PCR": pcr, "MEASURING": word}).to_string())
                .collect::<Vec<_>>()
                .join("\n")
        };
        let ready = |events: &[(&str, &str)]| BootPhases::parse(&log(events))?.require_ready();
        assert!(ready(&events).is_ok());
        assert!(ready(&events[..4]).is_err());
        let early = BootPhases::parse(&log(&events[..2])).unwrap();
        assert!(early.require_separator().is_ok());
        assert!(early.require_ready().is_err());
        let mut wrong_pcr = events;
        wrong_pcr[1].0 = "4";
        assert!(ready(&wrong_pcr).is_err());
        let mut reordered = events;
        reordered.swap(2, 4);
        assert!(ready(&reordered).is_err());
        assert!(BootPhases::parse(&format!("{}\ninvalid", log(&events))).is_err());
        assert!(
            BootPhases::parse(&format!(
                "{}\n{{\"PCR\":\"11\",\"MEASURING\":\"quoted \\\"word\\\"\"}}",
                log(&events)
            ))
            .unwrap()
            .require_ready()
            .is_ok()
        );
    }

    #[test]
    fn disk_configuration_rejects_missing_and_unknown_roles() {
        let mut disks = json!({
            "fast": "/dev/disk/by-id/fast-part1",
            "root": "/dev/disk/by-id/root-part2",
            "tank1": "/dev/disk/by-id/tank1-part1",
            "tank2": "/dev/disk/by-id/tank2-part1",
            "tank3": "/dev/disk/by-id/tank3-part1"
        });
        assert!(serde_json::from_value::<Disks>(disks.clone()).is_ok());
        disks["unknown"] = json!("/dev/disk/by-id/other");
        assert!(serde_json::from_value::<Disks>(disks.clone()).is_err());
        disks.as_object_mut().unwrap().remove("unknown");
        disks.as_object_mut().unwrap().remove("root");
        assert!(serde_json::from_value::<Disks>(disks).is_err());
    }

    #[test]
    fn recovery_identity_proof_requires_the_latest_successful_unseal_and_pinned_key() {
        let log = "Starting Unseal initrd recovery SSH identity...\n256 SHA256:expected root@predator (ED25519)\nFinished Unseal initrd recovery SSH identity.";
        assert!(recovery_key_proof(log, "SHA256:expected"));
        assert!(!recovery_key_proof(log, "SHA256:other"));
        assert!(!recovery_key_proof(
            &log.replace("Finished", "Failed"),
            "SHA256:expected"
        ));
        assert!(!recovery_key_proof(
            &format!(
                "{log}\nStarting Unseal initrd recovery SSH identity...\n256 SHA256:other\nFinished Unseal initrd recovery SSH identity."
            ),
            "SHA256:expected"
        ));
    }

    #[test]
    fn unavailable_tpm_or_plugin_is_not_a_phase_rejection() {
        assert!(phase_policy_rejected(
            "No signature for current PCR policy in TPM2 signature JSON, token does not apply to current boot state: Device not a stream."
        ));
        for diagnostic in [
            "Parsed TPM2 PCR mask 0x80. Failed to open TPM device.",
            "Failed to load libcryptsetup-token-systemd-tpm2.so",
            "TPM signature JSON is missing",
        ] {
            assert!(!phase_policy_rejected(diagnostic));
        }
    }

    #[test]
    fn token_unlock_proof_rejects_partial_or_wrong_slot_traces() {
        let log = "Completed TPM2 key unsealing in 1s.\nTrying to open keyslot 2 with token 0 (type systemd-tpm2).\nTrying to open LUKS2 keyslot 2.\nVerifying key from keyslot 2, digest 0.\nLoading key (type logon, name cryptsetup:key) in keyring.\ndm resume crypt-root  [ opencount flush securedata ]\nFinished Cryptography Setup for crypt-root.";
        assert!(boot_slot_proof(log, "0", "2", "crypt-root"));
        assert!(!boot_slot_proof(log, "1", "2", "crypt-root"));
        assert!(!boot_slot_proof(log, "0", "3", "crypt-root"));
        assert!(!boot_slot_proof(log, "0", "2", "fast-crypt"));
        for boundary in [
            "Digest 0 verify failed with -1.",
            "Token activation unsuccessful.",
            "Beginning attempt 1 to unlock.",
            "Trying to open LUKS2 keyslot 0.",
            "Key slot 0 unlocked.",
            "Starting Cryptography Setup for crypt-root...",
        ] {
            let recovery = log.replace(
                "Loading key (type logon,",
                &format!("{boundary}\nLoading key (type logon,"),
            );
            assert!(
                !boot_slot_proof(&recovery, "0", "2", "crypt-root"),
                "{boundary}"
            );
        }
        assert!(!boot_slot_proof(
            log.split("Finished").next().unwrap(),
            "0",
            "2",
            "crypt-root"
        ));
    }

    #[test]
    fn efi_payload_paths_cannot_escape_the_authenticated_directory() {
        for invalid in [
            "EFI/nixos/x",
            "\\EFI\\nixos\\..\\..\\x",
            "\\EFI\\nixos\\/etc/passwd",
            "\\other\\x",
        ] {
            assert!(
                efi_path(Path::new("/boot"), invalid.as_bytes()).is_err(),
                "{invalid}"
            );
        }
        assert_eq!(
            efi_path(Path::new("/boot"), b"\\EFI\\nixos\\initrd-x\0").unwrap(),
            Path::new("/boot/EFI/nixos/initrd-x")
        );
    }

    fn pe_image() -> Vec<u8> {
        let mut image = vec![0; 768];
        image[..2].copy_from_slice(b"MZ");
        image[0x3c..0x40].copy_from_slice(&64u32.to_le_bytes());
        image[64..68].copy_from_slice(b"PE\0\0");
        image[68..70].copy_from_slice(&0x8664u16.to_le_bytes());
        image[70..72].copy_from_slice(&1u16.to_le_bytes());
        image[84..86].copy_from_slice(&240u16.to_le_bytes());
        image[86..88].copy_from_slice(&0x22u16.to_le_bytes());
        image[88..90].copy_from_slice(&0x20bu16.to_le_bytes());
        image[120..124].copy_from_slice(&4096u32.to_le_bytes());
        image[124..128].copy_from_slice(&512u32.to_le_bytes());
        image[144..148].copy_from_slice(&8192u32.to_le_bytes());
        image[148..152].copy_from_slice(&512u32.to_le_bytes());
        image[156..158].copy_from_slice(&10u16.to_le_bytes());
        image[196..200].copy_from_slice(&16u32.to_le_bytes());
        image[328..336].copy_from_slice(b".osrel\0\0");
        image[336..340].copy_from_slice(&4u32.to_le_bytes());
        image[340..344].copy_from_slice(&4096u32.to_le_bytes());
        image[344..348].copy_from_slice(&8u32.to_le_bytes());
        image[348..352].copy_from_slice(&512u32.to_le_bytes());
        image[364..368].copy_from_slice(&0x40000040u32.to_le_bytes());
        image[512..516].copy_from_slice(b"test");
        image
    }

    #[test]
    fn pe_measurements_exclude_padding_and_reject_truncation_and_duplicates() {
        let mut image = pe_image();
        assert_eq!(sections(&image).unwrap()[".osrel"], b"test");
        image[336..340].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(sections(&image).unwrap()[".osrel"], b"");
        image[336..340].copy_from_slice(&9u32.to_le_bytes());
        assert!(sections(&image).is_err());
        assert!(sections(&image[..80]).is_err());
        let mut image = pe_image();
        image[70..72].copy_from_slice(&2u16.to_le_bytes());
        image.copy_within(328..368, 368);
        assert!(sections(&image).is_err());
    }

    #[test]
    fn luks_tokens_distinguish_other_credentials_from_malformed_tpm_policies() {
        let header: LuksHeader = serde_json::from_value(json!({"tokens":{"0":{"type":"systemd-fido2","keyslots":["0"]}, "1":{"type":"systemd-tpm2","keyslots":["2"],"tpm2-pcrs":[7],"tpm2-pcr-bank":"sha256","tpm2_pubkey_pcrs":[11],"tpm2_pubkey":"public"}}})).unwrap();
        assert!(matches!(header.tokens["0"], Token::Other));
        assert!(matches!(header.tokens["1"], Token::Tpm(_)));
        let mut token = json!({"keyslots":["2"],"tpm2-pcrs":[7],"tpm2-pcr-bank":"sha256","tpm2_pubkey_pcrs":[11],"tpm2_pubkey":"public"});
        token["tpm2-pcrs"] = json!([0, 2, 7]);
        assert!(serde_json::from_value::<TpmToken>(token.clone()).is_err());
        token["tpm2-pcrs"] = json!([7]);
        token["tpm2_pubkey_pcrs"] = json!([]);
        assert!(serde_json::from_value::<TpmToken>(token.clone()).is_err());
        token["tpm2_pubkey_pcrs"] = json!([11]);
        token["tpm2_pubkey"] = json!(null);
        assert!(serde_json::from_value::<TpmToken>(token).is_err());
        assert!(
            serde_json::from_value::<LuksHeader>(json!({"tokens":{"0":{"type":"systemd-tpm2"}}}))
                .is_err()
        );
        assert!(
            serde_json::from_value::<SignatureSet<[PolicySignature; 2]>>(json!({"sha256":[]}))
                .is_err()
        );
    }

    #[test]
    fn measured_inputs_reject_changed_payloads_and_unknown_sections() {
        let esp = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let payloads = esp.path().join("EFI/nixos");
        fs::create_dir_all(&payloads).unwrap();
        fs::write(payloads.join("kernel"), b"kernel").unwrap();
        fs::write(payloads.join("initrd"), b"initrd").unwrap();
        let kernel_hash = Sha256::digest(b"kernel");
        let initrd_hash = Sha256::digest(b"initrd");
        let mut sections = BTreeMap::from([
            (".linux", b"\\EFI\\nixos\\kernel".as_slice()),
            (".linuxh", kernel_hash.as_slice()),
            (".initrd", b"\\EFI\\nixos\\initrd".as_slice()),
            (".initrdh", initrd_hash.as_slice()),
            (".osrel", b"ID=nixos".as_slice()),
            (".cmdline", b"init=/nix/store/system/init".as_slice()),
        ]);
        assert!(inputs(&sections, esp.path(), work.path()).is_ok());
        sections.insert(".profile", b"unsupported");
        assert!(inputs(&sections, esp.path(), work.path()).is_err());
        sections.remove(".profile");
        fs::write(payloads.join("kernel"), b"changed").unwrap();
        assert!(
            inputs(&sections, esp.path(), work.path())
                .unwrap_err()
                .to_string()
                .contains("hash mismatch")
        );
    }

    fn entry(images: &Path, id: &str, flagged: bool) -> BootEntry {
        BootEntry {
            id: id.into(),
            path: Some(images.join(id)),
            is_default: flagged,
        }
    }

    #[test]
    fn boot_image_skips_stale_efivar_entries_and_specialisations() {
        let esp = tempfile::tempdir().unwrap();
        let images = esp.path().join("EFI/Linux");
        let entries = [
            BootEntry {
                id: "nixos-generation-151-zmdz.efi".into(),
                path: Some("/sys/firmware/efi/efivars/LoaderEntries".into()),
                is_default: true,
            },
            entry(&images, "nixos-generation-155-aaaa.efi", false),
            entry(&images, "nixos-generation-156-bbbb.efi", false),
            entry(
                &images,
                "nixos-generation-157-specialisation-pm-study-cccc.efi",
                false,
            ),
        ];
        let selected = boot_image(&entries, esp.path()).unwrap();
        assert_eq!(selected.id, "nixos-generation-156-bbbb.efi");
    }

    #[test]
    fn boot_image_keeps_a_flagged_image_that_is_installed() {
        let esp = tempfile::tempdir().unwrap();
        let images = esp.path().join("EFI/Linux");
        let entries = [
            entry(&images, "nixos-generation-155-aaaa.efi", true),
            entry(&images, "nixos-generation-156-bbbb.efi", false),
        ];
        let selected = boot_image(&entries, esp.path()).unwrap();
        assert_eq!(selected.id, "nixos-generation-155-aaaa.efi");
    }

    #[test]
    fn boot_image_fails_when_no_generation_is_installed() {
        let esp = tempfile::tempdir().unwrap();
        let error = boot_image(&[], esp.path()).unwrap_err();
        assert!(error.to_string().contains("no managed Predator generation"));
    }

    #[test]
    fn rollback_restores_default_payloads_and_only_retained_companions() {
        let esp = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let image = esp.path().join("EFI/Linux/nixos-old.efi");
        let kernel = esp.path().join("EFI/nixos/kernel-old");
        let initrd = esp.path().join("EFI/nixos/initrd-old");
        let removed = esp.path().join("EFI/Linux/nixos-removed.efi");
        let files = [
            "EFI/Linux/nixos-old.efi",
            "EFI/nixos/kernel-old",
            "EFI/nixos/initrd-old",
        ]
        .map(PathBuf::from);
        for (relative, bytes) in files
            .iter()
            .zip([b"signed UKI".as_slice(), b"kernel", b"initrd"])
        {
            let backup = work.path().join(relative);
            fs::create_dir_all(backup.parent().unwrap()).unwrap();
            fs::write(&backup, bytes).unwrap();
        }
        for (target, bytes) in [(&image, b"signature".as_slice()), (&removed, b"stale")] {
            let backup = companion(&work.path().join(target.strip_prefix(esp.path()).unwrap()));
            fs::create_dir_all(backup.parent().unwrap()).unwrap();
            fs::write(backup, bytes).unwrap();
        }
        let snapshot = BootSnapshot {
            default: "nixos-old.efi".into(),
            artifacts: BTreeSet::from([
                image.clone(),
                kernel.clone(),
                initrd.clone(),
                removed.clone(),
            ]),
            work,
        };
        fs::create_dir_all(esp.path().join("EFI/Linux")).unwrap();
        fs::create_dir_all(esp.path().join("EFI/nixos")).unwrap();
        let updated = esp.path().join("EFI/Linux/nixos-new.efi");
        let new_initrd = esp.path().join("EFI/nixos/initrd-new");
        let foreign = esp.path().join("EFI/Linux/foreign.efi");
        let directory = esp.path().join("EFI/Linux/nixos-directory.efi");
        fs::create_dir(&directory).unwrap();
        fs::write(&kernel, b"shared old kernel").unwrap();
        fs::write(&updated, b"new EFI").unwrap();
        fs::create_dir_all(companion(&updated).parent().unwrap()).unwrap();
        fs::write(companion(&updated), b"new signature").unwrap();
        fs::write(&new_initrd, vec![0; 65536]).unwrap();
        fs::write(&foreign, b"foreign EFI").unwrap();
        snapshot.discard_new_artifacts(esp.path()).unwrap();
        assert!(!updated.exists());
        assert!(!companion(&updated).exists());
        assert!(!new_initrd.exists());
        assert_eq!(fs::read(&kernel).unwrap(), b"shared old kernel");
        assert_eq!(fs::read(&foreign).unwrap(), b"foreign EFI");
        assert!(directory.is_dir());
        snapshot.restore_files(esp.path()).unwrap();
        assert_eq!(fs::read(&image).unwrap(), b"signed UKI");
        assert_eq!(fs::read(&kernel).unwrap(), b"kernel");
        assert_eq!(fs::read(&initrd).unwrap(), b"initrd");
        assert_eq!(fs::read(companion(&image)).unwrap(), b"signature");
        assert!(!companion(&removed).exists());
    }
}
