//! 系統設定（`config.conf`，`key=value`）。
//!
//! 只放系統層級設定；公鑰、白名單、頻道、排程都存在資料庫，由 TUI / 管理客戶端修改。
//! Secret（Discord token）只存檔案路徑，用到時才讀檔。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use jiff::tz::TimeZone;

/// `config.conf` 格式版本；新增或更名設定時加一，並在 setup 腳本寫升級步驟。
pub const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct Settings {
    pub listen: String,
    pub port: u16,
    pub database: PathBuf,
    pub host_key: PathBuf,
    /// 伺服器 TUI 掃描公鑰的預設目錄。
    pub keys_dir: PathBuf,
    pub discord_token_file: PathBuf,
    pub timezone_name: String,
    pub timezone: TimeZone,
    pub reload_interval_ms: u64,
}

impl Settings {
    pub fn config_path() -> PathBuf {
        std::env::var_os("WHATPLUS_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/config/config.conf"))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("cannot read config file {}", path.display()))?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self> {
        let map = parse_pairs(text);
        let get = |key: &str, default: &str| -> String {
            map.get(key).cloned().unwrap_or_else(|| default.to_string())
        };

        let version: u32 = get("config.version", "1")
            .parse()
            .context("config.version must be a number")?;
        if version > CONFIG_VERSION {
            bail!(
                "config.conf has format version {version}, but this build supports up to {CONFIG_VERSION}. \
                 It was written by a newer release; upgrade the server instead of downgrading."
            );
        }

        let timezone_name = get("timezone", "UTC");
        let timezone = TimeZone::get(&timezone_name)
            .with_context(|| format!("timezone: unknown time zone {timezone_name}"))?;
        Ok(Settings {
            listen: get("ssh.listen", "0.0.0.0"),
            port: get("ssh.port", &whatplus_proto::DEFAULT_PORT.to_string())
                .parse()
                .context("ssh.port must be a port number")?,
            database: get("database", "/data/whatplus.db").into(),
            host_key: get("hostKey", "/data/ssh_host_ed25519_key").into(),
            keys_dir: get("keys.dir", "/keys").into(),
            discord_token_file: get("discord.token", "/run/secrets/discord-token").into(),
            timezone_name,
            timezone,
            reload_interval_ms: get("reload.intervalMs", "5000")
                .parse()
                .context("reload.intervalMs must be a number")?,
        })
    }

    /// 讀取 Discord token。錯誤訊息只包含設定名稱與路徑，不包含內容。
    pub fn discord_token(&self) -> Result<String> {
        let token = fs::read_to_string(&self.discord_token_file).with_context(|| {
            format!(
                "discord.token: secret file {} is not readable",
                self.discord_token_file.display()
            )
        })?;
        let token = token.trim().to_string();
        if token.is_empty() {
            bail!(
                "discord.token: secret file {} is empty",
                self.discord_token_file.display()
            );
        }
        Ok(token)
    }
}

fn parse_pairs(text: &str) -> HashMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_overrides() {
        let s = Settings::parse("# c\nssh.port=2200\ntimezone=Asia/Taipei\n").unwrap();
        assert_eq!(s.port, 2200);
        assert_eq!(s.listen, "0.0.0.0");
        assert_eq!(s.timezone_name, "Asia/Taipei");
    }

    #[test]
    fn rejects_newer_version_and_bad_timezone() {
        assert!(Settings::parse("config.version=99").is_err());
        assert!(Settings::parse("timezone=Mars/Base").is_err());
    }
}
