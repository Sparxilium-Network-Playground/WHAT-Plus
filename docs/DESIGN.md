# WHAT Plus — 設計草案

> 狀態：已確認並實作（2026-11-13）。實作細節以 docs/PROJECT-DEV.md 為準；與草案不同處：SQLite 使用 sqlite crate、伺服器 port 以 Docker 對區網開放。

把多台電腦上 Claude Code / Codex / Antigravity 的用量集中到一台伺服器，依排程發送報告到 Discord。

參考：
- 伺服器邏輯：`C:/code/GitStatus`（設定 revision、Admin TUI、Sanitizer、fail-closed、只發不收的 Discord Bot、setup.sh）
- 用量讀取：`C:/code/What-Have-I-Actually-Transacted`（`what-core`、`what-sources`、`what-pricing`，底層為 ccusage）

## 元件

| 元件 | 形式 | 平台 | 職責 |
|---|---|---|---|
| `whatplus-server` | Docker（Linux 容器） | Linux、Windows（Docker Desktop） | SSH 服務（russh）、收原始紀錄、存 SQLite、排程產生報告、Discord Bot 發送；Admin TUI（`docker compose exec`） |
| `whatplus-agent`（來源端） | Docker（Linux 容器） | Linux、Windows（Docker Desktop） | 唯讀掛載使用者的 `.claude` / `.codex` / `.gemini`，每 10 分鐘（可設定）把新紀錄推送到伺服器；設定用 TUI（`docker compose exec`） |
| `whatplus-client`（管理客戶端） | Slint 桌面 GUI，單一 EXE | 僅 Windows | 設定報告排程與內容、Discord 頻道、來源公鑰；手動推送測試報告 |

## 通訊與驗證：全程 SSH（russh）

- 整個系統**沒有 HTTP 端點**，只有伺服器的一個 SSH port（暫定 `33445`）。
- 連線先比對 **IP 白名單**（CIDR，例如 `192.168.0.0/16`），不在名單內就直接斷線，不進入 SSH 握手。白名單是空的時候拒絕所有連線。
- 用戶端只能用 **Ed25519 公鑰**驗證，不支援密碼或 keyboard-interactive。
- 不提供 shell、exec、port forwarding，只接受一個自訂 subsystem（`whatplus`）。subsystem 內以長度前綴的 JSON 訊息交換。
- 驗證成功後依公鑰的**角色**限制可用的指令：
  - `agent`（來源端）：只能上傳紀錄、讀取自己的上傳狀態。
  - `admin`（管理客戶端）：讀寫報告設定、列出 Discord 頻道、手動推送，以及新增或停用 `agent` 公鑰。**不能新增 admin**。
- 伺服器的 host key 在第一次啟動時自動產生，存在 `data/`。來源端和客戶端第一次連線時顯示指紋，使用者確認後才記住（TOFU）；之後指紋變了就拒絕連線。伺服器 TUI 會顯示自己的指紋，方便對照。
- 每把公鑰可以另外設定允許的 IP（預設套用全域白名單）。

### 金鑰註冊

- 伺服器 TUI 掃描掛載的目錄（例如 `deploy/keys/`，可在 TUI 指定其他目錄），列出所有 `.pub` 與 `authorized_keys` 格式的檔案。**一個檔案裡有多把公鑰時逐把列出**，非 Ed25519 的公鑰會標示為不支援。
- 可以勾選多把公鑰並指定角色（`admin` / `agent`）和名稱，不限數量。
- **還沒有任何 admin 公鑰時**，伺服器只執行 TUI，SSH 服務拒絕所有連線，也不排程、不發送。
- 來源端和客戶端在 TUI 或 GUI 中選擇要用 `~/.ssh` 或其他目錄的哪一把私鑰。
- 建議每台機器產生一把專用金鑰，不要拿平常登入其他主機的 key。來源端在容器中執行，無法輸入 passphrase，因此需要不帶 passphrase 的專用 key；客戶端（GUI）可以支援帶 passphrase 的 key。

## 資料

- 來源端讀取**原始紀錄**（沿用 WHAT 的 `UsageRecord`：時間、工具、模型、token、費用、專案、Session），用自己內嵌的價格表計價後上傳。
- 增量上傳：來源端記錄伺服器最後確認的位置；伺服器以「來源 + 紀錄 ID」去重（`INSERT OR IGNORE`），重送不會重複計算。
- 伺服器 SQLite：`source_key`、`usage_record`、`report_schedule`、`discord_destination`、`delivery_log`、`runtime_status`、`configuration_state(revision)`。設定變更一律在 transaction 內 `revision + 1`（同 GitStatus `ConfigStore.mutate`）。

## 報告

- 第一版：
  1. **手動推送**（客戶端按鈕，用來測試）
  2. 每日報告（指定時間，送前一天）
  3. 每週報告（指定星期幾＋時間）
  4. 每月報告（每月 1 號送上個月）
- 內容可勾選：總費用、各工具、各模型、各機器、專案排行、token 明細。
- 時區由伺服器設定。每個頻道可以選擇語言（English / 繁體中文）。
- 送到 Discord 的文字一律經 Sanitizer 處理（中和 mention、跳脫 Markdown、截斷），`allowed_mentions` 為空。

## Discord

- Bot Token **只在伺服器的 `setup.sh` 設定**，存成 secret 檔（唯讀掛載）。客戶端拿不到，也改不了。
- Bot 只負責發訊息，不註冊指令、不監聽訊息、不用 privileged intent。
- 客戶端透過伺服器列出 Bot 所在的 Server 和頻道，選好後當作報告的目的地。

## 程式結構（Rust workspace）

```
crates/
  what-core, what-sources, what-pricing   從 WHAT 複製（Docker build context 無法參考 repo 以外的路徑）
  whatplus-proto     訊息型別、角色、序列化（server / agent / client 共用）
  whatplus-ssh       russh 共用：金鑰載入、.pub / authorized_keys 解析、指紋、known_hosts、IP 白名單
  whatplus-server    binary：run / admin(TUI)
  whatplus-agent     binary：run / admin(TUI)
  whatplus-client    Slint GUI（Windows）
deploy/
  server/  docker-compose.yml、setup.sh、setup.ps1
  agent/   docker-compose.yml、setup.sh、setup.ps1
```

- 伺服器、來源端（TUI、log、錯誤訊息）一律英文；客戶端 GUI 提供繁中／English；Discord 報告依頻道設定的語言。

## 已知風險（使用者已確認接受或待後續加強）

- 伺服器的 SSH port 必須開放給區網（不像 GitStatus 只綁 127.0.0.1）。目前以 IP 白名單＋公鑰驗證＋角色限制保護。
- 來源端容器的私鑰沒有 passphrase：容器被入侵時，這把 key 可以冒充這台機器上傳紀錄，但不能改設定。
- 原始紀錄含專案名稱與 Session，集中存放在伺服器上。
