//! 期間（日／週／月）、Session、專案、模型彙總。

use std::collections::{BTreeMap, BTreeSet, HashMap};

use jiff::{
    Timestamp,
    civil::{Date, Weekday},
    tz::TimeZone,
};
use serde::{Deserialize, Serialize};

use crate::model::{Source, UsageRecord, UsageTotals};

/// 期間粒度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Granularity {
    Daily,
    Weekly,
    Monthly,
}

/// 依模型（與來源）拆分的用量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelUsage {
    pub source: Source,
    pub model: String,
    pub totals: UsageTotals,
}

/// 一個期間的彙總。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeriodSummary {
    /// 期間第一天（週報為週起始日、月報為 1 號）。
    pub start: Date,
    pub totals: UsageTotals,
    /// 依成本由高到低排序。
    pub models: Vec<ModelUsage>,
    pub pricing_missing: bool,
}

/// 把日期對齊到所屬期間的第一天。
pub fn period_start(date: Date, granularity: Granularity, week_start: Weekday) -> Date {
    match granularity {
        Granularity::Daily => date,
        Granularity::Weekly => {
            let offset = date.weekday().since(week_start);
            date.checked_sub(jiff::Span::new().days(offset))
                .unwrap_or(date)
        }
        Granularity::Monthly => date.first_of_month(),
    }
}

/// 下一個期間的第一天。
pub fn next_period(start: Date, granularity: Granularity) -> Option<Date> {
    let span = match granularity {
        Granularity::Daily => jiff::Span::new().days(1),
        Granularity::Weekly => jiff::Span::new().weeks(1),
        Granularity::Monthly => jiff::Span::new().months(1),
    };
    start.checked_add(span).ok()
}

pub fn local_date(timestamp: Timestamp, tz: &TimeZone) -> Date {
    timestamp.to_zoned(tz.clone()).date()
}

#[derive(Default)]
struct Bucket {
    totals: UsageTotals,
    models: HashMap<(Source, String), UsageTotals>,
    pricing_missing: bool,
}

impl Bucket {
    fn add(&mut self, record: &UsageRecord) {
        self.totals.add_record(record);
        self.models
            .entry((record.source, record.model.clone()))
            .or_default()
            .add_record(record);
        self.pricing_missing |= record.pricing_missing;
    }

    fn sorted_models(self) -> Vec<ModelUsage> {
        let mut models: Vec<_> = self
            .models
            .into_iter()
            .map(|((source, model), totals)| ModelUsage {
                source,
                model,
                totals,
            })
            .collect();
        models.sort_by(|a, b| {
            b.totals
                .cost_usd
                .total_cmp(&a.totals.cost_usd)
                .then_with(|| b.totals.tokens.total().cmp(&a.totals.tokens.total()))
                .then_with(|| a.model.cmp(&b.model))
        });
        models
    }
}

/// 依期間彙總，結果依期間由舊到新排序；沒有用量的期間不會出現。
pub fn summarize_periods<'a>(
    records: impl IntoIterator<Item = &'a UsageRecord>,
    granularity: Granularity,
    tz: &TimeZone,
    week_start: Weekday,
) -> Vec<PeriodSummary> {
    let mut buckets: BTreeMap<Date, Bucket> = BTreeMap::new();
    for record in records {
        let start = period_start(local_date(record.timestamp, tz), granularity, week_start);
        buckets.entry(start).or_default().add(record);
    }
    buckets
        .into_iter()
        .map(|(start, bucket)| PeriodSummary {
            start,
            totals: bucket.totals,
            pricing_missing: bucket.pricing_missing,
            models: bucket.sorted_models(),
        })
        .collect()
}

/// 圖表用的連續序列：`from..=to` 間每個期間都有一筆，沒有用量的期間為 0。
pub fn continuous_series(
    summaries: &[PeriodSummary],
    granularity: Granularity,
    week_start: Weekday,
    from: Date,
    to: Date,
) -> Vec<PeriodSummary> {
    let by_start: HashMap<Date, &PeriodSummary> = summaries.iter().map(|s| (s.start, s)).collect();
    let mut out = Vec::new();
    let mut cursor = period_start(from, granularity, week_start);
    let end = period_start(to, granularity, week_start);
    // 上限防止錯誤參數造成無窮迴圈。
    while cursor <= end && out.len() < 10_000 {
        out.push(
            by_start
                .get(&cursor)
                .map(|s| (*s).clone())
                .unwrap_or(PeriodSummary {
                    start: cursor,
                    totals: UsageTotals::default(),
                    models: Vec::new(),
                    pricing_missing: false,
                }),
        );
        let Some(next) = next_period(cursor, granularity) else {
            break;
        };
        cursor = next;
    }
    out
}

/// Session 或專案的彙總。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupSummary {
    pub key: String,
    /// Session 所屬專案；專案彙總時與 `key` 相同。
    pub project: String,
    pub sources: BTreeSet<Source>,
    pub first_activity: Timestamp,
    pub last_activity: Timestamp,
    pub totals: UsageTotals,
    /// 依成本由高到低。
    pub models: Vec<ModelUsage>,
    pub pricing_missing: bool,
}

