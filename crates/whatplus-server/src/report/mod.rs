//! 報告：期間計算 → 讀取紀錄 → 彙總 → 依語言產生文字。

pub mod aggregate;
pub mod format;
pub mod period;

use anyhow::Result;
use whatplus_proto::{Language, ReportContent};

use crate::db::Store;
use period::Period;

/// 產生一份報告的文字（尚未分割成 Discord 訊息）。
pub fn build(
    store: &Store,
    period: &Period,
    timezone: &str,
    content: ReportContent,
    lang: Language,
) -> Result<String> {
    let records = store.records_between(period.start.timestamp(), period.end.timestamp())?;
    let data = aggregate::aggregate(&records);
    Ok(format::render(&data, period, timezone, content, lang))
}
