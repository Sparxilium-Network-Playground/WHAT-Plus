# 開發說明

> 最後修改：2026-11-13

使用者安裝與操作請看 [README.md](../README.md)。原始設計草案見 [DESIGN.md](DESIGN.md)。

## 建置與檢查

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all
cargo build --release -p whatplus-client
```

- 價格快照：`.cargo/config.toml` 把 `CCUSAGE_PRICING_JSON_PATH` 指向 `vendor/pricing/litellm-pricing.json`，建置時不連網。
- Docker image：`docker build --target server .`、`docker build --target agent .`（同一個 `Dockerfile`）。
- russh 使用 `ring` 後端（`default-features = false`）。預設的 aws-lc-sys 在 Windows 需要 NASM，無法建置。
- SQLite 使用 `sqlite` crate，不用 rusqlite。ccusage 的 Antigravity adapter 已經連結 `sqlite3`，同一個 workspace 只能有一個連結 `sqlite3` 的套件。`whatplus-server/src/db/sql.rs` 是它的薄包裝。

## 架構

```
                SSH (russh)，subsystem "whatplus"，長度前綴 JSON
whatplus-agent ───────────────────────────┐
  (Docker, 每台電腦)                       ▼
  what-sources → UsageRecord ──Upload──▶ whatplus-server (Docker) ──REST──▶ Discord
                                           │ SQLite、排程、報告
whatplus-client ──Status/Schedules/...─────┘  ▲
  (Slint, Windows)                            │ 同一個資料庫
                              whatplus-server admin (TUI, docker compose exec)
```

| Crate | 職責 |
|---|---|
| `what-core`、`what-sources`、`what-pricing` | 從 WHAT 複製：資料模型、ccusage 轉接、價格表 |
| `whatplus-proto` | `Request` / `Response`、資料型別、封包（4 bytes 長度＋JSON，上限 16 MB） |
| `whatplus-ssh` | 金鑰掃描／解析／產生（只支援 Ed25519）、指紋、IP 白名單、用戶端連線（TOFU） |
| `whatplus-term` | 伺服器與來源端 TUI 共用的純文字繪製（GitStatus 風格） |
| `whatplus-server` | daemon（`run`）與 Admin TUI（`admin`） |
| `whatplus-agent` | 上傳（`run`）與設定 TUI（`admin`） |
| `whatplus-client` | Slint 管理客戶端 |

### 伺服器（`whatplus-server/src`）

| 模組 | 職責 |
|---|---|
| `config` | `config.conf`（key=value，有 `config.version`）；secret 只存路徑 |
| `db/` | 所有 SQL。設定變更一律經 `Store::mutate`（transaction + `revision + 1`）。`schema.sql`、`PRAGMA user_version` |
| `auth` | 公鑰＋白名單快照，依 revision 重新載入（對應 GitStatus 的 RuleSetHolder） |
| `ssh/` | `mod.rs`：自己 accept TCP，**握手前**檢查白名單與「至少一把 admin」；`handler.rs`：russh Handler；`session.rs`：一問一答迴圈 |
| `api/` | 請求處理，依角色限制（agent 只能 `Upload` / `UploadState`） |
| `report/` | `period`（期間與排程時間）、`aggregate`（彙總）、`format`（en / zh-TW 文字、切成 ≤1900 字的訊息） |
| `delivery`、`scheduler` | 產生並發送報告；排程每 30 秒檢查 |
| `discord/` | REST（列出 Server／頻道、發訊息，`allowed_mentions` 為空）；`gateway` 只在第一次啟動時 Identify 一次 |
| `sanitize` | 資料值進 Discord 前：移除控制字元與 bidi 控制字元、跳脫 Markdown、中和 `@`、截斷 |
| `tui/` | Admin TUI：Authorized keys、Import keys、Network allowlist、System status、Restart service |

### 來源端（`whatplus-agent/src`）

- `collect`：呼叫 `what_sources::all_sources()` 讀取並用內嵌價格表計價。
- `snapshot`：容器內唯讀掛載的 Antigravity SQLite（WAL）無法與主機的寫入端共用 `-shm`，會讀到過期內容。設定 `WHATPLUS_ANTIGRAVITY_SNAPSHOT`（image 預設 `/tmp/whatplus-antigravity`）時，讀取前先把 `*.db`／`*.db-wal` 複製到該目錄（只複製大小或時間有變的檔案），再讓 ccusage 讀快照。`run` 與 `admin` 各用一個子目錄。
- `upload`：先問伺服器這台機器最新一筆的時間，往前 24 小時起重送（伺服器以內容去重），分批送出，每批最多 2000 筆。伺服器沒有資料時全部上傳。
- `daemon`：每 30 秒讀一次 `agent.json`，間隔到了就上傳。伺服器位址、私鑰、已確認的指紋三者缺一就不連線。
- `tui/`：Server address、Private key（掃描或產生）、Verify server（TOFU）、Upload interval、Upload now、Re-send all、Show local usage sources。

### 客戶端（`whatplus-client`）

```
ui/
  app.slint            主視窗：側邊導覽＋頁面
  state.slint          前後端介面：每頁一個 global
  theme.slint          色彩與尺寸（同 WHAT）
  widgets/             common（PageFrame、FormRow、CheckList、ContentOptions、ListRow）、sidebar
  pages/               每頁一個檔案
