use crate::util;
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, os::unix::fs::PermissionsExt, path::Path, process::Command};

/// Compare canonical public keys, not PEM formatting or a token's encoding.
#[derive(PartialEq, Eq)]
pub struct PublicKey {
    der: Vec<u8>,
    policy_fingerprint: String,
}

impl PublicKey {
    pub fn read(path: &Path) -> Result<Self> {
        let der = Self::read_der(path)?;
        let mut file = tempfile::NamedTempFile::new()?;
        file.write_all(&der)?;
        let rsa = util::command_output(Command::new("openssl").args([
            "rsa",
            "-pubin",
            "-inform",
            "DER",
            "-in",
            util::path(file.path())?,
            "-RSAPublicKey_out",
            "-outform",
            "DER",
        ]))
        .context("PCR signing authority must be an RSA public key")?;
        Ok(Self {
            der,
            policy_fingerprint: format!("{:x}", Sha256::digest(rsa)),
        })
    }

    fn read_der(path: &Path) -> Result<Vec<u8>> {
        util::command_output(Command::new("openssl").args([
            "pkey",
            "-pubin",
            "-in",
            util::path(path)?,
            "-outform",
            "DER",
        ]))
    }

    pub fn validate_private(&self, path: &Path) -> Result<()> {
        let derived = util::command_output(Command::new("openssl").args([
            "pkey",
            "-in",
            util::path(path)?,
            "-pubout",
            "-outform",
            "DER",
        ]))
        .with_context(|| {
            format!(
                "load signing key {}; restore missing keys from recovery",
                path.display()
            )
        })?;
        ensure!(
            derived == self.der,
            "signing key does not match its public key: {}",
            path.display()
        );
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        Ok(())
    }

    pub fn fingerprint(&self) -> String {
        format!("{:x}", Sha256::digest(&self.der))
    }

    /// systemd's RSA PCR-policy fingerprint hashes PKCS#1, not SubjectPublicKeyInfo.
    pub fn policy_fingerprint(&self) -> &str {
        &self.policy_fingerprint
    }

    pub fn matches_token_key(&self, encoded: &str) -> Result<bool> {
        let pem = STANDARD
            .decode(encoded)
            .context("invalid TPM public-key encoding")?;
        let mut file = tempfile::NamedTempFile::new()?;
        file.write_all(&pem)?;
        Ok(self.der == Self::read_der(file.path())?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn systemd_policy_fingerprint_uses_rsa_key_encoding() {
        let mut public = tempfile::NamedTempFile::new().unwrap();
        public
            .write_all(
                b"-----BEGIN PUBLIC KEY-----\n\
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAzaxJAXZgSxzPfHUxxPUQ\n\
ClmADXifSHbAWjoTytg9E+aLTphY/etYhHvcnLakHWSWZmkxPs6KQScCy7tL/1Ox\n\
Ch7mDL7olqN2T6q3vvCOiHG9Mq2Wt7kqBBp/+HYzSBnMbItzlUDeaK9VKgpSv0rp\n\
2d0vEt4qlDLKTkzzGkZnmutOJpNgRhR4w37rKzEmLMp2zfkkXxFb6iXUg3EbcVHb\n\
QuCIc1McKMqxeEq9hHDHt3m3uclHktptFhYJLV7frM4OQbewsEV0ClSK0rKMxyvX\n\
pdoFMx0ixNwJzZ5Mq7g/ClQ2PwmNZpB7ApLCwA4a/aoZGY445e2TX7/k78PdeEEE\n\
fQIDAQAB\n-----END PUBLIC KEY-----\n",
            )
            .unwrap();
        let key = PublicKey::read(public.path()).unwrap();
        assert_eq!(
            key.fingerprint(),
            "fb62995feb963f9f998e86592c6a601e11d178e2b27f04572ea2636141c91859"
        );
        assert_eq!(
            key.policy_fingerprint(),
            "9aa71cb048240f79402db08e03dd1a9750b936caee9354c503041df8939002c6"
        );
        let pem = fs::read(public.path()).unwrap();
        assert!(key.matches_token_key(&STANDARD.encode(&pem)).unwrap());
        let changed = String::from_utf8(pem)
            .unwrap()
            .replace("fQIDAQAB", "fwIDAQAB");
        assert!(!key.matches_token_key(&STANDARD.encode(changed)).unwrap());
        assert!(key.matches_token_key("invalid!").is_err());
    }
}
