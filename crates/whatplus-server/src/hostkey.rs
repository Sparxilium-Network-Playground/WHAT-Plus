//! 伺服器的 SSH host key：第一次啟動時產生並保存，之後沿用。

use std::path::Path;

use anyhow::{Context, Result, bail};
use whatplus_ssh::PrivateKey;
use whatplus_ssh::keys::{self, KeyError};

pub fn load_or_create(path: &Path) -> Result<PrivateKey> {
    if !path.exists() {
        let (private, _public) = keys::generate_ed25519("whatplus-server")?;
        keys::write_new_private_key(path, &private)
            .with_context(|| format!("hostKey: cannot create {}", path.display()))?;
        tracing::info!("generated a new SSH host key at {}", path.display());
    }
    match keys::load_private_key(path, None) {
        Ok(key) => Ok(key),
        Err(KeyError::NeedsPassphrase) => {
            bail!("hostKey: {} must not be protected by a passphrase", path.display())
        }
        Err(e) => Err(anyhow::anyhow!("hostKey: {}: {e}", path.display())),
    }
}

pub fn fingerprint(key: &PrivateKey) -> String {
    keys::fingerprint(key.public_key())
}
