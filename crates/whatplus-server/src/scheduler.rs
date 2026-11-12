//! 報告排程：每 30 秒檢查一次，各排程在觸發時間後送出一次。
//!
//! - 排程建立（或修改）之前的觸發時間不補送。
//! - 伺服器停機太久（超過 [`CATCH_UP`]）錯過的報告不補送，只記錄 `missed`。
//! - 沒有 admin 公鑰時不執行。

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use jiff::{SignedDuration, Timestamp};
use whatplus_proto::Schedule;

use crate::app::App;
use crate::delivery::{pick_destinations, send_report};
use crate::report::period::{latest_due, scheduled_period};

const TICK: Duration = Duration::from_secs(30);
const CATCH_UP: SignedDuration = SignedDuration::from_hours(6);

pub async fn run(app: Arc<App>) {
    let mut ticker = tokio::time::interval(TICK);
    loop {
        ticker.tick().await;
        if !app.auth.current().has_admin() {
            continue;
        }
        if let Err(e) = tick(&app).await {
            tracing::warn!("scheduler: {e:#}");
        }
    }
}

/// 排程目前該做什麼。
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// 這次觸發已處理過。
    Done,
    /// 觸發時間早於排程建立／修改時間：記錄基準，不發送。
    Baseline,
    /// 錯過太久：記錄，不發送。
    Missed,
    Send,
}

pub fn decide(due: Timestamp, now: Timestamp, last_due: Option<Timestamp>, baseline: Option<Timestamp>) -> Decision {
    if last_due.is_some_and(|last| last >= due) {
        return Decision::Done;
    }
    if last_due.is_none() && baseline.is_some_and(|b| b > due) {
        return Decision::Baseline;
    }
    if now.duration_since(due) > CATCH_UP {
        return Decision::Missed;
    }
    Decision::Send
}

async fn tick(app: &App) -> Result<()> {
    let now = app.now();
    for schedule in app.store.list_schedules()?.into_iter().filter(|s| s.enabled) {
        if let Err(e) = run_one(app, &schedule, &now).await {
            tracing::warn!(schedule = schedule.id, "schedule failed: {e:#}");
        }
    }
    Ok(())
}

async fn run_one(app: &App, schedule: &Schedule, now: &jiff::Zoned) -> Result<()> {
    let store = &app.store;
    let due = latest_due(schedule.kind, now)?;
    let due_ts = due.timestamp();
    let last_due = store
        .schedule_run(schedule.id)?
        .and_then(|r| r.last_due_at.parse::<Timestamp>().ok());
    let baseline = store
        .schedule_created_at(schedule.id)?
        .and_then(|c| c.parse::<Timestamp>().ok());

    match decide(due_ts, now.timestamp(), last_due, baseline) {
        Decision::Done => Ok(()),
        Decision::Baseline => store.set_schedule_run(schedule.id, &due_ts.to_string(), "waiting for next run"),
        Decision::Missed => {
            tracing::warn!(schedule = schedule.id, "missed run at {due}; not sending a late report");
            store.set_schedule_run(schedule.id, &due_ts.to_string(), "missed (server was offline)")
        }
        Decision::Send => {
            let period = scheduled_period(schedule.kind, &due)?;
            let result = match pick_destinations(app, &schedule.destination_ids) {
                Ok(dests) => send_report(app, &period, schedule.content, &dests).await?.join("; "),
                Err(e) => format!("FAILED {e}"),
            };
            tracing::info!(schedule = schedule.id, "report sent: {result}");
            store.set_schedule_run(schedule.id, &due_ts.to_string(), &result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    #[test]
    fn decisions() {
        let due = t("2026-11-13T01:00:00Z");
        let now = t("2026-11-13T01:00:30Z");
        assert_eq!(decide(due, now, None, Some(t("2026-11-01T00:00:00Z"))), Decision::Send);
        assert_eq!(decide(due, now, Some(due), None), Decision::Done);
        assert_eq!(decide(due, now, None, Some(t("2026-11-13T02:00:00Z"))), Decision::Baseline);
        assert_eq!(
            decide(due, t("2026-11-13T09:00:00Z"), Some(t("2026-11-12T01:00:00Z")), None),
            Decision::Missed
        );
    }
}
