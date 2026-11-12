//! 每頁一個 controller：設定 Slint callback、把伺服器回應轉成畫面資料。

mod connection;
mod destinations;
mod machines;
mod prefs;
mod push;
mod schedules;
mod status;

use crate::AppWindow;
use crate::ctx::Ctx;

pub fn init(ctx: &Ctx, ui: &AppWindow) {
    prefs::init(ctx, ui);
    connection::init(ctx, ui);
    status::init(ctx, ui);
    push::init(ctx, ui);
    schedules::init(ctx, ui);
    destinations::init(ctx, ui);
    machines::init(ctx, ui);
}

/// 連線後載入所有頁面的資料。
pub fn refresh_all(ctx: &Ctx) {
    status::refresh(ctx);
    destinations::refresh(ctx);
    schedules::refresh(ctx);
    machines::refresh(ctx);
}

/// 刪除前的確認對話框。
pub fn confirm(_ctx: &Ctx, question: &str) -> bool {
    rfd::MessageDialog::new()
        .set_title("WHAT Plus")
        .set_description(question)
        .set_buttons(rfd::MessageButtons::YesNo)
        .set_level(rfd::MessageLevel::Warning)
        .show()
        == rfd::MessageDialogResult::Yes
}
