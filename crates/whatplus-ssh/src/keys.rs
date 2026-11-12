//! 金鑰：解析、掃描、指紋與產生。只接受 Ed25519。

use std::fs;
use std::path::{Path, PathBuf};

use russh::keys::ssh_key::{
    self, HashAlg, LineEnding,
    private::{Ed25519Keypair, KeypairData},
};
use russh::keys::{Algorithm, PrivateKey, PublicKey};

/// 公鑰檔的大小上限（authorized_keys 可能有很多行，但不會到 1 MB）。
const MAX_PUBLIC_FILE: u64 = 1024 * 1024;
const MAX_PRIVATE_FILE: u64 = 64 * 1024;
const PRIVATE_HEADER: &str = "-----BEGIN OPENSSH PRIVATE KEY-----";

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("{0} is not an OpenSSH private key")]
    NotPrivateKey(String),
    #[error("the key is protected by a passphrase")]
    NeedsPassphrase,
    #[error("wrong passphrase")]
    WrongPassphrase,
    #[error("unsupported key type {0}; only ssh-ed25519 is supported")]
    Unsupported(String),
    #[error("invalid key: {0}")]
    Invalid(String),
}

/// `SHA256:...` 指紋。
pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

pub fn is_supported(key: &PublicKey) -> bool {
    key.algorithm() == Algorithm::Ed25519
}

/// 比對金鑰本身，忽略註解。
pub fn same_key(a: &PublicKey, b: &PublicKey) -> bool {
    a.key_data() == b.key_data()
}

/// OpenSSH 單行格式。
pub fn to_line(key: &PublicKey) -> String {
    key.to_openssh().unwrap_or_default()
}

pub fn comment(key: &PublicKey) -> String {
    key.comment().as_str_lossy().to_string()
}

/// 解析 `.pub` 或 `authorized_keys` 的一行。允許前面帶 authorized_keys 選項。
pub fn parse_public_key_line(line: &str) -> Result<PublicKey, KeyError> {
    let line = line.trim();
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let start = tokens
        .iter()
        .position(|t| t.starts_with("ssh-") || t.starts_with("ecdsa-") || t.starts_with("sk-"))
        .ok_or_else(|| KeyError::Invalid("no key type found".into()))?;
    let rest = tokens[start..].join(" ");
    PublicKey::from_openssh(&rest).map_err(|e| KeyError::Invalid(e.to_string()))
}

/// 解析單一 Ed25519 公鑰（給使用者貼上的公鑰使用）。
pub fn parse_ed25519_public_key(line: &str) -> Result<PublicKey, KeyError> {
    let key = parse_public_key_line(line)?;
    if !is_supported(&key) {
        return Err(KeyError::Unsupported(key.algorithm().to_string()));
    }
    Ok(key)
}

/// 掃描到的一把公鑰。一個檔案可能有多行，每行一筆。
#[derive(Debug, Clone)]
pub struct FoundPublicKey {
    pub path: PathBuf,
    /// 從 1 開始的行號。
    pub line: usize,
    pub result: Result<PublicKey, String>,
}

impl FoundPublicKey {
    pub fn supported_key(&self) -> Option<&PublicKey> {
        self.result.as_ref().ok().filter(|k| is_supported(k))
    }

    /// 給畫面顯示的說明：指紋＋類型＋註解，或錯誤原因。
    pub fn describe(&self) -> String {
        match &self.result {
            Ok(key) if is_supported(key) => {
                format!("{}  {}", fingerprint(key), comment(key))
            }
            Ok(key) => format!("unsupported {} ({})", key.algorithm(), comment(key)),
            Err(e) => format!("unreadable: {e}"),
        }
    }
}

fn is_public_key_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    name.ends_with(".pub") || name.starts_with("authorized_keys")
}

/// 列出目錄（不遞迴）中 `*.pub` 與 `authorized_keys*` 裡的每一把公鑰。
pub fn scan_public_keys(dir: &Path) -> Result<Vec<FoundPublicKey>, KeyError> {
    let mut found = Vec::new();
    for path in sorted_files(dir)? {
        if !is_public_key_file(&path) {
            continue;
        }
        let Ok(text) = read_limited(&path, MAX_PUBLIC_FILE) else {
            continue;
        };
        for (idx, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            found.push(FoundPublicKey {
                path: path.clone(),
                line: idx + 1,
                result: parse_public_key_line(line).map_err(|e| e.to_string()),
            });
        }
    }
    Ok(found)
}

/// 掃描到的私鑰檔（不解密；指紋取自檔案中未加密的公鑰部分）。
#[derive(Debug, Clone)]
pub struct FoundPrivateKey {
    pub path: PathBuf,
    pub encrypted: bool,
    pub supported: bool,
    pub fingerprint: String,
    pub comment: String,
}

