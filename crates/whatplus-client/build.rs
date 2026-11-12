fn main() {
    // .po 檔不使用 msgctxt：同一個英文字串在所有元件中共用同一個翻譯。
    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent".into())
        .with_bundled_translations("lang")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/app.slint", config).expect("compile Slint UI");
    println!("cargo:rerun-if-changed=lang");

    #[cfg(windows)]
    embed_windows_resources();
}

/// EXE 圖示與「內容 › 詳細資料」的版本資訊。
#[cfg(windows)]
fn embed_windows_resources() {
    println!("cargo:rerun-if-changed=assets/what.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/what.ico")
        .set("ProductName", "WHAT Plus")
        .set("FileDescription", "WHAT Plus client")
        .set("OriginalFilename", "whatplus-client.exe")
        .set("InternalName", "whatplus-client")
        .set("LegalCopyright", "Licensed under AGPL-3.0-or-later");
    res.compile().expect("embed Windows resources");
}
