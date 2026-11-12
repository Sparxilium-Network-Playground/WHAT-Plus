//! 手動推送頁。

use slint::ComponentHandle;
use whatplus_proto::{Request, Response};

use crate::convert::{checked_ids, content_from_ui, destination_checks, language_from_index, range_from_index};
use crate::ctx::Ctx;
use crate::{AppWindow, PushData};

pub fn init(ctx: &Ctx, ui: &AppWindow) {
    let data = ui.global::<PushData>();
    {
        let ctx = ctx.clone();
        data.on_refresh(move || super::destinations::refresh(&ctx));
    }
    {
        let ctx = ctx.clone();
        data.on_preview_report(move || {
            let Some(ui) = ctx.ui.upgrade() else { return };
            let d = ui.global::<PushData>();
            let req = Request::PreviewReport {
                range: range_from_index(d.get_range_index()),
                content: content_from_ui(&d.get_content()),
                language: language_from_index(d.get_language_index()),
            };
            ctx.call(req, |_, ui, resp| {
                if let Response::Preview { text } = resp {
                    ui.global::<PushData>().set_preview(text.into());
                }
            });
        });
    }
    {
        let ctx = ctx.clone();
        data.on_send_report(move || {
            let Some(ui) = ctx.ui.upgrade() else { return };
            let d = ui.global::<PushData>();
            let ids = checked_ids(&d.get_destinations());
            if ids.is_empty() {
                ctx.error(ctx.t("Select at least one channel.", "請至少選一個頻道。"));
                return;
            }
            let req = Request::SendReport {
                range: range_from_index(d.get_range_index()),
                content: content_from_ui(&d.get_content()),
                destination_ids: ids,
            };
            ctx.call(req, |ctx, _, resp| {
                if let Response::Sent { results } = resp {
                    let failed = results.iter().any(|r| r.contains("FAILED"));
                    let text = results.join("\n");
                    if failed { ctx.error(text) } else { ctx.info(text) }
                }
            });
        });
    }
}

/// 目的地清單更新後重建勾選清單（保留原本的勾選）。
pub fn update_destinations(ctx: &Ctx, ui: &AppWindow) {
    let d = ui.global::<PushData>();
    let checked = checked_ids(&d.get_destinations());
    d.set_destinations(destination_checks(&ctx.cache().destinations, &checked));
}
