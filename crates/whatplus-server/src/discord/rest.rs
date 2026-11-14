//! Discord REST API（同步，於 `spawn_blocking` 內呼叫）。
//!
//! 只用到：列出 Bot 所在的 Server 與文字頻道、發送 embed 訊息。錯誤訊息不含 token。

use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use serde::Deserialize;
use serde_json::json;
use whatplus_proto::{DiscordChannel, DiscordGuild};

use crate::report::card::Card;

const API: &str = "https://discord.com/api/v10";
const USER_AGENT: &str = concat!(
    "DiscordBot (https://github.com, ",
    env!("CARGO_PKG_VERSION"),
    ")"
);

/// 文字頻道與公告頻道。
const SENDABLE_CHANNEL_TYPES: [u8; 2] = [0, 5];

pub struct Rest {
    token: String,
    agent: ureq::Agent,
}

#[derive(Deserialize)]
struct GuildJson {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct ChannelJson {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(rename = "type")]
    kind: u8,
    #[serde(default)]
    position: i64,
}

/// Discord snowflake 只能是數字；先檢查再放進 URL。
pub fn valid_snowflake(id: &str) -> bool {
    !id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit())
}

impl Rest {
    pub fn new(token: String) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .http_status_as_error(false)
            .build()
            .new_agent();
        Self { token, agent }
    }

    fn auth(&self) -> String {
        format!("Bot {}", self.token)
    }

    fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let mut resp = self
            .agent
            .get(&format!("{API}{path}"))
            .header("Authorization", &self.auth())
            .header("User-Agent", USER_AGENT)
            .call()
            .map_err(|e| anyhow!("Discord request failed: {e}"))?;
        let status = resp.status().as_u16();
        if status != 200 {
            bail!(
                "Discord API returned HTTP {status} for {}",
                redact_path(path)
            );
        }
        Ok(resp.body_mut().read_json()?)
    }

    pub fn guilds(&self) -> Result<Vec<DiscordGuild>> {
        let guilds: Vec<GuildJson> = self.get("/users/@me/guilds")?;
        let mut out = Vec::new();
        for g in guilds {
            if !valid_snowflake(&g.id) {
                continue;
            }
            let mut channels: Vec<ChannelJson> = self.get(&format!("/guilds/{}/channels", g.id))?;
            channels.retain(|c| SENDABLE_CHANNEL_TYPES.contains(&c.kind) && valid_snowflake(&c.id));
            channels.sort_by_key(|c| c.position);
            out.push(DiscordGuild {
                id: g.id,
                name: g.name,
                channels: channels
                    .into_iter()
                    .map(|c| DiscordChannel {
                        id: c.id,
                        name: c.name,
                    })
                    .collect(),
            });
        }
        Ok(out)
    }

    /// 把卡片發成一則 embed 訊息；`allowed_mentions` 為空，任何內容都不會觸發提及。
    /// 遇到 429 會等待後重試一次。Bot 在頻道需要 Embed Links 權限，否則 Discord 不顯示卡片。
    pub fn send_card(&self, channel_id: &str, card: &Card) -> Result<()> {
        if !valid_snowflake(channel_id) {
            bail!("invalid channel id");
        }
        let body = json!({ "embeds": [embed(card)], "allowed_mentions": { "parse": [] } });
        for attempt in 0..2 {
            let mut resp = self
                .agent
                .post(&format!("{API}/channels/{channel_id}/messages"))
                .header("Authorization", &self.auth())
                .header("User-Agent", USER_AGENT)
                .send_json(&body)
                .map_err(|e| anyhow!("Discord request failed: {e}"))?;
            let status = resp.status().as_u16();
            match status {
                200 | 201 => return Ok(()),
                429 if attempt == 0 => {
                    let wait = resp
                        .body_mut()
                        .read_json::<serde_json::Value>()
                        .ok()
                        .and_then(|v| v.get("retry_after").and_then(|r| r.as_f64()))
                        .unwrap_or(2.0)
                        .clamp(0.5, 30.0);
                    std::thread::sleep(Duration::from_secs_f64(wait));
                }
                401 => bail!("Discord rejected the bot token (HTTP 401)"),
                // 沒有 Embed Links 權限時，只有 embed 的訊息會被當成空訊息（HTTP 400）。
                400 => bail!(
                    "Discord rejected the message (HTTP 400); the bot may lack the Embed Links permission in this channel"
                ),
                403 => bail!(
                    "the bot has no permission to send messages or embed links in this channel (HTTP 403)"
                ),
                404 => bail!("channel not found or the bot is not in that server (HTTP 404)"),
                other => bail!("Discord API returned HTTP {other}"),
            }
        }
        bail!("Discord rate limit")
    }
}

/// Discord embed 物件。長度已由 [`Card::fit`] 限制。
fn embed(card: &Card) -> serde_json::Value {
    let fields: Vec<serde_json::Value> = card
        .fields
        .iter()
        .map(|f| json!({ "name": f.name, "value": f.value, "inline": false }))
        .collect();
    let mut e = json!({
        "title": card.title,
        "color": card.color,
        "fields": fields,
        "footer": { "text": card.footer },
        "timestamp": jiff::Timestamp::now().to_string(),
    });
    if !card.description.is_empty() {
        e["description"] = json!(card.description);
    }
    e
}

fn redact_path(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snowflake_validation() {
        assert!(valid_snowflake("123456789012345678"));
        assert!(!valid_snowflake("../users/@me"));
        assert!(!valid_snowflake(""));
    }
}