/// 列出目錄（不遞迴）中的 OpenSSH 私鑰檔。
pub fn scan_private_keys(dir: &Path) -> Result<Vec<FoundPrivateKey>, KeyError> {
    let mut found = Vec::new();
    for path in sorted_files(dir)? {
        if is_public_key_file(&path) {
            continue;
        }
        let Ok(text) = read_limited(&path, MAX_PRIVATE_FILE) else {
            continue;
        };
        if !text.trim_start().starts_with(PRIVATE_HEADER) {
            continue;
        }
        let Ok(key) = PrivateKey::from_openssh(text.as_bytes()) else {
            continue;
        };
        found.push(FoundPrivateKey {
            path,
            encrypted: key.is_encrypted(),
            supported: key.algorithm() == Algorithm::Ed25519,
            fingerprint: fingerprint(key.public_key()),
            comment: comment(key.public_key()),
        });
    }
    Ok(found)
}

/// 讀取 Ed25519 私鑰；有 passphrase 時需提供。
pub fn load_private_key(path: &Path, passphrase: Option<&str>) -> Result<PrivateKey, KeyError> {
    let text = read_limited(path, MAX_PRIVATE_FILE)?;
    if !text.trim_start().starts_with(PRIVATE_HEADER) {
        return Err(KeyError::NotPrivateKey(path.display().to_string()));
    }
    let key =
        PrivateKey::from_openssh(text.as_bytes()).map_err(|e| KeyError::Invalid(e.to_string()))?;
    if key.algorithm() != Algorithm::Ed25519 {
        return Err(KeyError::Unsupported(key.algorithm().to_string()));
    }
    if !key.is_encrypted() {
        return Ok(key);
    }
    match passphrase {
        None | Some("") => Err(KeyError::NeedsPassphrase),
        Some(p) => key.decrypt(p).map_err(|_| KeyError::WrongPassphrase),
    }
}

/// 產生新的 Ed25519 金鑰對，回傳（OpenSSH 私鑰文字, 公鑰單行）。
pub fn generate_ed25519(comment: &str) -> Result<(String, String), KeyError> {
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).map_err(|e| KeyError::Invalid(e.to_string()))?;
    let keypair = Ed25519Keypair::from_seed(&seed);
    seed.fill(0);
    let key = PrivateKey::new(KeypairData::from(keypair), comment)
        .map_err(|e: ssh_key::Error| KeyError::Invalid(e.to_string()))?;
    let private = key
        .to_openssh(LineEnding::LF)
        .map_err(|e| KeyError::Invalid(e.to_string()))?
        .to_string();
    Ok((private, to_line(key.public_key())))
}

/// 寫入私鑰檔（Unix 上權限 0600）。檔案已存在時失敗，避免覆蓋既有金鑰。
pub fn write_new_private_key(path: &Path, contents: &str) -> Result<(), KeyError> {
    use std::io::Write;
    let io_err = |source| KeyError::Io {
        path: path.display().to_string(),
        source,
    };
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(io_err)?;
    file.write_all(contents.as_bytes()).map_err(io_err)?;
    Ok(())
}

fn sorted_files(dir: &Path) -> Result<Vec<PathBuf>, KeyError> {
    let entries = fs::read_dir(dir).map_err(|source| KeyError::Io {
        path: dir.display().to_string(),
        source,
    })?;
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    Ok(files)
}

fn read_limited(path: &Path, limit: u64) -> Result<String, KeyError> {
    let io_err = |source| KeyError::Io {
        path: path.display().to_string(),
        source,
    };
    let meta = fs::metadata(path).map_err(io_err)?;
    if meta.len() > limit {
        return Err(KeyError::Invalid(format!(
            "{} is larger than {limit} bytes",
            path.display()
        )));
    }
    fs::read_to_string(path).map_err(io_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_parse_and_scan() {
        let dir = tempfile::tempdir().unwrap();
        let (private, public) = generate_ed25519("test@host").unwrap();
        let key_path = dir.path().join("id_test");
        write_new_private_key(&key_path, &private).unwrap();
        assert!(write_new_private_key(&key_path, &private).is_err());

        let other = generate_ed25519("other").unwrap().1;
        fs::write(
            dir.path().join("multi.pub"),
            format!("# comment\n{public}\nrestrict,from=\"10.0.0.1\" {other}\nssh-rsa AAAA bad\n"),
        )
        .unwrap();

        let pubs = scan_public_keys(dir.path()).unwrap();
        assert_eq!(pubs.len(), 3);
        assert!(pubs[0].supported_key().is_some());
        assert!(pubs[1].supported_key().is_some());
        assert!(pubs[2].result.is_err());

        let privs = scan_private_keys(dir.path()).unwrap();
        assert_eq!(privs.len(), 1);
        assert!(!privs[0].encrypted);

        let loaded = load_private_key(&key_path, None).unwrap();
        let parsed = parse_ed25519_public_key(&public).unwrap();
        assert!(same_key(loaded.public_key(), &parsed));
        assert_eq!(fingerprint(&parsed), privs[0].fingerprint);
        assert!(fingerprint(&parsed).starts_with("SHA256:"));
    }
}
