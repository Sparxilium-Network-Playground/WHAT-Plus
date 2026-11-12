//! daemon 共用的狀態。

use std::sync::Arc;

use jiff::{Timestamp, Zoned};

use crate::auth::AuthHolder;
use crate::config::Settings;
use crate::db::Store;
use crate::discord::Discord;

pub struct App {
    pub store: Arc<Store>,
    pub auth: Arc<AuthHolder>,
    pub discord: Discord,
    pub settings: Settings,
    pub host_fingerprint: String,
}

impl App {
    pub fn now(&self) -> Zoned {
        Timestamp::now().to_zoned(self.settings.timezone.clone())
    }

    /// 設定修改後立即重新載入驗證快照（不等輪詢）。
    pub fn reload_auth(&self) {
        if let Err(e) = self.auth.refresh(&self.store) {
            tracing::warn!("reloading authorization settings failed: {e:#}");
        }
    }
}
