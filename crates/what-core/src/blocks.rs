//! 5 小時計費區塊、燃燒速率與預估用量。
//!
//! 演算法移植自 ccusage `crates/ccusage/src/blocks.rs`
//! （Copyright (c) 2025 ryoppippi，MIT License，見 THIRD-PARTY-NOTICES.md）。
//! 規則：區塊起點為第一筆紀錄向下取整到整點；與起點或上一筆相距超過區塊長度時開新區塊，
//! 與上一筆相距超過區塊長度時另外插入一個閒置（gap）區塊。

use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};

use crate::model::{Source, UsageRecord, UsageTotals};

/// 預設區塊長度（小時），與 Claude 訂閱的 5 小時重置窗口一致。
pub const DEFAULT_BLOCK_HOURS: f64 = 5.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionBlock {
    pub start: Timestamp,
    pub end: Timestamp,
    /// 區塊內最後一筆紀錄時間；閒置區塊為 `None`。
    pub last_activity: Option<Timestamp>,
    pub is_active: bool,
    pub is_gap: bool,
    pub first_activity: Option<Timestamp>,
    pub totals: UsageTotals,
    pub models: Vec<String>,
    pub sources: Vec<Source>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BurnRate {
    /// 含快取的 token／分鐘。
    pub tokens_per_minute: f64,
    /// 不含快取（輸入＋輸出）的 token／分鐘，用於速率指示燈。
    pub non_cache_tokens_per_minute: f64,
    pub cost_per_hour: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Projection {
    pub total_tokens: u64,
    pub total_cost: f64,
    pub remaining_minutes: u64,
}

fn block_duration(hours: f64) -> SignedDuration {
    SignedDuration::from_millis((hours.max(0.1) * 3_600_000.0) as i64)
}

fn floor_to_hour(ts: Timestamp) -> Timestamp {
    let secs = ts.as_second();
    Timestamp::from_second(secs - secs.rem_euclid(3600)).unwrap_or(ts)
}

/// 把紀錄切成計費區塊，依時間由舊到新排序。`now` 用於判斷區塊是否仍在進行中。
pub fn identify_blocks<'a>(
    records: impl IntoIterator<Item = &'a UsageRecord>,
    block_hours: f64,
    now: Timestamp,
) -> Vec<SessionBlock> {
    let mut records: Vec<&UsageRecord> = records.into_iter().collect();
    if records.is_empty() {
        return Vec::new();
    }
    records.sort_by_key(|r| r.timestamp);
    let duration = block_duration(block_hours);

    let mut blocks = Vec::new();
    let mut current_start: Option<Timestamp> = None;
    let mut current: Vec<&UsageRecord> = Vec::new();

    for record in records {
        if let Some(start) = current_start {
            let last = current.last().map(|r| r.timestamp).unwrap_or(start);
            let since_start = record.timestamp.duration_since(start);
            let since_last = record.timestamp.duration_since(last);
            if since_start > duration || since_last > duration {
                blocks.push(create_block(start, &current, now, duration));
                current.clear();
                if since_last > duration {
                    blocks.push(create_gap_block(last, record.timestamp, duration));
                }
                current_start = Some(floor_to_hour(record.timestamp));
            }
        } else {
            current_start = Some(floor_to_hour(record.timestamp));
        }
        current.push(record);
    }
    if let Some(start) = current_start
        && !current.is_empty()
    {
        blocks.push(create_block(start, &current, now, duration));
    }
    blocks
}

fn create_block(
    start: Timestamp,
    records: &[&UsageRecord],
    now: Timestamp,
    duration: SignedDuration,
) -> SessionBlock {
    let end = start.checked_add(duration).unwrap_or(start);
    let last = records.last().map(|r| r.timestamp);
    let first = records.first().map(|r| r.timestamp);
    let is_active = last.is_some_and(|last| now.duration_since(last) < duration && now < end);
    let mut totals = UsageTotals::default();
    let mut models: Vec<String> = Vec::new();
    let mut sources: Vec<Source> = Vec::new();
    for record in records {
        totals.add_record(record);
        if !record.model.is_empty() && !models.contains(&record.model) {
            models.push(record.model.clone());
        }
        if !sources.contains(&record.source) {
            sources.push(record.source);
        }
    }
    SessionBlock {
        start,
        end,
        last_activity: last,
        first_activity: first,
        is_active,
        is_gap: false,
        totals,
        models,
        sources,
    }
}

fn create_gap_block(last: Timestamp, next: Timestamp, duration: SignedDuration) -> SessionBlock {
    SessionBlock {
        start: last.checked_add(duration).unwrap_or(last),
        end: next,
        last_activity: None,
        first_activity: None,
        is_active: false,
        is_gap: true,
        totals: UsageTotals::default(),
        models: Vec::new(),
        sources: Vec::new(),
    }
}

/// 以區塊內第一筆到最後一筆的時間計算速率；時間差為 0 時無法計算。
pub fn burn_rate(block: &SessionBlock) -> Option<BurnRate> {
    if block.is_gap {
        return None;
    }
    let (first, last) = (block.first_activity?, block.last_activity?);
    let minutes = last.duration_since(first).as_secs_f64() / 60.0;
    if minutes <= 0.0 {
        return None;
    }
    Some(BurnRate {
        tokens_per_minute: block.totals.tokens.total() as f64 / minutes,
        non_cache_tokens_per_minute: block.totals.tokens.non_cache() as f64 / minutes,
        cost_per_hour: block.totals.cost_usd / minutes * 60.0,
    })
}

/// 依目前速率預估區塊結束時的用量；只對進行中的區塊有意義。
pub fn project_block(block: &SessionBlock, now: Timestamp) -> Option<Projection> {
    if !block.is_active || block.is_gap {
        return None;
    }
    let burn = burn_rate(block)?;
    let remaining = (block.end.duration_since(now).as_secs_f64() / 60.0)
        .round()
        .max(0.0);
    let total_tokens = block.totals.tokens.total() as f64 + burn.tokens_per_minute * remaining;
    let total_cost = block.totals.cost_usd + burn.cost_per_hour / 60.0 * remaining;
    Some(Projection {
        total_tokens: total_tokens.round() as u64,
        total_cost,
        remaining_minutes: remaining as u64,
    })
}

/// 歷史區塊中的最大 token 數（不含進行中的區塊），可作為預設上限參考。
pub fn max_block_tokens(blocks: &[SessionBlock]) -> u64 {
    blocks
        .iter()
        .filter(|b| !b.is_gap && !b.is_active)
        .map(|b| b.totals.tokens.total())
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::record;

    fn ts(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    #[test]
    fn splits_blocks_and_inserts_gaps() {
        let records = vec![
            record(Source::ClaudeCode, "2026-11-01T10:30:00Z", "m", 1.0),
            record(Source::ClaudeCode, "2026-11-01T12:00:00Z", "m", 1.0),
            // 超過起點（10:00）5 小時 → 新區塊，但距上一筆未超過 5 小時 → 無 gap
            record(Source::ClaudeCode, "2026-11-01T15:10:00Z", "m", 1.0),
            // 距上一筆超過 5 小時 → 新區塊＋gap
            record(Source::ClaudeCode, "2026-11-02T01:00:00Z", "m", 1.0),
        ];
        let blocks = identify_blocks(&records, 5.0, ts("2026-11-10T00:00:00Z"));
        let kinds: Vec<_> = blocks.iter().map(|b| b.is_gap).collect();
        assert_eq!(kinds, vec![false, false, true, false]);
        assert_eq!(blocks[0].start, ts("2026-11-01T10:00:00Z"));
        assert_eq!(blocks[0].end, ts("2026-11-01T15:00:00Z"));
        assert_eq!(blocks[0].totals.requests, 2);
        assert_eq!(blocks[1].start, ts("2026-11-01T15:00:00Z"));
        assert_eq!(blocks[2].start, ts("2026-11-01T20:10:00Z"));
        assert!(blocks.iter().all(|b| !b.is_active));
    }

    #[test]
    fn active_block_projection() {
        let mut a = record(Source::ClaudeCode, "2026-11-01T10:00:00Z", "m", 1.0);
        a.tokens = crate::TokenCounts {
            input: 600,
            ..Default::default()
        };
        let mut b = record(Source::ClaudeCode, "2026-11-01T11:00:00Z", "m", 1.0);
        b.tokens = crate::TokenCounts {
            input: 600,
            ..Default::default()
        };
        let now = ts("2026-11-01T12:00:00Z");
        let blocks = identify_blocks(&[a, b], 5.0, now);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].is_active);
        let burn = burn_rate(&blocks[0]).unwrap();
        assert!((burn.tokens_per_minute - 20.0).abs() < 1e-9);
        assert!((burn.cost_per_hour - 2.0).abs() < 1e-9);
        let projection = project_block(&blocks[0], now).unwrap();
        assert_eq!(projection.remaining_minutes, 180);
        assert_eq!(projection.total_tokens, 1200 + 20 * 180);
        assert!((projection.total_cost - 8.0).abs() < 1e-9);
    }
}
