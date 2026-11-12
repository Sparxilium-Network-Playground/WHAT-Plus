//! Discord 發送（只發不收：不註冊指令、不監聽訊息、不用 privileged intent）。

mod gateway;
pub mod rest;

use std::sync::Arc;

use anyhow::{Result, anyhow};
use whatplus_proto::{Destination, DiscordGuild};

use crate::db::{Store, runtime};
use crate::report::format::split_messages;
use rest::Rest;

pub struct Discord {
    rest: Option<Arc<Rest>>,
    store: Arc<Store>,
}

impl Discord {
    /// token 讀不到時仍可啟動（狀態顯示錯誤），其他功能照常運作。
    pub fn new(token: Result<String>, store: Arc<Store>) -> Self {
        let rest = match token {
            Ok(token) => Some(Arc::new(Rest::new(token))),
            Err(e) => {
                tracing::error!("{e:#}");
                let _ = store.runtime_set(runtime::DISCORD_STATUS, &format!("ERROR {e:#}"));
                None
            }
        };
        Self { rest, store }
    }

    fn rest(&self) -> Result<Arc<Rest>> {
        self.rest
            .clone()
            .ok_or_else(|| anyhow!("Discord bot token is not configured (run setup on the server)"))
    }

    /// 第一次啟動時連一次 Gateway（Discord 的要求），之後只用 REST。
    pub async fn ensure_identified(&self, token: Option<String>) {
        let Some(token) = token else { return };
        if self
            .store
            .runtime_get(runtime::DISCORD_IDENTIFIED)
            .ok()
            .flatten()
            .is_some()
        {
            let _ = self.store.runtime_set(runtime::DISCORD_STATUS, "OK");
            return;
        }
        let result = tokio::task::spawn_blocking(move || gateway::identify_once(&token)).await;
        let status = match result {
            Ok(Ok(())) => {
                let _ = self.store.runtime_set(runtime::DISCORD_IDENTIFIED, "1");
                tracing::info!("Discord gateway identify succeeded");
                "OK".to_string()
            }
            Ok(Err(e)) => {
                tracing::warn!("{e:#}");
                format!("ERROR {e:#}")
            }
            Err(e) => format!("ERROR {e}"),
        };
        let _ = self.store.runtime_set(runtime::DISCORD_STATUS, &status);
    }

    pub async fn guilds(&self) -> Result<Vec<DiscordGuild>> {
        let rest = self.rest()?;
        tokio::task::spawn_blocking(move || rest.guilds()).await?
    }

    /// 把文字切成多則訊息依序送出；結果寫入 destination_health 並回傳說明。
    pub async fn deliver(&self, dest: &Destination, text: &str) -> String {
        let result = match self.rest() {
            Ok(rest) => {
                let channel = dest.channel_id.clone();
                let parts = split_messages(text);
                tokio::task::spawn_blocking(move || -> Result<()> {
                    for part in parts {
                        rest.send(&channel, &part)?;
                    }
                    Ok(())
                })
                .await
                .map_err(anyhow::Error::from)
                .and_then(|r| r)
            }
            Err(e) => Err(e),
        };
        let summary = match result {
            Ok(()) => "OK".to_string(),
            Err(e) => {
                tracing::warn!(destination = dest.id, "delivery failed: {e:#}");
                format!("FAILED {e:#}")
            }
        };
        let _ = self.store.set_destination_result(dest.id, &summary);
        format!("{}: {summary}", dest.name)
    }
}
