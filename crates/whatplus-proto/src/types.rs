//! 協定中使用的資料型別。

use serde::{Deserialize, Serialize};

/// 公鑰的角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// 管理客戶端：設定報告、頻道、來源公鑰。
    Admin,
    /// 來源端：只能上傳用量紀錄。
    Agent,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Agent => "agent",
        }
    }

    pub fn parse(s: &str) -> Option<Role> {
        match s {
            "admin" => Some(Role::Admin),
            "agent" => Some(Role::Agent),
            _ => None,
        }
    }
}

/// 報告語言（每個 Discord 目的地各自設定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Language {
    #[default]
    #[serde(rename = "en")]
    En,
    #[serde(rename = "zh-TW")]
    ZhTw,
}

impl Language {
    pub fn code(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::ZhTw => "zh-TW",
        }
    }

    /// 未知代碼回退為英文。
    pub fn from_code(code: &str) -> Language {
        match code {
            "zh-TW" => Language::ZhTw,
            _ => Language::En,
        }
    }
}

/// 已註冊的公鑰。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyInfo {
    pub id: i64,
    pub name: String,
    pub role: Role,
    /// `SHA256:...`
    pub fingerprint: String,
    /// OpenSSH 單行格式（`ssh-ed25519 AAAA... comment`）。
    pub public_key: String,
    /// 這把公鑰另外限制的 CIDR；空陣列表示只套用全域白名單。
    pub allowed_cidrs: Vec<String>,
    pub enabled: bool,
    /// RFC 3339；從未連線時為 `None`。
    pub last_seen_at: Option<String>,
    pub last_seen_ip: Option<String>,
    /// 此來源上傳的紀錄數（admin 為 0）。
    pub record_count: u64,
}

/// 新增或修改來源公鑰。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentKeyInput {
    pub name: String,
    pub public_key: String,
    pub allowed_cidrs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscordChannel {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscordGuild {
    pub id: String,
    pub name: String,
    pub channels: Vec<DiscordChannel>,
}

/// 報告的 Discord 目的地。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Destination {
    /// 新增時填 0。
    pub id: i64,
    pub name: String,
    pub guild_id: String,
    pub guild_name: String,
    pub channel_id: String,
    pub channel_name: String,
    pub language: Language,
    pub enabled: bool,
    /// 最近一次發送結果（唯讀，伺服器填入）。
    #[serde(default)]
    pub last_result: Option<String>,
}

/// 報告要包含的段落。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportContent {
    pub totals: bool,
    pub by_tool: bool,
    pub by_model: bool,
    pub by_machine: bool,
    pub top_projects: bool,
    pub tokens: bool,
}

impl Default for ReportContent {
    fn default() -> Self {
        Self {
            totals: true,
            by_tool: true,
            by_model: true,
            by_machine: true,
            top_projects: false,
            tokens: true,
        }
    }
}

/// 排程週期。時間以伺服器設定的時區解讀。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScheduleKind {
    /// 每天 `hour:minute` 送前一天。
    Daily { hour: u8, minute: u8 },
    /// 每週 `weekday`（1 = 週一 … 7 = 週日）`hour:minute` 送前 7 天。
    Weekly { weekday: u8, hour: u8, minute: u8 },
    /// 每月 1 號 `hour:minute` 送上個月。
    Monthly { hour: u8, minute: u8 },
}

impl ScheduleKind {
    pub fn validate(&self) -> Result<(), String> {
        let (hour, minute) = match *self {
            ScheduleKind::Daily { hour, minute } | ScheduleKind::Monthly { hour, minute } => {
                (hour, minute)
            }
            ScheduleKind::Weekly {
                weekday,
                hour,
                minute,
            } => {
                if !(1..=7).contains(&weekday) {
                    return Err("weekday must be 1-7".into());
                }
                (hour, minute)
            }
        };
        if hour > 23 || minute > 59 {
            return Err("time must be within 00:00-23:59".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Schedule {
    /// 新增時填 0。
    pub id: i64,
    pub name: String,
    pub kind: ScheduleKind,
    pub destination_ids: Vec<i64>,
    pub content: ReportContent,
    pub enabled: bool,
    /// 最近一次執行的時間（唯讀，RFC 3339）。
    #[serde(default)]
    pub last_run_at: Option<String>,
}

/// 手動推送的期間。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManualRange {
    Today,
    Yesterday,
    Last7Days,
    ThisMonth,
    LastMonth,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerStatus {
    pub server_version: String,
    pub timezone: String,
    pub host_fingerprint: String,
    /// Discord 連線狀態的說明文字（英文）。
    pub discord: String,
    pub agents: u64,
    pub records: u64,
    pub allowed_cidrs: Vec<String>,
    pub last_report_at: Option<String>,
    pub last_report_result: Option<String>,
}
