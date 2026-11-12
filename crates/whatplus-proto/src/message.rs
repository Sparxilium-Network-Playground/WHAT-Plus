//! 請求與回應。伺服器依連線公鑰的角色決定哪些請求可以執行。

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use what_core::UsageRecord;

use crate::types::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// 連線後的第一個請求。
    Hello { client: String, protocol: u32 },

    // ---- agent ----
    /// 上傳一批紀錄（最多 [`crate::MAX_UPLOAD_BATCH`] 筆）。重複的紀錄會被覆寫，不會重複計算。
    Upload { records: Vec<UsageRecord> },
    /// 查詢伺服器上這個來源最新一筆紀錄的時間。
    UploadState,

    // ---- admin ----
    Status,
    ListKeys,
    AddAgentKey { key: AgentKeyInput },
    UpdateAgentKey { id: i64, name: String, allowed_cidrs: Vec<String> },
    SetAgentKeyEnabled { id: i64, enabled: bool },
    DeleteAgentKey { id: i64 },
    ListDiscordChannels,
    ListDestinations,
    SaveDestination { destination: Destination },
    DeleteDestination { id: i64 },
    TestDestination { id: i64 },
    ListSchedules,
    SaveSchedule { schedule: Schedule },
    DeleteSchedule { id: i64 },
    /// 產生報告文字但不發送。
    PreviewReport { range: ManualRange, content: ReportContent, language: Language },
    /// 立即發送報告到指定目的地。
    SendReport { range: ManualRange, content: ReportContent, destination_ids: Vec<i64> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Welcome { role: Role, name: String, server_version: String, protocol: u32 },
    Ok,
    /// 新增成功時回傳的 id。
    Saved { id: i64 },
    Error { message: String },
    Uploaded { accepted: usize },
    UploadState { latest: Option<Timestamp>, records: u64 },
    Status { status: ServerStatus },
    Keys { keys: Vec<KeyInfo> },
    Guilds { guilds: Vec<DiscordGuild> },
    Destinations { destinations: Vec<Destination> },
    Schedules { schedules: Vec<Schedule> },
    Preview { text: String },
    /// 每個目的地的發送結果。
    Sent { results: Vec<String> },
}

impl Response {
    pub fn error(message: impl Into<String>) -> Self {
        Response::Error {
            message: message.into(),
        }
    }
}
