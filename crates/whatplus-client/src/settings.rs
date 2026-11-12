//! 客戶端設定（`%APPDATA%\WHATPlus\config\client.json`）。
//!
//! 只存非敏感資料：伺服器位址、私鑰路徑、已確認的伺服器指紋、介面偏好。passphrase 不保存。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientSettings {
    pub host: String,
    pub port: u16,
    pub key_path: String,
    /// `host:port` → 已確認的伺服器指紋。
    pub known_hosts: BTreeMap<String, String>,
    /// `zh-TW` 或 `en`。
    pub language: String,
    /// 0 = 深色、1 = 淺色、2 = 跟隨系統
    pub theme: i32,
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: whatplus_proto::DEFAULT_PORT,
            key_path: String::new(),
            known_hosts: BTreeMap::new(),
            language: "zh-TW".into(),
            theme: 0,
        }
    }
}

pub fn host_key(host: &str, port: u16) -> String {
    format!("{}:{port}", host.trim())
}

pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    /// 預設在 `%APPDATA%\WHATPlus\config`；可用環境變數 `WHATPLUS_CLIENT_CONFIG` 指定檔案（測試用）。
    pub fn new() -> Self {
        if let Some(path) = std::env::var_os("WHATPLUS_CLIENT_CONFIG") {
            return Self {
                path: PathBuf::from(path),
            };
        }
        let dir = directories::ProjectDirs::from("", "", "WHATPlus")
            .map(|d| d.config_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        Self {
            path: dir.join("client.json"),
        }
    }

    #[cfg(test)]
    pub fn at(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
        }
    }

    /// 讀不到或損毀時回傳預設值（損毀檔改名保留，避免覆蓋使用者資料）。
    pub fn load(&self) -> ClientSettings {
        let Ok(text) = fs::read_to_string(&self.path) else {
            return ClientSettings::default();
        };
        match serde_json::from_str(&text) {
            Ok(s) => s,
            Err(_) => {
                let _ = fs::rename(&self.path, self.path.with_extension("json.corrupt"));
                ClientSettings::default()
            }
        }
    }

    pub fn save(&self, settings: &ClientSettings) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
        }
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(settings)?)?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// 預設的私鑰目錄（`%USERPROFILE%\.ssh`）。
pub fn default_ssh_dir() -> Option<PathBuf> {
    directories::UserDirs::new().map(|d| d.home_dir().join(".ssh"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_corrupt_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::at(&dir.path().join("client.json"));
        assert_eq!(store.load(), ClientSettings::default());
        let mut s = ClientSettings {
            host: "192.168.1.10".into(),
            ..Default::default()
        };
        s.known_hosts
            .insert(host_key("192.168.1.10", 33445), "SHA256:x".into());
        store.save(&s).unwrap();
        assert_eq!(store.load(), s);
        fs::write(store.path(), "{broken").unwrap();
        assert_eq!(store.load(), ClientSettings::default());
        assert!(dir.path().join("client.json.corrupt").exists());
    }
}
