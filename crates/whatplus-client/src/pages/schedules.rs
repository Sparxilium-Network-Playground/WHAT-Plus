//! 排程頁。

use slint::{ComponentHandle, ModelRc, VecModel};
use whatplus_proto::{ReportContent, Request, Response, Schedule};

use crate::convert::{
    checked_ids, content_from_ui, content_to_ui, destination_checks, schedule_fields,
    schedule_kind, schedule_summary,
};
use crate::ctx::Ctx;
use crate::{AppWindow, ScheduleData, ScheduleRow};

pub fn init(ctx: &Ctx, ui: &AppWindow) {
    clear_form(ctx, ui);
    let data = ui.global::<ScheduleData>();
    {
        let ctx = ctx.clone();
        data.on_refresh(move || refresh(&ctx));
    }
    {
        let ctx = ctx.clone();
        data.on_new_schedule(move || {
            if let Some(ui) = ctx.ui.upgrade() {
                clear_form(&ctx, &ui);
            }
        });
    }
    {
        let ctx = ctx.clone();
        data.on_select(move |id| {
            let Some(ui) = ctx.ui.upgrade() else { return };
            let found = ctx
                .cache()
                .schedules
                .iter()
                .find(|s| s.id == id as i64)
                .cloned();
            if let Some(s) = found {
                fill_form(&ctx, &ui, &s);
            }
        });
    }
    {
        let ctx = ctx.clone();
        data.on_save(move || save(&ctx));
    }
    {
        let ctx = ctx.clone();
        data.on_delete(move || {
            let Some(ui) = ctx.ui.upgrade() else { return };
            let d = ui.global::<ScheduleData>();
            let id = d.get_selected() as i64;
            if id <= 0 {
                return;
            }
            let question = format!(
                "{} \"{}\"?",
                ctx.t("Delete schedule", "刪除排程"),
                d.get_name()
            );
            if super::confirm(&ctx, &question) {
                ctx.call(Request::DeleteSchedule { id }, |ctx, ui, _| {
                    clear_form(ctx, ui);
                    refresh(ctx);
                });
            }
        });
    }
}

fn clear_form(ctx: &Ctx, ui: &AppWindow) {
    let d = ui.global::<ScheduleData>();
    d.set_selected(-1);
    d.set_name(ctx.t("Daily report", "每日報告").into());
    d.set_kind_index(0);
    d.set_weekday_index(0);
    d.set_hour(9);
    d.set_minute(0);
    d.set_enabled(true);
    d.set_content(content_to_ui(ReportContent::default()));
    d.set_destinations(destination_checks(&ctx.cache().destinations, &[]));
}

fn fill_form(ctx: &Ctx, ui: &AppWindow, s: &Schedule) {
    let d = ui.global::<ScheduleData>();
    let (kind, weekday, hour, minute) = schedule_fields(s.kind);
    d.set_selected(s.id as i32);
    d.set_name(s.name.clone().into());
    d.set_kind_index(kind);
    d.set_weekday_index(weekday);
    d.set_hour(hour);
    d.set_minute(minute);
    d.set_enabled(s.enabled);
    d.set_content(content_to_ui(s.content));
    d.set_destinations(destination_checks(
        &ctx.cache().destinations,
        &s.destination_ids,
    ));
}

fn save(ctx: &Ctx) {
    let Some(ui) = ctx.ui.upgrade() else { return };
    let d = ui.global::<ScheduleData>();
    let schedule = Schedule {
        id: d.get_selected().max(0) as i64,
        name: d.get_name().trim().to_string(),
        kind: schedule_kind(
            d.get_kind_index(),
            d.get_weekday_index(),
            d.get_hour(),
            d.get_minute(),
        ),
        destination_ids: checked_ids(&d.get_destinations()),
        content: content_from_ui(&d.get_content()),
        enabled: d.get_enabled(),
        last_run_at: None,
    };
    if schedule.destination_ids.is_empty() {
        ctx.error(ctx.t("Select at least one channel.", "請至少選一個頻道。"));
        return;
    }
    ctx.call(Request::SaveSchedule { schedule }, |ctx, ui, resp| {
        if let Response::Saved { id } = resp {
            ui.global::<ScheduleData>().set_selected(id as i32);
        }
        ctx.info(ctx.t("Schedule saved.", "排程已儲存。"));
        refresh(ctx);
    });
}

pub fn refresh(ctx: &Ctx) {
    ctx.call(Request::ListSchedules, |ctx, ui, resp| {
        let Response::Schedules { schedules } = resp else {
            return;
        };
        let zh = ctx.zh();
        let never = ctx.t("never run", "尚未執行");
        let rows: Vec<ScheduleRow> = schedules
            .iter()
            .map(|s| ScheduleRow {
                id: s.id as i32,
                name: s.name.clone().into(),
                summary: schedule_summary(s.kind, zh).into(),
                last_run: s
                    .last_run_at
                    .clone()
                    .unwrap_or_else(|| never.clone())
                    .into(),
                enabled: s.enabled,
            })
            .collect();
        ui.global::<ScheduleData>()
            .set_rows(ModelRc::new(VecModel::from(rows)));
        ctx.cache().schedules = schedules;
    });
}

/// 目的地清單更新後重建表單中的勾選清單（保留原本的勾選）。
pub fn update_destinations(ctx: &Ctx, ui: &AppWindow) {
    let d = ui.global::<ScheduleData>();
    let checked = checked_ids(&d.get_destinations());
    d.set_destinations(destination_checks(&ctx.cache().destinations, &checked));
}
