//! 依來源與日期範圍篩選紀錄。

use std::collections::BTreeSet;

use jiff::{civil::Date, tz::TimeZone};

use crate::model::{Source, UsageRecord};

/// 篩選條件。日期以指定時區的當地日期判斷，兩端皆包含。
#[derive(Debug, Clone, Default)]
pub struct RecordFilter {
    /// 空集合代表全部來源。
    pub sources: BTreeSet<Source>,
    pub since: Option<Date>,
    pub until: Option<Date>,
}

impl RecordFilter {
    pub fn matches(&self, record: &UsageRecord, tz: &TimeZone) -> bool {
        if !self.sources.is_empty() && !self.sources.contains(&record.source) {
            return false;
        }
        if self.since.is_none() && self.until.is_none() {
            return true;
        }
        let date = record.timestamp.to_zoned(tz.clone()).date();
        self.since.is_none_or(|since| date >= since) && self.until.is_none_or(|until| date <= until)
    }

    pub fn apply<'a>(&self, records: &'a [UsageRecord], tz: &TimeZone) -> Vec<&'a UsageRecord> {
        records.iter().filter(|r| self.matches(r, tz)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::record;

    #[test]
    fn filters_by_source_and_local_date() {
        let tz = TimeZone::get("Asia/Taipei").unwrap();
        // 2026-11-11T17:00Z 在台北是 11-12 01:00
        let records = vec![
            record(Source::ClaudeCode, "2026-11-11T17:00:00Z", "m", 1.0),
            record(Source::Codex, "2026-11-11T10:00:00Z", "m", 1.0),
        ];
        let filter = RecordFilter {
            sources: [Source::ClaudeCode].into(),
            since: Some(jiff::civil::date(2026, 11, 12)),
            until: None,
        };
        let kept = filter.apply(&records, &tz);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].source, Source::ClaudeCode);
    }
}
