# WHAT Plus

> 最後修改：2026-11-13

把多台電腦上 **Claude Code**、**OpenAI Codex**、**Google Antigravity** 的 token 用量與預估費用集中起來，依排程把報告發到 Discord。

適合同時在好幾台電腦（公司、家裡、伺服器）使用 AI 程式工具，想每天、每週或每月在 Discord 頻道看到「總共花了多少」的人或團隊。

## 功能

- **自動收集**：每台電腦跑一個來源端，預設每 10 分鐘把新的用量送到伺服器。只讀取，不會修改任何日誌。
- **排程報告**：每日、每週、每月自動發到 Discord，也可以隨時手動發送。
- **報告內容可選**：總費用與 token、各工具、各模型、各機器、專案排行、token 明細。
- **雙語**：每個 Discord 頻道可以選擇繁體中文或 English。
- **集中管理**：在 Windows 管理程式設定頻道、排程、報告內容，以及哪些電腦可以上傳。
- **安全**：
  - 只用 SSH 金鑰連線，全程加密，不使用密碼。
  - 先用 IP 白名單擋掉區網以外的連線。
  - 第一次連線時比對伺服器指紋。
  - 還沒設定管理金鑰前，伺服器不接受任何連線。

費用依公開 API 價格估算，**不是**實際帳單。用量讀取沿用 WHAT（What Have I Actually Transacted）與 [ccusage](https://github.com/ryoppippi/ccusage)。

## 組成

| 元件 | 執行在 | 用途 |
|---|---|---|
| 伺服器 | 一台 Docker 主機（Linux，或 Windows + Docker Desktop） | 保存用量、產生報告、發到 Discord |
| 來源端 | 每台要統計的電腦（Docker） | 讀取本機用量並上傳 |
| 管理客戶端 | 一台 Windows 電腦 | 設定頻道、排程、報告內容與來源機器 |

## 文件

| 文件 | 內容 |
|---|---|
| [安裝與更新](docs/INSTALL.md) | 準備事項、Discord Bot、伺服器與來源端的設定精靈、更新版本、常見問題 |
| [管理客戶端使用指南](docs/CLIENT-GUIDE.md) | 連線、Discord 頻道、排程、立即發送、來源機器 |
| [伺服器設定畫面](docs/SERVER-TUI.md) | 管理金鑰、IP 白名單、系統狀態 |
| [來源端設定畫面](docs/AGENT-TUI.md) | 伺服器位址、金鑰、立即上傳、檢查本機資料夾 |
| [開發說明](docs/PROJECT-DEV.md) | 給開發者 |

## 授權

GNU Affero General Public License v3.0 or later，見 [LICENSE](LICENSE)。第三方授權見 [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)。
