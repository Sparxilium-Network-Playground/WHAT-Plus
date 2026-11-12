//! 管理客戶端：狀態、Discord 目的地、排程、手動推送。

use whatplus_proto::{
    Destination, Language, ManualRange, ReportContent, Response, Schedule, ServerStatus,
};

use super::{ApiError, ApiResult};
use crate::app::App;
use crate::db::runtime;
use crate::delivery::{pick_destinations, send_report};
use crate::report::{self, period::manual_period};
use crate::validate;

pub fn status(app: &App) -> ApiResult {
    let store = &app.store;
    let agents = store
        .list_keys()?
        .iter()
        .filter(|k| k.role == whatplus_proto::Role::Agent)
        .count() as u64;
    Ok(Response::Status {
        status: ServerStatus {
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            timezone: app.settings.timezone_name.clone(),
            host_fingerprint: app.host_fingerprint.clone(),
            discord: store
                .runtime_get(runtime::DISCORD_STATUS)?
                .unwrap_or_else(|| "unknown".into()),
            agents,
            records: store.record_count()?,
            allowed_cidrs: store.allowed_networks()?,
            last_report_at: store.runtime_get(runtime::LAST_REPORT_AT)?,
            last_report_result: store.runtime_get(runtime::LAST_REPORT_RESULT)?,
        },
    })
}

pub async fn guilds(app: &App) -> ApiResult {
    let guilds = app
        .discord
        .guilds()
        .await
        .map_err(|e| ApiError::User(format!("{e:#}")))?;
    Ok(Response::Guilds { guilds })
}

pub fn destinations(app: &App) -> ApiResult {
    Ok(Response::Destinations {
        destinations: app.store.list_destinations()?,
    })
}

pub fn save_destination(app: &App, d: &Destination) -> ApiResult {
    let d = validate::destination(d)?;
    let id = app.store.save_destination(&d)?;
    Ok(Response::Saved { id })
}

pub fn delete_destination(app: &App, id: i64) -> ApiResult {
    let in_use = app
        .store
        .list_schedules()?
        .into_iter()
        .find(|s| s.destination_ids.contains(&id));
    if let Some(s) = in_use {
        return Err(ApiError::User(format!(
            "destination is used by schedule \"{}\"; remove it there first",
            s.name
        )));
    }
    app.store.delete_destination(id)?;
    Ok(Response::Ok)
}

pub async fn test_destination(app: &App, id: i64) -> ApiResult {
    let dest = app
        .store
        .destination(id)?
        .ok_or_else(|| ApiError::User("destination not found".into()))?;
    let text = match dest.language {
        Language::En => "**WHAT Plus** test message. Reports will be delivered to this channel.",
        Language::ZhTw => "**WHAT Plus** 測試訊息。報告會發送到這個頻道。",
    };
    let result = app.discord.deliver(&dest, text).await;
    Ok(Response::Sent {
        results: vec![result],
    })
}

pub fn schedules(app: &App) -> ApiResult {
    Ok(Response::Schedules {
        schedules: app.store.list_schedules()?,
    })
}

pub fn save_schedule(app: &App, s: &Schedule) -> ApiResult {
    let known: Vec<i64> = app.store.list_destinations()?.iter().map(|d| d.id).collect();
    let s = validate::schedule(s, &known)?;
    let id = app.store.save_schedule(&s)?;
    Ok(Response::Saved { id })
}

pub fn delete_schedule(app: &App, id: i64) -> ApiResult {
    app.store.delete_schedule(id)?;
    Ok(Response::Ok)
}

pub fn preview(app: &App, range: ManualRange, content: ReportContent, lang: Language) -> ApiResult {
    let period = manual_period(range, &app.now())?;
    let text = report::build(&app.store, &period, &app.settings.timezone_name, content, lang)?;
    Ok(Response::Preview { text })
}

pub async fn send(app: &App, range: ManualRange, content: ReportContent, ids: &[i64]) -> ApiResult {
    if ids.is_empty() {
        return Err(ApiError::User("select at least one destination".into()));
    }
    let destinations = pick_destinations(app, ids)?;
    let period = manual_period(range, &app.now())?;
    let results = send_report(app, &period, content, &destinations).await?;
    Ok(Response::Sent { results })
}
