//! 來源端設定與狀態（JSON，放在資料目錄）。
//!
//! - `agent.json`：TUI 編輯的設定（伺服器位址、私鑰路徑、已確認的伺服器指紋、上傳間隔）
//! - `state.json`：daemon 寫入的執行狀態（最近一次上傳結果）

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub const DEFAULT_INTERVAL_MINUTES: u32 = 10;
pub const MIN_INTERVAL_MINUTES: u32 = 1;
pub const MAX_INTERVAL_MINUTES: u32 = 1440;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    pub server_host: String,
    pub server_port: u16,
    /// 私鑰檔路徑（不能有 passphrase；容器內無法輸入）。
    pub key_path: Option<PathBuf>,
    /// 使用者確認過的伺服器指紋（TOFU）。
    pub server_fingerprint: Option<String>,
    pub interval_minutes: u32,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            server_host: String::new(),
            server_port: whatplus_proto::DEFAULT_PORT,
            key_path: None,
            server_fingerprint: None,
            interval_minutes: DEFAULT_INTERVAL_MINUTES,
        }
    }
}

impl AgentConfig {
    /// 三項都設定好才會上傳；否則 daemon 只等待。
    pub fn missing(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.server_host.trim().is_empty() {
            out.push("server address");
        }
        if self.key_path.is_none() {
            out.push("private key");
        }
        if self.server_fingerprint.is_none() {
            out.push("verified server fingerprint");
        }
        out
    }

    pub fn interval(&self) -> u32 {
        self.interval_minutes
            .clamp(MIN_INTERVAL_MINUTES, MAX_INTERVAL_MINUTES)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentState {
    pub last_attempt_at: Option<String>,
    pub last_success_at: Option<String>,
    pub last_result: Option<String>,
}

pub struct Paths {
    pub data_dir: PathBuf,
    /// TUI 掃描私鑰的預設目錄。
    pub keys_dir: PathBuf,
}

impl Paths {
    pub fn from_env() -> Self {
        let get = |name: &str, default: &str| {
            std::env::var_os(name)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(default))
        };
        Self {
            data_dir: get("WHATPLUS_AGENT_DATA", "/data"),
            keys_dir: get("WHATPLUS_AGENT_KEYS", "/keys"),
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.data_dir.join("agent.json")
    }

    pub fn state_file(&self) -> PathBuf {
        self.data_dir.join("state.json")
    }

    pub fn load_config(&self) -> Result<AgentConfig> {
        load_json(&self.config_file())
    }

    pub fn save_config(&self, config: &AgentConfig) -> Result<()> {
        save_json(&self.config_file(), config)
    }

    pub fn load_state(&self) -> AgentState {
        load_json(&self.state_file()).unwrap_or_default()
    }

    pub fn save_state(&self, state: &AgentState) -> Result<()> {
        save_json(&self.state_file(), state)
    }
}

/// 檔案不存在時回傳預設值。
fn load_json<T: DeserializeOwned + Default>(path: &Path) -> Result<T> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).with_context(|| format!("{} is not valid JSON", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
    }
}

/// 先寫暫存檔再改名，避免 daemon 讀到寫一半的檔案。
fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(value)?)
        .with_context(|| format!("cannot write {} (is the data directory writable?)", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("cannot replace {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_missing() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            data_dir: dir.path().into(),
            keys_dir: dir.path().into(),
        };
        let cfg = paths.load_config().unwrap();
        assert_eq!(cfg.missing().len(), 3);
        let cfg = AgentConfig {
            server_host: "192.168.1.2".into(),
            key_path: Some("/keys/id".into()),
            server_fingerprint: Some("SHA256:x".into()),
            interval_minutes: 0,
            ..Default::default()
        };
        paths.save_config(&cfg).unwrap();
        let back = paths.load_config().unwrap();
        assert_eq!(back, cfg);
        assert!(back.missing().is_empty());
        assert_eq!(back.interval(), MIN_INTERVAL_MINUTES);
    }
}
