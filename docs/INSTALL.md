# 安裝與更新

> 最後修改：2026-11-13

這份文件帶你從零開始裝好 WHAT Plus：一台**伺服器**、每台要統計的電腦各一個**來源端**，以及一台 Windows 電腦上的**管理客戶端**。伺服器與來源端都有設定精靈，照著回答問題就能完成。

## 你需要準備

- 一台可以執行 Docker 的**伺服器**，各電腦都能透過區網連到它：
  - Linux：Docker Engine 與 Docker Compose plugin（`docker compose`）。**建議使用 Linux**，原因見[用 Docker Desktop 架伺服器](#用-docker-desktop-架伺服器)。
  - Windows：Docker Desktop（使用 Linux 容器，預設值）。
- 每台要統計用量的電腦都安裝 Docker（Linux 或 Windows + Docker Desktop）。
- 一台 **Windows** 電腦執行管理客戶端。
- Discord Server 的管理權限（用來邀請 Bot）。
- 伺服器的 TCP **33445** port 要讓區網內的電腦連得到（防火牆放行）。不要開放到網際網路。

## 1. 建立 Discord Bot

到 [Discord Developer Portal](https://discord.com/developers/applications) → **New Application** → 左側 **Bot**：

- 按 **Reset Token** → **Copy**，複製 **Bot Token**（等一下伺服器精靈會問）。Token 只顯示一次，忘記的話再按一次 Reset Token。
- **不需要**開啟任何 Privileged Gateway Intents。
- 左側 **OAuth2** → **URL Generator**：Scope 只勾 `bot`，Bot Permissions 勾 **View Channels**、**Send Messages**。
- 用產生的連結把 Bot 邀請到你的 Discord Server。

Bot 只會發報告，不會讀取或回覆頻道裡的訊息。

## 2. 產生管理客戶端的金鑰

在要執行管理客戶端的 Windows 電腦開啟 PowerShell，產生一把**專用**金鑰：

```powershell
ssh-keygen -t ed25519 -f $env:USERPROFILE\.ssh\whatplus_admin -C "whatplus-admin"
```

- 可以設定 passphrase（客戶端連線時會詢問，不會記住）。
- 會產生兩個檔案：`whatplus_admin`（**私鑰**，留在這台電腦，不要給任何人）與 `whatplus_admin.pub`（**公鑰**，等一下要放到伺服器）。
- 建議不要拿平常登入其他主機的金鑰來用。

## 3. 安裝伺服器

在伺服器上取得原始碼並進入 `deploy/server/`：

```bash
git clone https://github.com/Sparxilium-Network-Playground/WHAT-Plus.git
```

```bash
cd WHAT-Plus/deploy/server
```

把步驟 2 的**公鑰**（`whatplus_admin.pub`）放進 `deploy/server/keys/`。例如在 Windows 電腦上執行（換成你的帳號與伺服器位址）：

```powershell
scp $env:USERPROFILE\.ssh\whatplus_admin.pub user@192.168.1.10:~/WHAT-Plus/deploy/server/keys/
```

執行設定精靈：

- Linux：

  ```bash
  sudo ./setup.sh
  ```

- Windows（Docker Desktop）：

  ```powershell
  powershell -ExecutionPolicy Bypass -File .\setup.ps1
  ```

精靈會依序詢問：

| 步驟 | 要輸入的內容 |
|---|---|
| 1. Time zone | 報告使用的時區，例如 `Asia/Taipei`（直接按 Enter 使用預設值）。每日報告的「前一天」、每月報告的「上個月」都依這個時區計算 |
| 2. Discord bot token | 步驟 1 複製的 Token。輸入時畫面不會顯示，也不會留在指令歷史紀錄裡 |
| 3. Building the image | 不用輸入。第一次建置需要幾分鐘，完成後會顯示伺服器的**指紋**（`SHA256:...`） |
| 4. Admin key | 精靈會列出 `keys/` 裡還沒註冊的公鑰，輸入編號選擇管理客戶端的金鑰，再取個名字（預設是公鑰的註解）。還沒放進來的話，放好後按 Enter 重新搜尋 |
| 5. Network allowlist | 允許連線的區網範圍。精靈會依伺服器的 IP 建議一個範圍（例如 `192.168.0.0/16` 代表所有 `192.168.x.x`），按 Enter 接受，或輸入更小的範圍，例如 `192.168.1.0/24` 或單一電腦 `192.168.1.20` |
| 6. Starting the service | 按 Enter 啟動。看到 `OK  The service is running.` 和伺服器指紋就完成了 |

**請記下最後顯示的伺服器指紋**，之後管理客戶端與每台來源端第一次連線時都要比對。忘記的話，再執行一次精靈或開啟[伺服器設定畫面](SERVER-TUI.md)都會顯示。

> 還沒有任何 admin 金鑰時，伺服器會拒絕**所有**連線，也不會發送報告。這是刻意的設計，避免伺服器在沒人管理時被別人接手。

之後要更換 Token、新增管理金鑰或白名單範圍，再執行一次精靈，第一個問題選 **2) Setup**，要保留的項目直接按 Enter。

## 4. 安裝管理客戶端

管理客戶端是單一的 `whatplus-client.exe`。從原始碼建置（需要 [Rust](https://rustup.rs/)）：

```powershell
cargo build --release -p whatplus-client
```

產生的檔案在 `target\release\whatplus-client.exe`，可以複製到任何位置執行。

第一次連線：

1. **連線**頁：填入伺服器的區網位址（例如 `192.168.1.10`），port 維持 `33445`。
2. 在「.ssh 中的金鑰」選 `whatplus_admin`，有 passphrase 的話填入。
3. 按「連線」。畫面會顯示伺服器指紋，與步驟 3 記下的**完全相同**才按「信任並連線」。

接著到 **Discord 頻道**頁新增要收報告的頻道，按「測試」確認 Bot 能發訊息。詳細操作見[管理客戶端使用指南](CLIENT-GUIDE.md)。

## 5. 安裝來源端（每台要統計的電腦）

在每台電腦上取得原始碼並進入 `deploy/agent/`，然後執行設定精靈：

- Linux（用**你自己的帳號**執行，精靈會讀取這個帳號的資料夾）：

  ```bash
  cd WHAT-Plus/deploy/agent && ./setup.sh
  ```

  你的帳號不能直接執行 `docker` 時可以加 `sudo`，精靈仍會讀取執行 sudo 的那個帳號的資料夾。

- Windows（Docker Desktop）：

  ```powershell
  cd WHAT-Plus\deploy\agent; powershell -ExecutionPolicy Bypass -File .\setup.ps1
  ```

精靈會依序詢問：

| 步驟 | 要輸入的內容 |
|---|---|
| 1. Usage folders | Claude Code、Codex、Antigravity 的資料夾，預設是家目錄下的 `.claude`、`.codex`、`.gemini`，按 Enter 接受。資料夾以**唯讀**方式掛載；不存在的工具用量會是 0 |
| 2. Building the image | 不用輸入。第一次建置需要幾分鐘 |
| 3. Server address | 伺服器的區網位址，例如 `192.168.1.10`；port 按 Enter 使用 `33445`。伺服器和來源端在**同一台** Docker Desktop 上時填 `host.docker.internal` |
| 4. Key for this computer | 不用輸入。精靈會產生這台電腦專用的金鑰並顯示**公鑰**（`ssh-ed25519 ...`）。到管理客戶端的**來源機器**頁，把這一行貼上並新增，完成後回到精靈按 Enter |
| 5. Verify the server | 精靈顯示伺服器指紋，與步驟 3 記下的**完全相同**才回答 `y`。接著會測試登入，看到 `Logged in as "..."` 就成功了 |
| 6. Upload interval and start | 上傳間隔（分鐘），按 Enter 使用 10 分鐘。精靈會列出在這台電腦找到的紀錄數，啟動後等待第一次上傳完成 |

第一次會上傳全部的歷史紀錄，可能需要幾分鐘。看到 `OK  upload finished sent=... server_records=...` 就完成了。

之後要修改設定可以再執行一次精靈（選 **2) Setup**），或使用[來源端設定畫面](AGENT-TUI.md)。

## 6. 設定排程

在管理客戶端的**排程**頁新增每日、每週或每月報告，選擇時間、內容與頻道。想先看看效果，可以在**立即發送**頁選擇期間與頻道，按「預覽」或「發送」。

## 用 Docker Desktop 架伺服器

Docker Desktop（Windows／macOS）會轉送連線，伺服器看到的來源位址通常**不是**各電腦真正的 IP，而是 Docker 內部的閘道位址，例如：

- 同一台電腦上的來源端容器：`172.18.0.1` 這類位址（每個 Docker 網路不同）。
- 區網其他電腦：可能也是 Docker 的內部位址。

所以 IP 白名單在 Docker Desktop 上**分不出不同的電腦**，安全性主要靠金鑰。建議用 Linux 主機架伺服器。如果一定要用 Docker Desktop：

1. 讓客戶端或來源端先連一次（會被拒絕）。
2. 再執行一次伺服器精靈並選 **2) Setup**，第 5 步會顯示 `Last refused connection: 172.18.0.1 ...`。回答 `y` 新增，按 Enter 就會加入這個位址。也可以在[伺服器設定畫面](SERVER-TUI.md)的 **System status** 看到 `Last refused`，再到 **Network allowlist** 加入。
3. 只加入實際看到的位址，不要直接開放 `0.0.0.0/0`。

