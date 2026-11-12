//! Rust 端產生的文字翻譯。`.slint` 內的文字用 `@tr` + `lang/zh_TW/.../whatplus-client.po`。

pub fn pick(zh: bool, en: &str, zh_text: &str) -> String {
    if zh {
        zh_text.to_string()
    } else {
        en.to_string()
    }
}

/// 切換 Slint 的內嵌翻譯。
pub fn apply(language: &str) {
    let lang = if language == "zh-TW" { "zh_TW" } else { "en" };
    // 英文是原文，找不到 en 翻譯時 Slint 會回到原文。
    let _ = slint::select_bundled_translation(lang);
}
