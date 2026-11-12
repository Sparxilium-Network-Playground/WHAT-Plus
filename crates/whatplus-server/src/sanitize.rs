//! 送到 Discord 前的文字處理（對應 GitStatus 的 Sanitizer）。
//!
//! 所有來自來源端的資料值（模型、專案、機器名稱）都要經過 [`clean`]：
//! 移除控制字元、跳脫 Markdown、中和 mention、依字元數截斷。

// 不跳脫 `-` 與 `:`：資料值不會出現在行首，而模型名稱常含這兩個字元，跳脫後很難讀。
const MARKDOWN: &[char] = &['\\', '*', '_', '~', '`', '|', '>', '#', '[', ']', '(', ')'];
const ZERO_WIDTH_SPACE: char = '\u{200B}';

pub fn clean(raw: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut count = 0;
    for ch in raw.chars() {
        if count >= max_chars {
            out.push('…');
            break;
        }
        if ch.is_control() || is_bidi_control(ch) {
            continue;
        }
        if MARKDOWN.contains(&ch) {
            out.push('\\');
        }
        out.push(ch);
        if ch == '@' {
            // `@everyone`、`@here`、`<@id>` 都不會成為 mention。
            out.push(ZERO_WIDTH_SPACE);
        }
        count += 1;
    }
    if out.is_empty() { "-".to_string() } else { out }
}

fn is_bidi_control(ch: char) -> bool {
    matches!(ch, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutralizes_mentions_and_markdown() {
        let s = clean("@everyone **x**\n\u{202E}", 100);
        assert!(!s.contains("@everyone"));
        assert!(s.contains("\\*\\*x\\*\\*"));
        assert!(!s.contains('\n'));
    }

    #[test]
    fn truncates_by_chars() {
        assert_eq!(clean("繁體中文字", 2), "繁體…");
        assert_eq!(clean("", 5), "-");
    }
}
