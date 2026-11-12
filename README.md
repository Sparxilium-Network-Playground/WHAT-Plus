# WHAT Plus

> 最後修改：2026-11-13

把多台電腦上 **Claude Code**、**OpenAI Codex**、**Google Antigravity** 的 token 用量與預估費用集中到一台伺服器，依排程把報告發到 Discord。

- 每台電腦跑一個**來源端**（Docker），預設每 10 分鐘把新的用量上傳到伺服器。
- **伺服器**（Docker）保存資料、依排程產生每日／每週／每月報告，用 Discord Bot 發到指定頻道。每個頻道可選繁體中文或 English。
- **管理客戶端**（Windows 桌面程式）負責設定：要發到哪些頻道、什麼時候發、報告內容，以及哪些電腦可以上傳。也可以按一下就立即發送，用來測試。

用量讀取與計價沿用 WHAT（What Have I Actually Transacted）與 [ccusage](https://github.com/ryoppippi/ccusage)。費用依公開 API 價格估算，**不是**實際帳單。

> 開發與技術細節請看 [docs/PROJECT-DEV.md](docs/PROJECT-DEV.md)。

## 安全設計

- 三個元件之間**只用 SSH 連線**（全程加密），沒有任何 HTTP 端點。
- 不使用密碼。每台電腦、每個客戶端各用一把 **Ed25519 金鑰**；伺服器只保存公鑰。
- 伺服器先檢查 **IP 白名單**（例如 `192.168.0.0/16`），不在名單內的連線在 SSH 握手前就關閉。
- 第一次連線時會顯示伺服器的**指紋**，請與伺服器設定畫面上顯示的指紋比對；之後指紋改變會拒絕連線。
- 金鑰分成兩種角色：
  - **admin**（管理客戶端）：可以改設定。
  - **agent**（來源端）：只能上傳用量。
- 伺服器**還沒有 admin 金鑰時不做任何事**：拒絕所有連線，也不發送報告。
- Discord Bot Token 只存在伺服器上，客戶端拿不到。報告中的 `@everyone` 等內容不會觸發提及。

## 你需要準備

- 一台可以執行 Docker 的伺服器（Linux，或 Windows + Docker Desktop），各電腦都能透過區網連到它。
- 每台要統計用量的電腦都安裝 Docker（Linux 或 Windows + Docker Desktop）。
- 一台 Windows 電腦執行管理客戶端。
- Discord Server 的管理權限（用來邀請 Bot）。

## 安裝

### 1. 建立 Discord Bot

到 [Discord Developer Portal](https://discord.com/developers/applications) → New Application → Bot：

- 複製 **Bot Token**。不需要開啟任何 Privileged Gateway Intents。
- OAuth2 URL Generator：Scope 只勾 `bot`，權限勾 **View Channels**、**Send Messages**。
- 用產生的連結把 Bot 邀請到你的 Discord Server。

### 2. 準備管理客戶端的金鑰

在要執行管理客戶端的 Windows 電腦上，建議產生一把**專用**金鑰：

```powershell
ssh-keygen -t ed25519 -f $env:USERPROFILE\.ssh\whatplus_admin -C "whatplus-admin"
```

可以設定 passphrase（客戶端連線時會詢問）。

### 3. 安裝伺服器

取得原始碼後進入 `deploy/server/`：

- Linux：

  ```bash
  cd deploy/server && sudo ./setup.sh
  ```

- Windows（Docker Desktop）：

  ```powershell
  cd deploy\server; powershell -ExecutionPolicy Bypass -File .\setup.ps1
  ```

精靈會詢問時區與 Discord Bot Token，接著建置並啟動服務，最後顯示伺服器的**指紋**（`SHA256:...`），請記下來。

### 4. 在伺服器上加入管理金鑰與白名單

1. 把步驟 2 的公鑰（`whatplus_admin.pub`）複製到伺服器的 `deploy/server/keys/` 目錄。一個檔案可以放多把公鑰，一行一把。
2. 開啟伺服器的設定畫面：

   ```bash
   docker compose exec server whatplus-server admin
   ```

3. **Network allowlist** → 按 `A` 新增區網範圍，例如 `192.168.0.0/16`。
4. **Import keys** → 勾選管理金鑰 → 角色選 **admin** → 取名字。

### 5. 連線管理客戶端

執行 `whatplus-client.exe`：

1. **連線**頁：填入伺服器位址，選擇步驟 2 的私鑰，按「連線」。
2. 第一次連線會顯示伺服器指紋，與步驟 3 記下的指紋**完全相同**才按「信任並連線」。
3. **Discord 頻道**頁：按「載入 Server 與頻道」，選擇頻道與報告語言後新增，可以按「測試」確認 Bot 能發訊息。

### 6. 安裝來源端（每台電腦）

進入 `deploy/agent/`：

- Linux：

  ```bash
  cd deploy/agent && ./setup.sh
  ```

- Windows（Docker Desktop）：

  ```powershell
  cd deploy\agent; powershell -ExecutionPolicy Bypass -File .\setup.ps1
  ```

精靈會找到 `.claude`、`.codex`、`.gemini` 資料夾（以唯讀方式掛載），然後啟動來源端。接著：

1. 開啟來源端設定畫面：

   ```bash
   docker compose exec agent whatplus-agent admin
   ```

2. **Server address**：填伺服器位址。
3. **Private key** → **Generate a new key**：產生這台電腦專用的金鑰，畫面會顯示公鑰（`ssh-ed25519 ...`）。
4. 在管理客戶端的**來源機器**頁，貼上這行公鑰並新增。
5. 回到來源端畫面，執行 **Verify server and test login**，比對指紋後確認。

之後來源端會自動上傳，第一次會上傳全部的歷史紀錄。

### 7. 設定排程

在管理客戶端：

- **排程**：新增每日、每週或每月報告，選擇時間、內容與頻道。時間以伺服器時區為準。
- **立即發送**：選擇期間與頻道，按「發送」就會馬上送出。可以先按「預覽」看內容。

## 報告內容

| 選項 | 內容 |
|---|---|
| 總費用與 token | 期間總費用、token、請求數 |
| 各工具 | Claude Code／Codex／Antigravity 分別的費用 |
| 各模型 | 費用最高的 8 個模型，其餘併為「其他」 |
| 各機器 | 每台來源電腦的費用 |
| 專案排行 | 費用最高的 5 個專案（機器 / 專案） |
| Token 明細 | 輸入、輸出、快取寫入、快取讀取 |

- 每日報告送前一天，每週報告送前 7 天，每月報告在 1 號送上個月。
- 伺服器停機超過 6 小時而錯過的報告不會補送。

## 常見問題

**用 Docker Desktop（Windows／macOS）架伺服器時，IP 白名單看到的位址不對**
Docker Desktop 會轉送連線，伺服器看到的來源位址可能都是 Docker 內部的閘道位址，而不是各電腦真正的 IP。這時白名單無法分辨不同的電腦，安全性主要靠金鑰。建議用 Linux 主機架伺服器。如果一定要用 Docker Desktop，先連一次，再到伺服器設定畫面的 **System status** 查看 `last seen` 顯示的 IP，並把白名單盡量縮小。

**客戶端顯示「伺服器拒絕這把金鑰」**
這把公鑰還沒加入伺服器、被停用了，或是設定的 IP 範圍不包含你的位址。

**客戶端或來源端連不上（connection reset）**
你的 IP 不在伺服器的白名單內，或是伺服器還沒有 admin 金鑰。

**指紋改變的警告**
伺服器重新安裝後（`data/` 被清除）會產生新的指紋。確認是自己重裝的，才在客戶端按「忘記」、在來源端重新設定伺服器位址，然後重新比對。否則請不要連線。

**某個工具的用量是 0**
在來源端設定畫面選 **Show local usage sources**，確認資料夾有正確掛載。

**Discord 顯示 `ERROR`**
在伺服器的 `deploy/server/` 重新執行 setup 輸入 Token，然後到伺服器設定畫面選 **Restart service**。

## 更新版本

```bash
git pull && sudo ./setup.sh upgrade
```

在 `deploy/server/` 執行。來源端在 `deploy/agent/` 執行 `docker compose up -d --build`。資料庫會自動升級。

## 授權

GNU Affero General Public License v3.0 or later，見 [LICENSE](LICENSE)。第三方授權見 [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)。