## 更新版本

在伺服器的 `deploy/server/` 取得新版原始碼，再執行設定精靈：

```bash
git pull && sudo ./setup.sh
```

已經安裝過的話，精靈第一步會出現兩個選項：

- **1) Upgrade**（直接按 Enter）：不問任何問題，保留所有設定。它會把 `config/config.conf` 升級到新版格式、修正檔案權限，然後重新建置並重啟服務。
- **2) Setup**：重新走一次時區、Token、管理金鑰與白名單的設定。

精靈**不會**幫你執行 `git pull`，記得先拉新版。想完全不用互動可以直接執行 `sudo ./setup.sh upgrade`（Windows：`.\setup.ps1 upgrade`）。

每台來源端也一樣，在 `deploy/agent/` 執行：

```bash
git pull && ./setup.sh upgrade
```

資料庫會在服務啟動時自動升級，既有資料都會保留。`config/config.conf` 與資料庫都有版本號；如果它們是由更新的版本寫入的（例如更新後又退回舊版），服務會拒絕啟動並說明原因。

管理客戶端重新建置 `whatplus-client.exe` 即可，設定存在 `%APPDATA%\WHATPlus\config\client.json`，不受影響。

## 常見問題

**客戶端或來源端連不上（`the server closed the connection`、connection reset）**
你的 IP 不在伺服器的白名單內，或伺服器還沒有 admin 金鑰。到[伺服器設定畫面](SERVER-TUI.md)的 **System status** 看 `Last refused` 顯示的位址，確認後加入 **Network allowlist**。用 Docker Desktop 時請看[上面的說明](#用-docker-desktop-架伺服器)。

**連線逾時（`timed out`）**
伺服器沒有啟動，或防火牆擋住了 33445 port。在伺服器上執行 `docker compose ps` 確認 `server` 是 `running`。

**「伺服器拒絕這把金鑰」／來源端精靈顯示 `The server rejected this key`**
這把公鑰還沒註冊、被停用了，或設定了只允許特定 IP。管理客戶端用的 admin 金鑰在伺服器精靈或伺服器設定畫面註冊；來源端的金鑰在管理客戶端的**來源機器**頁註冊。

**指紋改變的警告（`fingerprint CHANGED`）**
伺服器重新安裝後（`data/` 被清除）會產生新的指紋。確認是自己重裝的，才在客戶端的連線頁按「忘記」、在來源端精靈重新輸入伺服器位址，然後重新比對。否則**不要連線**，可能有人在攔截。

**某個工具的用量是 0**
來源端精靈第 6 步會列出每個工具找到的紀錄數與資料夾。顯示 `-` 或 0 時，確認第 1 步的資料夾路徑正確；也可以在[來源端設定畫面](AGENT-TUI.md)選 **Show local usage sources** 檢查。

**Discord 狀態顯示 `ERROR` 或 `Discord rejected the bot token`**
Token 錯誤或已被重設。在 `deploy/server/` 重新執行精靈、選 **2) Setup**，在第 2 步更換 Token。

**頻道收不到報告**
在管理客戶端的 **Discord 頻道**頁按「測試」。失敗通常是 Bot 不在那個 Server，或在頻道沒有 View Channels／Send Messages 權限。

**伺服器停機時錯過的排程**
停機 6 小時內恢復的會補送；超過 6 小時的不會補送，避免一次湧入很多舊報告。

**`docker compose logs` 要在哪裡執行**
在 `deploy/server/`（伺服器）或 `deploy/agent/`（來源端）目錄執行，例如 `docker compose logs --tail=40 server`。
