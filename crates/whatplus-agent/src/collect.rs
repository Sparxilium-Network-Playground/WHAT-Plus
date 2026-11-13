//! 讀取本機用量（透過 what-sources / ccusage）並計價。

use anyhow::Result;
use jiff::Timestamp;
use what_core::UsageRecord;
use what_pricing::PricingTable;

/// 每個來源讀取的結果摘要（給 TUI 顯示）。
#[derive(Debug, Clone)]
pub struct SourceSummary {
    pub name: String,
    pub dirs: Vec<String>,
    pub records: usize,
    pub error: Option<String>,
}

/// 讀取所有來源；單一來源失敗不影響其他來源。會做檔案 I/O，請在背景執行緒呼叫。
pub fn load_all() -> (Vec<UsageRecord>, Vec<SourceSummary>) {
    let failed = crate::snapshot::refresh();
    if failed > 0 {
        tracing::warn!(
            "could not copy {failed} Antigravity database file(s); they will be retried"
        );
    }
    let pricing = PricingTable::embedded();
    let mut records = Vec::new();
    let mut summaries = Vec::new();
    for source in what_sources::all_sources() {
        let dirs = source
            .data_dirs()
            .iter()
            .map(|d| d.display().to_string())
            .collect();
        let result: Result<Vec<UsageRecord>> = source.load().map(|loaded| loaded.price(&pricing));
        let summary = match result {
            Ok(mut list) => {
                let n = list.len();
                records.append(&mut list);
                SourceSummary {
                    name: source.source().display_name().into(),
                    dirs,
                    records: n,
                    error: None,
                }
            }
            Err(e) => SourceSummary {
                name: source.source().display_name().into(),
                dirs,
                records: 0,
                error: Some(format!("{e:#}")),
            },
        };
        summaries.push(summary);
    }
    records.sort_by_key(|r| r.timestamp);
    (records, summaries)
}

/// 只保留 `since` 之後的紀錄（`None` 表示全部）。
pub fn since(records: Vec<UsageRecord>, since: Option<Timestamp>) -> Vec<UsageRecord> {
    match since {
        Some(t) => records.into_iter().filter(|r| r.timestamp >= t).collect(),
        None => records,
    }
}
