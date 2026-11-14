//! 報告卡片：送到 Discord 時是一則 embed，給客戶端預覽時轉成純文字。
//!
//! 所有欄位在建立後經 [`Card::fit`] 套用 Discord embed 的長度上限，超出的部分截斷並加上「…」。
//! 資料值在放進卡片之前已經過 [`crate::sanitize::clean`]。

/// 一般報告。
pub const COLOR_REPORT: u32 = 0x0969DA;
/// 期間內沒有用量。
pub const COLOR_EMPTY: u32 = 0x6E7781;
/// 測試訊息。
pub const COLOR_TEST: u32 = 0x8250DF;

// Discord embed 上限：https://discord.com/developers/docs/resources/message#embed-object-embed-limits
const TITLE_LIMIT: usize = 256;
const DESCRIPTION_LIMIT: usize = 4096;
const FIELD_COUNT_LIMIT: usize = 25;
const FIELD_NAME_LIMIT: usize = 256;
const FIELD_VALUE_LIMIT: usize = 1024;
const FOOTER_LIMIT: usize = 2048;
/// 整個 embed 的字元總數上限 6000，留一點餘裕。
const TOTAL_LIMIT: usize = 5800;

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub title: String,
    pub description: String,
    pub color: u32,
    pub fields: Vec<Field>,
    pub footer: String,
}

impl Card {
    pub fn new(title: impl Into<String>, color: u32) -> Self {
        Self {
            title: title.into(),
            description: String::new(),
            color,
            fields: Vec::new(),
            footer: "WHAT Plus".into(),
        }
    }

    pub fn field(&mut self, name: impl Into<String>, lines: &[String]) {
        self.fields.push(Field {
            name: name.into(),
            value: lines.join("\n"),
        });
    }

    /// 套用 Discord 的長度上限。多行的內容依行截斷，不會切在一行的中間。
    pub fn fit(mut self) -> Self {
        self.title = truncate(&self.title, TITLE_LIMIT);
        self.description = truncate_lines(&self.description, DESCRIPTION_LIMIT);
        self.footer = truncate(&self.footer, FOOTER_LIMIT);
        self.fields.truncate(FIELD_COUNT_LIMIT);
        for f in &mut self.fields {
            f.name = truncate(&f.name, FIELD_NAME_LIMIT);
            f.value = truncate_lines(&f.value, FIELD_VALUE_LIMIT);
        }
        // 總長超過時從最後一個欄位開始丟掉。
        while self.total_chars() > TOTAL_LIMIT && !self.fields.is_empty() {
            self.fields.pop();
        }
        self
    }

    fn total_chars(&self) -> usize {
        let count = |s: &str| s.chars().count();
        count(&self.title)
            + count(&self.description)
            + count(&self.footer)
            + self
                .fields
                .iter()
                .map(|f| count(&f.name) + count(&f.value))
                .sum::<usize>()
    }

    /// 給客戶端預覽用的純文字（Markdown 粗體保留，與 Discord 上看到的接近）。
    pub fn to_text(&self) -> String {
        let mut out = vec![format!("**{}**", self.title)];
        if !self.description.is_empty() {
            out.push(self.description.clone());
        }
        for f in &self.fields {
            out.push(String::new());
            out.push(format!("**{}**", f.name));
            out.push(f.value.clone());
        }
        out.join("\n")
    }
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut s: String = text.chars().take(limit - 1).collect();
    s.push('…');
    s
}

/// 依行截斷；放不下的行不放，最後加一行「…」。
fn truncate_lines(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut out = String::new();
    for line in text.lines() {
        let needed = out.chars().count() + line.chars().count() + 1;
        if needed + 2 > limit {
            break;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
    }
    if out.is_empty() {
        return truncate(text, limit);
    }
    out.push_str("\n…");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_applies_discord_limits() {
        let mut card = Card::new("t".repeat(300), COLOR_REPORT);
        let lines: Vec<String> = (0..200)
            .map(|i| format!("• machine-{i}: **$1.00**"))
            .collect();
        card.field("By machine", &lines);
        for i in 0..30 {
            card.field(format!("f{i}"), &["x".repeat(500)]);
        }
        let card = card.fit();
        assert_eq!(card.title.chars().count(), TITLE_LIMIT);
        assert!(card.fields.len() <= FIELD_COUNT_LIMIT);
        assert!(card.fields[0].value.chars().count() <= FIELD_VALUE_LIMIT);
        assert!(card.fields[0].value.ends_with("\n…"));
        assert!(card.total_chars() <= TOTAL_LIMIT);
    }

    #[test]
    fn text_preview_contains_fields() {
        let mut card = Card::new("Daily usage report", COLOR_REPORT);
        card.description = "2026-11-13".into();
        card.field("By tool", &["• Codex: **$1.00**".to_string()]);
        let text = card.to_text();
        assert!(text.starts_with("**Daily usage report**\n2026-11-13"));
        assert!(text.contains("**By tool**\n• Codex"));
    }
}
