//! 執行期狀態（runtime_status，key/value）。不 bump revision。

use anyhow::Result;

use super::{Store, now_rfc3339};
use crate::params;

pub const DAEMON_HEARTBEAT: &str = "daemon.heartbeat";
pub const DAEMON_STARTED_AT: &str = "daemon.started_at";
pub const DAEMON_RESTART_REQUEST: &str = "daemon.restart_request";
pub const HOST_FINGERPRINT: &str = "ssh.host_fingerprint";
pub const DISCORD_STATUS: &str = "discord.status";
pub const DISCORD_IDENTIFIED: &str = "discord.identified";
pub const LAST_REPORT_AT: &str = "report.last_at";
pub const LAST_REPORT_RESULT: &str = "report.last_result";
/// 最近一次被 IP 白名單拒絕的位址與時間。
pub const LAST_REFUSED: &str = "ssh.last_refused";

impl Store {
    pub fn runtime_get(&self, key: &str) -> Result<Option<String>> {
        self.lock().query_row(
            "SELECT value FROM runtime_status WHERE key = ?",
            params![key],
            |r| r.get(0),
        )
    }

    pub fn runtime_set(&self, key: &str, value: &str) -> Result<()> {
        self.lock().execute(
            "INSERT INTO runtime_status (key, value, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, now_rfc3339()],
        )?;
        Ok(())
    }

    /// 由 TUI 呼叫：寫入新的 token，daemon 發現 token 改變就結束，由 Docker 重新啟動。
    pub fn request_restart(&self) -> Result<String> {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("{e}"))?;
        let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        self.runtime_set(DAEMON_RESTART_REQUEST, &token)?;
        Ok(token)
    }
}
