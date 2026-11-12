//! 偏好設定頁：介面語言與主題，存在 client.json。

use slint::ComponentHandle;

use crate::ctx::Ctx;
use crate::{AppWindow, Prefs, i18n};

pub fn init(ctx: &Ctx, ui: &AppWindow) {
    let s = ctx.settings();
    let prefs = ui.global::<Prefs>();
    prefs.set_language_index(if s.language == "zh-TW" { 0 } else { 1 });
    prefs.set_theme_index(s.theme.clamp(0, 2));
    prefs.set_settings_path(ctx.store.path().display().to_string().into());
    ui.set_theme_mode(s.theme.clamp(0, 2));
    i18n::apply(&s.language);

    let ctx = ctx.clone();
    prefs.on_changed(move || {
        let Some(ui) = ctx.ui.upgrade() else { return };
        let p = ui.global::<Prefs>();
        let language = if p.get_language_index() == 0 { "zh-TW" } else { "en" };
        let theme = p.get_theme_index();
        ctx.update_settings(|s| {
            s.language = language.to_string();
            s.theme = theme;
        });
        i18n::apply(language);
        ui.set_theme_mode(theme);
        if ui.global::<crate::StatusBar>().get_connected() {
            // 重新產生 Rust 端的文字（例如排程摘要）。
            super::refresh_all(&ctx);
        }
    });
}
