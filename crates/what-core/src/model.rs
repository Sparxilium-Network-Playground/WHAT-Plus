//! 與資料來源無關的用量資料模型。
//!
//! 所有來源（Claude Code、Codex、Antigravity……）都先轉成 [`UsageRecord`]，
//! 之後的彙總、圖表、匯出只依賴這裡的型別。

use std::fmt;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

/// 用量來源工具。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    ClaudeCode,
    Codex,
    Antigravity,
}

impl Source {
    pub const ALL: [Source; 3] = [Source::ClaudeCode, Source::Codex, Source::Antigravity];

    /// 在 [`Source::ALL`] 中的位置；圖表系列與顏色依此排序。
    pub fn index(self) -> usize {
        self as usize
    }

    /// 穩定的識別字串，用於設定檔與匯出。
    pub fn id(self) -> &'static str {
        match self {
            Source::ClaudeCode => "claude-code",
            Source::Codex => "codex",
            Source::Antigravity => "antigravity",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Source::ClaudeCode => "Claude Code",
            Source::Codex => "Codex",
            Source::Antigravity => "Antigravity",
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

/// Token 計數。`input` 為未命中快取的輸入；`reasoning` 已包含在 `output` 內，只供顯示。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenCounts {
    pub input: u64,
    pub output: u64,
    pub cache_creation: u64,
    pub cache_read: u64,
    pub reasoning: u64,
}

impl TokenCounts {
    /// 計費相關的總 token 數（不重複計算 reasoning）。
    pub fn total(&self) -> u64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.cache_creation)
            .saturating_add(self.cache_read)
    }

    /// 不含快取的 token 數（輸入＋輸出），用於燃燒速率指標。
    pub fn non_cache(&self) -> u64 {
        self.input.saturating_add(self.output)
    }

    pub fn add(&mut self, other: &TokenCounts) {
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.cache_creation = self.cache_creation.saturating_add(other.cache_creation);
        self.cache_read = self.cache_read.saturating_add(other.cache_read);
        self.reasoning = self.reasoning.saturating_add(other.reasoning);
    }
}

impl std::ops::AddAssign<&TokenCounts> for TokenCounts {
    fn add_assign(&mut self, rhs: &TokenCounts) {
        self.add(rhs);
    }
}

/// 單筆 API 回應的用量（已去重、已計價）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageRecord {
    pub source: Source,
    pub timestamp: Timestamp,
    /// 正規化後的模型名稱；未知時為空字串。
    pub model: String,
    /// 給人看的專案名稱。
    pub project: String,
    pub session_id: String,
    pub tokens: TokenCounts,
    pub cost_usd: f64,
    /// 價格表找不到此模型，`cost_usd` 可能低估。
    pub pricing_missing: bool,
}

/// 用量加總。
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct UsageTotals {
    pub tokens: TokenCounts,
    pub cost_usd: f64,
    pub requests: u64,
}

impl UsageTotals {
    pub fn add_record(&mut self, record: &UsageRecord) {
        self.tokens += &record.tokens;
        self.cost_usd += record.cost_usd;
        self.requests += 1;
    }

    pub fn merge(&mut self, other: &UsageTotals) {
        self.tokens += &other.tokens;
        self.cost_usd += other.cost_usd;
        self.requests += other.requests;
    }

    pub fn from_records<'a>(records: impl IntoIterator<Item = &'a UsageRecord>) -> Self {
        let mut totals = Self::default();
        for record in records {
            totals.add_record(record);
        }
        totals
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_excludes_reasoning() {
        let tokens = TokenCounts {
            input: 1,
            output: 10,
            cache_creation: 100,
            cache_read: 1000,
            reasoning: 5,
        };
        assert_eq!(tokens.total(), 1111);
        assert_eq!(tokens.non_cache(), 11);
    }
}
