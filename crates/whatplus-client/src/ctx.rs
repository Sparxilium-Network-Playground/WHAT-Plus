//! 各頁 controller 共用的狀態與小工具。

use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, Weak};
use whatplus_proto::{Destination, DiscordGuild, Request, Response, Schedule};

use crate::AppWindow;
use crate::backend::Backend;
use crate::settings::{ClientSettings, SettingsStore};
use crate::{StatusBar, i18n};

/// 多個頁面共用的伺服器資料。
#[derive(Default)]
pub struct Cache {
    pub destinations: Vec<Destination>,
    pub schedules: Vec<Schedule>,
    pub guilds: Vec<DiscordGuild>,
}

#[derive(Clone)]
pub struct Ctx {
    pub cache: Arc<Mutex<Cache>>,
    pub ui: Weak<AppWindow>,
    pub backend: Arc<Backend>,
    pub settings: Arc<Mutex<ClientSettings>>,
    pub store: Arc<SettingsStore>,
}

impl Ctx {
    pub fn settings(&self) -> ClientSettings {
        self.settings
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    pub fn update_settings(&self, f: impl FnOnce(&mut ClientSettings)) {
        let snapshot = {
            let mut s = self.settings.lock().unwrap_or_else(|p| p.into_inner());
            f(&mut s);
            s.clone()
        };
        if let Err(e) = self.store.save(&snapshot) {
            self.error(format!("{e:#}"));
        }
    }

    pub fn cache(&self) -> std::sync::MutexGuard<'_, Cache> {
        self.cache.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn zh(&self) -> bool {
        self.settings().language == "zh-TW"
    }

    /// Rust 端產生的文字：依介面語言選一個。
    pub fn t(&self, en: &str, zh: &str) -> String {
        i18n::pick(self.zh(), en, zh)
    }

    pub fn info(&self, msg: impl Into<String>) {
        self.set_message(msg.into(), false);
    }

    pub fn error(&self, msg: impl Into<String>) {
        self.set_message(msg.into(), true);
    }

    fn set_message(&self, msg: String, is_error: bool) {
        if let Some(ui) = self.ui.upgrade() {
            let bar = ui.global::<StatusBar>();
            bar.set_message(msg.into());
            bar.set_message_is_error(is_error);
        }
    }

    fn set_busy(&self, busy: bool) {
        if let Some(ui) = self.ui.upgrade() {
            ui.global::<StatusBar>().set_busy(busy);
        }
    }

    /// 送出請求；成功時在 UI 執行緒呼叫 `on_ok`，失敗時顯示錯誤。
    pub fn call(
        &self,
        req: Request,
        on_ok: impl FnOnce(&Ctx, &AppWindow, Response) + Send + 'static,
    ) {
        self.set_busy(true);
        let ctx = self.clone();
        self.backend
            .spawn(self.backend.request(req), move |result| {
                ctx.set_busy(false);
                let Some(ui) = ctx.ui.upgrade() else { return };
                match result {
                    Ok(resp) => on_ok(&ctx, &ui, resp),
                    Err(e) => ctx.error(e),
                }
            });
    }
}
