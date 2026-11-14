//! 報告內容（每種語言一份措辭），產生成 [`Card`]。資料值一律先經 [`crate::sanitize::clean`]。

use what_core::UsageTotals;
use whatplus_proto::{Language, ReportContent};

use super::aggregate::ReportData;
use super::card::{COLOR_EMPTY, COLOR_REPORT, COLOR_TEST, Card};
use super::period::{Period, ReportKind};
use crate::sanitize::clean;

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
        format!(
            "• {name}: **{}** · {} {}",
            money(u.cost_usd),
            compact(u.tokens.total()),
            t.tokens
        )
    } else {
        format!("• {name}: **{}**", money(u.cost_usd))
    }
}

/// 產生報告卡片。標題是報告種類，說明是期間與總計，其餘每個區塊一個欄位。
pub fn render(
    data: &ReportData,
    period: &Period,
    timezone: &str,
    content: ReportContent,
    lang: Language,
) -> Card {
    let t = texts(lang);
    let title = match period.kind {
        ReportKind::Daily => t.daily,
        ReportKind::Weekly => t.weekly,
        ReportKind::Monthly => t.monthly,
        ReportKind::Manual => t.manual,
    };
    let mut description = vec![format!(
        "{} ({})",
        period.label(),
        clean(timezone, NAME_LIMIT)
    )];

    if data.totals.requests == 0 {
        description.push(String::new());
        description.push(t.no_usage.to_string());
        let mut card = Card::new(title, COLOR_EMPTY);
        card.description = description.join("\n");
        return card.fit();
    }

    let mut card = Card::new(title, COLOR_REPORT);
    let tok = content.tokens;
    if content.totals {
        description.push(String::new());
        description.push(format!(
            "**{}**: **{}** · {} {} · {} {}",
            t.total,
            money(data.totals.cost_usd),
            compact(data.totals.tokens.total()),
            t.tokens,
            data.totals.requests,
            t.requests
        ));
        if tok {
            let k = &data.totals.tokens;
            description.push(
                t.token_detail
                    .replace("{i}", &compact(k.input))
                    .replace("{o}", &compact(k.output))
                    .replace("{cw}", &compact(k.cache_creation))
                    .replace("{cr}", &compact(k.cache_read)),
            );
        }
    }
    if data.pricing_missing {
        description.push(String::new());
        description.push(format!("_{}_", t.pricing_missing));
    }
    card.description = description.join("\n");

    if content.by_tool && !data.by_tool.is_empty() {
        let lines: Vec<String> = data
            .by_tool
            .iter()
            .map(|(source, u)| line(t, source.display_name(), u, tok))
            .collect();
        card.field(t.by_tool, &lines);
    }
    if content.by_model && !data.by_model.is_empty() {
        let mut lines: Vec<String> = data
            .by_model
            .iter()
            .map(|(model, u)| line(t, &clean(model, NAME_LIMIT), u, tok))
            .collect();
        if let Some(other) = &data.other_models {
            lines.push(line(t, t.other, other, tok));
        }
        card.field(t.by_model, &lines);
    }
    if content.by_machine && !data.by_machine.is_empty() {
        let lines: Vec<String> = data
            .by_machine
            .iter()
            .map(|(machine, u)| line(t, &clean(machine, NAME_LIMIT), u, tok))
            .collect();
        card.field(t.by_machine, &lines);
    }
    if content.top_projects && !data.top_projects.is_empty() {
        let lines: Vec<String> = data
            .top_projects
            .iter()
            .map(|(machine, project, u)| {
                let name = format!(
                    "{} / {}",
                    clean(machine, NAME_LIMIT),
                    clean(project, NAME_LIMIT)
                );
                line(t, &name, u, tok)
            })
            .collect();
        card.field(t.top_projects, &lines);
    }
    card.fit()
}

/// 「這個頻道已設定為報告目的地」的測試卡片。
pub fn test_card(destination_name: &str, lang: Language) -> Card {
    let name = clean(destination_name, NAME_LIMIT);
    let (title, body) = match lang {
        Language::En => (
            "WHAT Plus test message",
            format!("This channel is set up as a report destination \"{name}\"."),
        ),
        Language::ZhTw => (
            "WHAT Plus 測試訊息",
            format!("此頻道已設定為報告目的地「{name}」。"),
        ),
    };
    let mut card = Card::new(title, COLOR_TEST);
    card.description = body;
    card.fit()
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
        let en = render(
            &ReportData::default(),
            &p,
            "Asia/Taipei",
            ReportContent::default(),
            Language::En,
        );
        assert!(en.description.contains("No usage"));
        assert_eq!(en.color, COLOR_EMPTY);
        assert!(en.fields.is_empty());
        let zh = render(
            &ReportData::default(),
            &p,
            "Asia/Taipei",
            ReportContent::default(),
            Language::ZhTw,
        );
        assert!(zh.description.contains("沒有用量"));
    }

    #[test]
    fn test_card_mentions_destination() {
        let card = test_card("Server / #dev @everyone", Language::ZhTw);
        assert_eq!(card.title, "WHAT Plus 測試訊息");
        // `#` 會被跳脫成 `\#`（Markdown），顯示時仍是 `#`。
        assert!(card.description.contains("Server / \\#dev"));
        assert!(!card.description.contains("@everyone"));
    }

    #[test]
    fn compact_numbers() {
        assert_eq!(compact(999), "999");
        assert_eq!(compact(1_500), "1.5K");
        assert_eq!(compact(2_500_000), "2.5M");
    }
}
