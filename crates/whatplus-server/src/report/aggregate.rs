//! 把紀錄彙總成報告資料。

use std::collections::HashMap;

use what_core::{Source, UsageTotals};

use crate::db::StoredRecord;

/// 報告中專案排行的筆數。
pub const TOP_PROJECTS: usize = 5;
/// 各模型最多列出幾筆，其餘併入「其他」。
pub const MAX_MODELS: usize = 8;

#[derive(Debug, Clone, Default)]
pub struct ReportData {
    pub totals: UsageTotals,
    pub by_tool: Vec<(Source, UsageTotals)>,
    /// 依費用由高到低。
    pub by_model: Vec<(String, UsageTotals)>,
    pub other_models: Option<UsageTotals>,
    pub by_machine: Vec<(String, UsageTotals)>,
    /// (機器, 專案, 用量)
    pub top_projects: Vec<(String, String, UsageTotals)>,
    pub pricing_missing: bool,
}

fn sorted_by_cost<K>(map: HashMap<K, UsageTotals>) -> Vec<(K, UsageTotals)> {
    let mut items: Vec<_> = map.into_iter().collect();
    items.sort_by(|a, b| {
        b.1.cost_usd
            .total_cmp(&a.1.cost_usd)
            .then(b.1.tokens.total().cmp(&a.1.tokens.total()))
    });
    items
}

pub fn aggregate(records: &[StoredRecord]) -> ReportData {
    let mut data = ReportData::default();
    let mut tools: HashMap<Source, UsageTotals> = HashMap::new();
    let mut models: HashMap<String, UsageTotals> = HashMap::new();
    let mut machines: HashMap<String, UsageTotals> = HashMap::new();
    let mut projects: HashMap<(String, String), UsageTotals> = HashMap::new();

    for stored in records {
        let r = &stored.record;
        data.totals.add_record(r);
        data.pricing_missing |= r.pricing_missing;
        tools.entry(r.source).or_default().add_record(r);
        let model = if r.model.is_empty() {
            "unknown".to_string()
        } else {
            r.model.clone()
        };
        models.entry(model).or_default().add_record(r);
        machines
            .entry(stored.agent_name.clone())
            .or_default()
            .add_record(r);
        projects
            .entry((stored.agent_name.clone(), r.project.clone()))
            .or_default()
            .add_record(r);
    }

    data.by_tool = Source::ALL
        .into_iter()
        .filter_map(|s| tools.remove(&s).map(|t| (s, t)))
        .collect();
    let mut models = sorted_by_cost(models);
    if models.len() > MAX_MODELS {
        let mut other = UsageTotals::default();
        for (_, t) in models.drain(MAX_MODELS..) {
            other.merge(&t);
        }
        data.other_models = Some(other);
    }
    data.by_model = models;
    data.by_machine = sorted_by_cost(machines);
    data.top_projects = sorted_by_cost(projects)
        .into_iter()
        .take(TOP_PROJECTS)
        .map(|((m, p), t)| (m, p, t))
        .collect();
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use what_core::{TokenCounts, UsageRecord};

    fn rec(machine: &str, source: Source, model: &str, cost: f64) -> StoredRecord {
        StoredRecord {
            agent_name: machine.into(),
            record: UsageRecord {
                source,
                timestamp: jiff::Timestamp::UNIX_EPOCH,
                model: model.into(),
                project: "p".into(),
                session_id: "s".into(),
                tokens: TokenCounts {
                    input: 1,
                    ..Default::default()
                },
                cost_usd: cost,
                pricing_missing: false,
            },
        }
    }

    #[test]
    fn groups_and_sorts() {
        let data = aggregate(&[
            rec("a", Source::Codex, "gpt", 1.0),
            rec("b", Source::ClaudeCode, "opus", 5.0),
            rec("a", Source::ClaudeCode, "opus", 2.0),
        ]);
        assert_eq!(data.totals.requests, 3);
        assert_eq!(data.by_tool[0].0, Source::ClaudeCode);
        assert_eq!(data.by_model[0].0, "opus");
        assert_eq!(data.by_machine[0].0, "b");
        assert_eq!(data.top_projects.len(), 2);
    }
}
