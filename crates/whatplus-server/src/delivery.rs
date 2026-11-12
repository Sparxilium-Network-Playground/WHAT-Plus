//! 產生報告並送到多個目的地（手動推送與排程共用）。

use anyhow::Result;
use std::collections::HashMap;
use whatplus_proto::{Destination, Language, ReportContent};

use crate::app::App;
use crate::db::runtime;
use crate::report::{self, period::Period};

/// 每種語言只產生一次文字；停用的目的地略過。回傳每個目的地的結果。
pub async fn send_report(
    app: &App,
    period: &Period,
    content: ReportContent,
    destinations: &[Destination],
) -> Result<Vec<String>> {
    let mut texts: HashMap<Language, String> = HashMap::new();
    let mut results = Vec::new();
    for dest in destinations {
        if !dest.enabled {
            results.push(format!("{}: skipped (disabled)", dest.name));
            continue;
        }
        let text = match texts.get(&dest.language) {
            Some(t) => t.clone(),
            None => {
                let t = report::build(
                    &app.store,
                    period,
                    &app.settings.timezone_name,
                    content,
                    dest.language,
                )?;
                texts.insert(dest.language, t.clone());
                t
            }
        };
        results.push(app.discord.deliver(dest, &text).await);
    }
    let summary = results.join("; ");
    let _ = app
        .store
        .runtime_set(runtime::LAST_REPORT_AT, &crate::db::now_rfc3339());
    let _ = app.store.runtime_set(runtime::LAST_REPORT_RESULT, &summary);
    Ok(results)
}

/// 依 id 取出目的地；不存在的 id 回錯誤。
pub fn pick_destinations(app: &App, ids: &[i64]) -> Result<Vec<Destination>, String> {
    let all = app.store.list_destinations().map_err(|e| e.to_string())?;
    ids.iter()
        .map(|id| {
            all.iter()
                .find(|d| d.id == *id)
                .cloned()
                .ok_or_else(|| format!("destination {id} does not exist"))
        })
        .collect()
}
