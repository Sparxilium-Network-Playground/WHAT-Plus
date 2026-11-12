//! 伺服器狀態頁。

use slint::{ComponentHandle, ModelRc, VecModel};
use whatplus_proto::{Request, Response};

use crate::ctx::Ctx;
use crate::{AppWindow, KeyValueRow, StatusData};

pub fn init(ctx: &Ctx, ui: &AppWindow) {
    let ctx = ctx.clone();
    ui.global::<StatusData>().on_refresh(move || refresh(&ctx));
}

pub fn refresh(ctx: &Ctx) {
    ctx.call(Request::Status, |ctx, ui, resp| {
        let Response::Status { status: s } = resp else { return };
        let dash = |v: Option<String>| v.unwrap_or_else(|| "-".into());
        let rows = [
            (ctx.t("Server version", "伺服器版本"), s.server_version),
            (ctx.t("Time zone", "時區"), s.timezone),
            (ctx.t("Host fingerprint", "伺服器指紋"), s.host_fingerprint),
            ("Discord".to_string(), s.discord),
            (ctx.t("Machines", "來源機器"), s.agents.to_string()),
            (ctx.t("Usage records", "用量紀錄"), s.records.to_string()),
            (ctx.t("IP allowlist", "IP 白名單"), s.allowed_cidrs.join(", ")),
            (ctx.t("Last report", "最近一次報告"), dash(s.last_report_at)),
            (ctx.t("Last result", "最近結果"), dash(s.last_report_result)),
        ];
        let rows: Vec<KeyValueRow> = rows
            .into_iter()
            .map(|(k, v)| KeyValueRow { key: k.into(), value: v.into() })
            .collect();
        ui.global::<StatusData>().set_rows(ModelRc::new(VecModel::from(rows)));
    });
}
