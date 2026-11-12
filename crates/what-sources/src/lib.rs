//! 用量資料來源的抽象層。
//!
//! 上層（service / UI）只透過 [`UsageSource`] 與 [`LoadedUsage`] 取得 [`UsageRecord`]，
//! 不直接接觸 ccusage。目前的實作放在 [`ccusage_backend`]，委託 ccusage 的 adapter
//! 處理檔案探索、JSONL 解析與去重；之後若改為自行移植，只需替換這個模組。

use std::path::PathBuf;

use anyhow::Result;
use what_core::{Source, UsageRecord};
use what_pricing::PricingTable;

mod ccusage_backend;

pub use ccusage_backend::{AntigravitySource, ClaudeCodeSource, CodexSource};

/// 一種用量資料來源（例如 Claude Code）。
pub trait UsageSource: Send + Sync {
    fn source(&self) -> Source;

    /// 偵測到的資料目錄；供設定頁顯示與檔案監看使用。不存在時回傳空陣列。
    fn data_dirs(&self) -> Vec<PathBuf>;

    /// 讀取並去重原始資料。會做檔案 I/O，應在背景執行緒呼叫。
    fn load(&self) -> Result<Box<dyn LoadedUsage>>;
}

/// 已讀入記憶體的原始用量，可用不同價格表反覆計價而不必重讀檔案。
pub trait LoadedUsage: Send + Sync {
    fn price(&self, pricing: &PricingTable) -> Vec<UsageRecord>;

    /// 原始紀錄數（去重後、計價前）。
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 回傳所有內建來源。
pub fn all_sources() -> Vec<Box<dyn UsageSource>> {
    vec![
        Box::new(ClaudeCodeSource),
        Box::new(CodexSource),
        Box::new(AntigravitySource),
    ]
}
