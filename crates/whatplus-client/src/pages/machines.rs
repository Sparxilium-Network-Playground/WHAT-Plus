//! 來源機器頁：列出公鑰、新增／停用／刪除 agent 公鑰（admin 公鑰只能在伺服器 TUI 管理）。

use slint::{ComponentHandle, ModelRc, VecModel};
use whatplus_proto::{AgentKeyInput, KeyInfo, Request, Response, Role};

use crate::ctx::Ctx;
use crate::{AppWindow, MachineData, MachineRow};

pub fn init(ctx: &Ctx, ui: &AppWindow) {
    let data = ui.global::<MachineData>();
    {
        let ctx = ctx.clone();
        data.on_refresh(move || refresh(&ctx));
    }
    {
        let ctx = ctx.clone();
        data.on_add(move || {
            let Some(ui) = ctx.ui.upgrade() else { return };
            let d = ui.global::<MachineData>();
            let key = AgentKeyInput {
                name: d.get_name().trim().to_string(),
                public_key: d.get_public_key().trim().to_string(),
                allowed_cidrs: d
                    .get_cidrs()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
            };
            ctx.call(Request::AddAgentKey { key }, |ctx, ui, _| {
                let d = ui.global::<MachineData>();
                d.set_name("".into());
                d.set_public_key("".into());
                d.set_cidrs("".into());
                ctx.info(ctx.t(
                    "Machine added. On that machine, run \"Verify server and test login\" in the agent TUI.",
                    "已新增機器。請在那台機器的 agent TUI 執行「Verify server and test login」。",
                ));
                refresh(ctx);
            });
        });
    }
    {
        let ctx = ctx.clone();
        data.on_toggle(move |id| {
            let Some(ui) = ctx.ui.upgrade() else { return };
            let enabled = row_enabled(&ui, id);
            ctx.call(
                Request::SetAgentKeyEnabled { id: id as i64, enabled: !enabled },
                |ctx, _, _| refresh(ctx),
            );
        });
    }
    {
        let ctx = ctx.clone();
        data.on_delete(move |id| {
            let question = ctx.t(
                "Delete this machine? Its uploaded usage records are deleted too. This cannot be undone.",
                "刪除這台機器？它上傳的用量紀錄也會一併刪除，且無法復原。",
            );
            if super::confirm(&ctx, &question) {
                ctx.call(Request::DeleteAgentKey { id: id as i64 }, |ctx, _, _| refresh(ctx));
            }
        });
    }
}

fn row_enabled(ui: &AppWindow, id: i32) -> bool {
    use slint::Model;
    ui.global::<MachineData>()
        .get_rows()
        .iter()
        .find(|r| r.id == id)
        .is_some_and(|r| r.enabled)
}

fn row(ctx: &Ctx, k: &KeyInfo) -> MachineRow {
    let records = format!("{} {}", k.record_count, ctx.t("records", "筆紀錄"));
    let last_seen = match (&k.last_seen_at, &k.last_seen_ip) {
        (Some(t), Some(ip)) => format!("{} {t} ({ip})", ctx.t("last seen", "最後連線")),
        _ => ctx.t("never connected", "從未連線"),
    };
    let cidrs = if k.allowed_cidrs.is_empty() {
        ctx.t("any allowed IP", "白名單內任何 IP")
    } else {
        k.allowed_cidrs.join(", ")
    };
    MachineRow {
        id: k.id as i32,
        name: k.name.clone().into(),
        role: k.role.as_str().into(),
        fingerprint: k.fingerprint.clone().into(),
        records: records.into(),
        last_seen: last_seen.into(),
        cidrs: cidrs.into(),
        enabled: k.enabled,
        editable: k.role == Role::Agent,
    }
}

pub fn refresh(ctx: &Ctx) {
    ctx.call(Request::ListKeys, |ctx, ui, resp| {
        let Response::Keys { keys } = resp else { return };
        let rows: Vec<MachineRow> = keys.iter().map(|k| row(ctx, k)).collect();
        ui.global::<MachineData>().set_rows(ModelRc::new(VecModel::from(rows)));
    });
}