struct GroupBucket {
    project: String,
    sources: BTreeSet<Source>,
    first: Timestamp,
    last: Timestamp,
    bucket: Bucket,
}

fn summarize_groups<'a>(
    records: impl IntoIterator<Item = &'a UsageRecord>,
    key_of: impl Fn(&UsageRecord) -> String,
) -> Vec<GroupSummary> {
    let mut groups: HashMap<String, GroupBucket> = HashMap::new();
    for record in records {
        let group = groups.entry(key_of(record)).or_insert_with(|| GroupBucket {
            project: record.project.clone(),
            sources: BTreeSet::new(),
            first: record.timestamp,
            last: record.timestamp,
            bucket: Bucket::default(),
        });
        group.sources.insert(record.source);
        group.first = group.first.min(record.timestamp);
        group.last = group.last.max(record.timestamp);
        group.bucket.add(record);
    }
    let mut out: Vec<_> = groups
        .into_iter()
        .map(|(key, group)| GroupSummary {
            key,
            project: group.project,
            sources: group.sources,
            first_activity: group.first,
            last_activity: group.last,
            totals: group.bucket.totals,
            pricing_missing: group.bucket.pricing_missing,
            models: group.bucket.sorted_models(),
        })
        .collect();
    // 最近活動的排前面。
    out.sort_by(|a, b| {
        b.last_activity
            .cmp(&a.last_activity)
            .then_with(|| a.key.cmp(&b.key))
    });
    out
}

/// 依 Session 彙總（不同來源的 session id 不會互相合併）。
pub fn summarize_sessions<'a>(
    records: impl IntoIterator<Item = &'a UsageRecord>,
) -> Vec<GroupSummary> {
    summarize_groups(records, |r| format!("{}:{}", r.source.id(), r.session_id))
}

/// 依專案彙總（跨來源合併同名專案）。
pub fn summarize_projects<'a>(
    records: impl IntoIterator<Item = &'a UsageRecord>,
) -> Vec<GroupSummary> {
    summarize_groups(records, |r| r.project.clone())
}

/// 依模型彙總，成本由高到低。
pub fn summarize_models<'a>(records: impl IntoIterator<Item = &'a UsageRecord>) -> Vec<ModelUsage> {
    let mut bucket = Bucket::default();
    for record in records {
        bucket.add(record);
    }
    bucket.sorted_models()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::record;
    use jiff::civil::date;

    #[test]
    fn weekly_period_respects_week_start() {
        // 2026-11-12 是星期四
        let d = date(2026, 11, 12);
        assert_eq!(
            period_start(d, Granularity::Weekly, Weekday::Monday),
            date(2026, 11, 9)
        );
        assert_eq!(
            period_start(d, Granularity::Weekly, Weekday::Sunday),
            date(2026, 11, 8)
        );
        assert_eq!(period_start(d, Granularity::Weekly, Weekday::Thursday), d);
        assert_eq!(
            period_start(d, Granularity::Monthly, Weekday::Monday),
            date(2026, 11, 1)
        );
    }

    #[test]
    fn daily_summary_groups_by_local_date_and_model() {
        let tz = TimeZone::get("Asia/Taipei").unwrap();
        let records = vec![
            record(Source::ClaudeCode, "2026-11-11T15:00:00Z", "opus", 2.0), // 11-11 23:00
            record(Source::ClaudeCode, "2026-11-11T17:00:00Z", "opus", 1.0), // 11-12 01:00
            record(Source::ClaudeCode, "2026-11-11T18:00:00Z", "sonnet", 3.0), // 11-12 02:00
        ];
        let out = summarize_periods(&records, Granularity::Daily, &tz, Weekday::Monday);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].start, date(2026, 11, 11));
        assert_eq!(out[1].start, date(2026, 11, 12));
        assert_eq!(out[1].totals.requests, 2);
        assert_eq!(out[1].models[0].model, "sonnet", "成本高的模型排前面");
    }

    #[test]
    fn continuous_series_fills_gaps() {
        let tz = TimeZone::UTC;
        let records = vec![
            record(Source::Codex, "2026-11-01T00:00:00Z", "gpt", 1.0),
            record(Source::Codex, "2026-11-04T00:00:00Z", "gpt", 1.0),
        ];
        let summaries = summarize_periods(&records, Granularity::Daily, &tz, Weekday::Monday);
        let series = continuous_series(
            &summaries,
            Granularity::Daily,
            Weekday::Monday,
            date(2026, 10, 31),
            date(2026, 11, 4),
        );
        assert_eq!(series.len(), 5);
        assert_eq!(series[0].totals.requests, 0);
        assert_eq!(series[1].totals.requests, 1);
        assert_eq!(series[4].totals.requests, 1);
    }

    #[test]
    fn sessions_are_scoped_by_source() {
        let mut a = record(Source::ClaudeCode, "2026-11-01T00:00:00Z", "m", 1.0);
        a.session_id = "same".into();
        let mut b = record(Source::Codex, "2026-11-02T00:00:00Z", "m", 1.0);
        b.session_id = "same".into();
        let sessions = summarize_sessions(&[a, b]);
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].sources, [Source::Codex].into(), "最近的排前面");
    }
}
