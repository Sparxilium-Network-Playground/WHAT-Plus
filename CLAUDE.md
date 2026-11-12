# CLAUDE.md

開始任何工作前，先完整閱讀並遵守 `.claude/Common-dev-instruct.md`（Agent 工作規範）。

- 記憶檔：`.claude/MEMORY.md`（不要寫入家目錄的 `~/.claude/`）。
- 開發文件入口：`docs/PROJECT-DEV.md`。
- 每完成一個功能就 commit（commit 前跑 `cargo test --workspace`）；不 push、不改寫歷史。
- 檔案保持小而模組化：每個畫面／頁面一個檔案，不要寫出巨大的 UI 檔。
- 伺服器端與來源端（TUI、log、錯誤訊息）一律英文；客戶端介面提供繁中／English。
