//! WHAT 的純邏輯層：不做檔案 I/O、不連網、不依賴任何資料來源實作。

pub mod blocks;
pub mod filter;
pub mod model;
pub mod report;

pub use filter::RecordFilter;
pub use model::{Source, TokenCounts, UsageRecord, UsageTotals};
pub use report::{Granularity, GroupSummary, ModelUsage, PeriodSummary};

/// 測試用的紀錄建構函式，供本 crate 與下游 crate 的測試使用。
#[doc(hidden)]
pub mod test_support {
    use crate::model::{Source, TokenCounts, UsageRecord};

    pub fn record(source: Source, timestamp: &str, model: &str, cost: f64) -> UsageRecord {
        UsageRecord {
            source,
            timestamp: timestamp.parse().expect("valid RFC 3339 timestamp"),
            model: model.to_string(),
            project: "project".to_string(),
            session_id: "session".to_string(),
            tokens: TokenCounts {
                input: 10,
                output: 20,
                ..TokenCounts::default()
            },
            cost_usd: cost,
            pricing_missing: false,
        }
    }
}