lang/zh_TW/LC_MESSAGES/whatplus-client.po
src/
  main.rs              啟動
  backend.rs           背景 tokio runtime、SSH 連線（閒置斷線時自動重連一次）
  ctx.rs               各頁共用狀態；`Ctx::call` 送請求並把結果帶回 UI 執行緒
  settings.rs          client.json（%APPDATA%\WHATPlus\config；WHATPLUS_CLIENT_CONFIG 可覆寫，測試用）
  convert.rs           協定型別 ↔ Slint 結構
  i18n.rs              Rust 端文字的中英文
  pages/               每頁一個 controller
```

- `.slint` 字串用 `@tr("English")`，翻譯放 `.po`（不使用 msgctxt）。新增或修改 `@tr` 後要同步更新 `.po`。
- **不要把多個頁面寫進同一個檔案**：新增頁面時，各加一個 `ui/pages/*.slint` 與 `src/pages/*.rs`。

## 協定

- 連線後第一個請求必須是 `Hello { protocol }`，版本不同就拒絕。
- 角色檢查在 `api::allowed`；admin 公鑰只能在伺服器 TUI 管理（`admin_keys::require_agent`）。
- 新增請求：`whatplus-proto::Request` 加一個變體 → `api::request_name` / `allowed` / `handle` → 補測試（`ssh/tests.rs` 是端到端範例）。不相容的變更要把 `PROTOCOL_VERSION` 加一。

## 資料模型（SQLite）

| 表 | 用途 |
|---|---|
| `configuration_state` | `revision`，daemon 輪詢以重新載入驗證設定 |
| `authorized_key` | 公鑰、角色、指紋（唯一）、個別 IP 限制、啟用 |
| `allowed_network` | 全域白名單 |
| `key_activity` | 最後連線時間與 IP（執行期，不 bump revision） |
| `usage_record` | `(agent_id, record_key)` 唯一。`record_key` 是來源、時間、模型、session 與 token 數的 SHA-256 前 16 bytes；費用不列入，重送時會覆寫費用 |
| `discord_destination`、`destination_health` | 目的地與最近發送結果 |
| `report_schedule`、`schedule_run` | 排程與最近一次觸發。修改排程時 `created_at` 重設為現在，避免補送 |
| `runtime_status` | 心跳、host key 指紋、Discord 狀態、重啟 token、最近報告 |

變更 schema 時要把 `db::SCHEMA_VERSION` 加一並寫升級步驟；變更 `config.conf` 時要把 `config::CONFIG_VERSION` 與 setup 腳本的 `CONFIG_VERSION` 加一。

## 安全對照

| 項目 | 實作 |
|---|---|
| 傳輸 | 全程 SSH；只開一個 port；不提供 shell、exec、pty、轉發 |
| 驗證 | 只接受 Ed25519 公鑰；`auth_rejection_time` 1 秒；每條連線最多 3 次嘗試；同時最多 32 條連線 |
| IP | 握手前檢查全域白名單（空白名單＝全部拒絕）；個別公鑰可再限制 CIDR |
| Fail-closed | 沒有啟用中的 admin 公鑰時拒絕所有連線、不排程；TUI 不允許停用或刪除最後一把 admin |
| 伺服器身分 | host key 第一次啟動時產生（`data/`，0600）；用戶端採 TOFU，指紋不符就拒絕 |
| Secret | Discord token 只從檔案讀取，錯誤訊息只含路徑；客戶端拿不到 |
| 輸出 | Discord 內容經 `sanitize`，`allowed_mentions` 為空；channel id 只接受數字 |
| 容器 | uid 10001（來源端在 Linux 上以日誌擁有者的 uid 執行）；日誌資料夾唯讀掛載 |

已知限制：Docker Desktop（Windows／macOS）的 port 轉送不保留來源 IP，伺服器看到的是 Docker 內部的閘道位址，所以白名單無法分辨不同的電腦。建議用 Linux 架伺服器。

## 驗證紀錄（2026-11-13）

- `cargo test --workspace` 全數通過。`ssh/tests.rs` 端到端涵蓋：TOFU、指紋不符、未註冊的金鑰、角色限制、重送去重、停用金鑰、空白名單、沒有 admin。
- 本機實測（Windows，不經 Docker）：來源端讀取真實日誌，上傳 15,453 筆，伺服器筆數一致；客戶端連線、狀態、預覽報告、排程頁、來源機器頁正常。
- Docker（Windows 11 + Docker Desktop 29.9）：兩個 image 建置成功（各約 126 MB）。伺服器容器：產生 host key、沒有 admin 時拒絕連線、白名單外的連線在握手前關閉、Discord token 錯誤時顯示 `ERROR`。來源端容器以唯讀掛載真實日誌上傳 15,542 筆，三個工具的筆數與原生執行一致（修正 Antigravity WAL 問題後）。伺服器看到的來源 IP 是 `172.17.0.1`（Docker 閘道），證實上述已知限制。
- 測試時從主機（Windows）直接寫入容器正在使用的 SQLite 會遺失：WAL 的 `-shm` 無法跨主機與 Docker VM 共用。要從主機改資料庫，先停止容器、寫入後 `PRAGMA wal_checkpoint(TRUNCATE)` 再啟動。正式使用一律透過容器內的 TUI，不受影響。
- **尚未驗證**：實際發送 Discord 訊息（沒有測試用 Bot Token）、TUI 的按鍵操作流程、setup 腳本的互動流程。
