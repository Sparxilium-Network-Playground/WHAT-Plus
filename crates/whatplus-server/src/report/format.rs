//! 報告文字（每種語言一份措辭）。資料值一律先經 [`crate::sanitize::clean`]。

use what_core::UsageTotals;
use whatplus_proto::{Language, ReportContent};

use super::aggregate::ReportData;
use super::period::{Period, ReportKind};
use crate::sanitize::clean;

/// Discord 單則訊息上限 2000 字元，留一點餘裕。
const MESSAGE_LIMIT: usize = 1900;
const NAME_LIMIT: usize = 60;

struct Texts {
    daily: &'static str,
    weekly: &'static str,
    monthly: &'static str,
    manual: &'static str,
    total: &'static str,
    by_tool: &'static str,
    by_model: &'static str,
    by_machine: &'static str,
    top_projects: &'static str,
    other: &'static str,
    requests: &'static str,
    tokens: &'static str,
    token_detail: &'static str,
    no_usage: &'static str,
    pricing_missing: &'static str,
}

const EN: Texts = Texts {
    daily: "Daily usage report",
    weekly: "Weekly usage report",
    monthly: "Monthly usage report",
    manual: "Usage report",
    total: "Total",
    by_tool: "By tool",
    by_model: "By model",
    by_machine: "By machine",
    top_projects: "Top projects",
    other: "Other",
    requests: "requests",
    tokens: "tokens",
    token_detail: "input {i} · output {o} · cache write {cw} · cache read {cr}",
    no_usage: "No usage recorded in this period.",
    pricing_missing: "Some models have no price data; costs may be underestimated.",
};

const ZH_TW: Texts = Texts {
    daily: "每日用量報告",
    weekly: "每週用量報告",
    monthly: "每月用量報告",
    manual: "用量報告",
    total: "總計",
    by_tool: "各工具",
    by_model: "各模型",
    by_machine: "各機器",
    top_projects: "專案排行",
    other: "其他",
    requests: "次請求",
    tokens: "tokens",
    token_detail: "輸入 {i} · 輸出 {o} · 快取寫入 {cw} · 快取讀取 {cr}",
    no_usage: "這段期間沒有用量紀錄。",
    pricing_missing: "部分模型沒有價格資料，費用可能低估。",
};

fn texts(lang: Language) -> &'static Texts {
    match lang {
        Language::En => &EN,
        Language::ZhTw => &ZH_TW,
    }
}

pub fn money(v: f64) -> String {
    format!("${v:.2}")
}

pub fn compact(n: u64) -> String {
    let f = n as f64;
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{:.1}K", f / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1}M", f / 1e6),
        _ => format!("{:.2}B", f / 1e9),
    }
}

fn line(t: &Texts, name: &str, u: &UsageTotals, show_tokens: bool) -> String {
    if show_tokens {
        format!("• {name}: **{}** · {} {}", money(u.cost_usd), compact(u.tokens.total()), t.tokens)
    } else {
        format!("• {name}: **{}**", money(u.cost_usd))
    }
}

/// 產生報告文字（不分割）。
pub fn render(
    data: &ReportData,
    period: &Period,
    timezone: &str,
    content: ReportContent,
    lang: Language,
) -> String {
    let t = texts(lang);
    let title = match period.kind {
        ReportKind::Daily => t.daily,
        ReportKind::Weekly => t.weekly,
        ReportKind::Monthly => t.monthly,
        ReportKind::Manual => t.manual,
    };
    let mut out = vec![
        format!("**{title}**"),
        format!("{} ({})", period.label(), clean(timezone, NAME_LIMIT)),
    ];

    if data.totals.requests == 0 {
        out.push(String::new());
        out.push(t.no_usage.to_string());
        return out.join("\n");
    }

    let tok = content.tokens;
    if content.totals {
        out.push(String::new());
        out.push(format!(
            "**{}**: {} · {} {} · {} {}",
            t.total,
            money(data.totals.cost_usd),
            compact(data.totals.tokens.total()),
            t.tokens,
            data.totals.requests,
            t.requests
        ));
        if tok {
            let k = &data.totals.tokens;
            out.push(
                t.token_detail
                    .replace("{i}", &compact(k.input))
                    .replace("{o}", &compact(k.output))
                    .replace("{cw}", &compact(k.cache_creation))
                    .replace("{cr}", &compact(k.cache_read)),
            );
        }
    }
    if content.by_tool && !data.by_tool.is_empty() {
        out.push(String::new());
        out.push(format!("**{}**", t.by_tool));
        for (source, u) in &data.by_tool {
            out.push(line(t, source.display_name(), u, tok));
        }
    }
    if content.by_model && !data.by_model.is_empty() {
        out.push(String::new());
        out.push(format!("**{}**", t.by_model));
        for (model, u) in &data.by_model {
            out.push(line(t, &clean(model, NAME_LIMIT), u, tok));
        }
        if let Some(other) = &data.other_models {
            out.push(line(t, t.other, other, tok));
        }
    }
    if content.by_machine && !data.by_machine.is_empty() {
        out.push(String::new());
        out.push(format!("**{}**", t.by_machine));
        for (machine, u) in &data.by_machine {
            out.push(line(t, &clean(machine, NAME_LIMIT), u, tok));
        }
    }
    if content.top_projects && !data.top_projects.is_empty() {
        out.push(String::new());
        out.push(format!("**{}**", t.top_projects));
        for (machine, project, u) in &data.top_projects {
            let name = format!("{} / {}", clean(machine, NAME_LIMIT), clean(project, NAME_LIMIT));
            out.push(line(t, &name, u, tok));
        }
    }
    if data.pricing_missing {
        out.push(String::new());
        out.push(format!("_{}_", t.pricing_missing));
    }
    out.join("\n")
}

/// 依行切成不超過 Discord 上限的多則訊息。
pub fn split_messages(text: &str) -> Vec<String> {
    let mut messages = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        let line: String = line.chars().take(MESSAGE_LIMIT).collect();
        if current.chars().count() + line.chars().count() + 1 > MESSAGE_LIMIT && !current.is_empty() {
            messages.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(&line);
    }
    if !current.is_empty() {
        messages.push(current);
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::period::manual_period;
    use whatplus_proto::ManualRange;

    #[test]
    fn empty_report_and_languages() {
        let now: jiff::Zoned = "2026-11-13T10:00:00+08:00[Asia/Taipei]".parse().unwrap();
        let p = manual_period(ManualRange::Today, &now).unwrap();
        let en = render(&ReportData::default(), &p, "Asia/Taipei", ReportContent::default(), Language::En);
        assert!(en.contains("No usage"));
        let zh = render(&ReportData::default(), &p, "Asia/Taipei", ReportContent::default(), Language::ZhTw);
        assert!(zh.contains("沒有用量"));
    }

    #[test]
    fn split_respects_limit() {
        let text = (0..500).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let parts = split_messages(&text);
        assert!(parts.len() > 1);
        assert!(parts.iter().all(|p| p.chars().count() <= MESSAGE_LIMIT));
    }

    #[test]
    fn compact_numbers() {
        assert_eq!(compact(999), "999");
        assert_eq!(compact(1_500), "1.5K");
        assert_eq!(compact(2_500_000), "2.5M");
    }
}
