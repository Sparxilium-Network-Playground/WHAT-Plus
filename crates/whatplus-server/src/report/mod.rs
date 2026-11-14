//! 報告：期間計算 → 讀取紀錄 → 彙總 → 依語言產生卡片（Discord embed）。

pub mod aggregate;
pub mod card;
pub mod format;
pub mod period;

use anyhow::Result;
use whatplus_proto::{Language, ReportContent};

use crate::db::Store;
use card::Card;
use period::Period;

/// 產生一份報告卡片。
pub fn build(
    store: &Store,
    period: &Period,
    timezone: &str,
    content: ReportContent,
    lang: Language,
) -> Result<Card> {
    let records = store.records_between(period.start.timestamp(), period.end.timestamp())?;
    let data = aggregate::aggregate(&records);
    Ok(format::render(&data, period, timezone, content, lang))
}
