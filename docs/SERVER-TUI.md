# 伺服器設定畫面使用說明

> 最後修改：2026-11-13

伺服器上的文字設定畫面，用來管理**誰可以連線**：已註冊的金鑰、從檔案匯入金鑰、IP 白名單，以及查看系統狀態。介面是英文的，這份說明逐一解釋每個畫面。

報告排程、Discord 頻道、來源機器等日常設定都在[管理客戶端](CLIENT-GUIDE.md)裡完成。第一次安裝用[設定精靈](INSTALL.md#3-安裝伺服器)就能完成大部分設定，這個畫面主要用在之後的管理。

## 開啟設定畫面

SSH 登入伺服器，在 `deploy/server/` 目錄執行：

```bash
docker compose exec server whatplus-server admin
```

服務沒有在執行時（例如還沒啟動過），改用：

```bash
docker compose run --rm server admin
```

設定一儲存就寫進資料庫，正在執行的服務約 5 秒內自動套用，**不需要重新啟動**。

## 通用按鍵

| 按鍵 | 作用 |
|---|---|
| `Up` / `Down` | 移動游標（`>` 標示目前位置） |
| `Enter` | 選擇、進入下一層、確認輸入 |
| `Space` | 勾選或取消 `[ ]` / `[*]` |
| `Esc` | 返回上一層、取消輸入；在主選單是離開 |
| `y` | 回答確認問題（其他任何鍵代表「否」） |

每個畫面最下方都會列出這個畫面能用的按鍵。

## 主選單

```
 Host key fingerprint: SHA256:htoOzLsLVKq97YfFvMP3YD4jr0zeag4VQmZyirB7nlY

 ! No admin key is registered. The service refuses all connections until
   you import one (Import keys) and give it the admin role.

> Authorized keys
  Import keys from a directory
  Network allowlist
  System status
  Restart service
  Exit
```

- **Host key fingerprint**：伺服器的指紋。管理客戶端與來源端第一次連線時會顯示指紋，兩邊**完全相同**才可以信任。
- 上方的 `!` 警告代表目前**不會接受任何連線**：
  - `No admin key is registered`：還沒有啟用中的 admin 金鑰。
  - `The network allowlist is empty`：白名單是空的。

## 先搞懂：連線什麼時候會被接受

一條連線要**依序**通過下面的檢查，任何一項不符合就直接斷線：

1. 伺服器至少有一把**啟用中的 admin 金鑰**。
2. 對方的 IP 在 **Network allowlist** 的範圍內（在 SSH 握手前就檢查）。
3. 對方使用的金鑰在 **Authorized keys** 裡，而且是啟用的。
4. 這把金鑰有設定 IP 範圍的話，對方的 IP 也要在那個範圍內。

通過之後，依金鑰的**角色**決定能做什麼：

| 角色 | 給誰用 | 可以做的事 |
|---|---|---|
| `admin` | 管理客戶端 | 所有設定：頻道、排程、手動發送、新增或停用 agent 金鑰 |
| `agent` | 來源端 | 只能上傳用量紀錄 |

admin 金鑰**只能**在這個畫面或設定精靈新增，管理客戶端不能新增 admin。

## Authorized keys：已註冊的金鑰

```
 [*] = enabled. Admin keys can change everything from the client app;
 agent keys can only upload usage records.

> [*] admin test-admin           SHA256:MHP6/...  [any allowed IP]  last seen 2026-11-13T11:20:03Z
  [*] agent office-pc            SHA256:sWnt3...  [any allowed IP]  last seen 2026-11-13T11:30:12Z
  [ ] agent old-laptop           SHA256:Ayt7R...  [192.168.1.20/32]  last seen never
```

每一列依序是：是否啟用、角色、名稱、指紋、允許的 IP（`any allowed IP` 表示套用全域白名單）、最後一次連線的時間。

- `Space`：啟用或停用這把金鑰。停用後立刻不能連線，但資料都保留。
- `Enter`：進入編輯，畫面上方會顯示完整資訊（公鑰、上傳的紀錄數、最後連線的 IP），可以選擇：

| 選項 | 作用 |
|---|---|
| `Rename` | 改名稱。來源端的名稱會出現在報告的「各機器」 |
| `Change role to ...` | 在 admin 與 agent 之間切換 |
| `Restrict to IP ranges` | 限制這把金鑰只能從哪些位址連線，用逗號分隔，例如 `192.168.1.20/32`。留空表示套用全域白名單 |
| `Disable` / `Enable` | 同 `Space` |
| `Delete` | 刪除金鑰，**連同這台機器上傳的所有用量紀錄**，無法復原。只想暫停的話請用 Disable |

**最後一把啟用中的 admin 金鑰**受到保護：不能刪除、不能改成 agent；要停用時會再確認一次，因為停用後伺服器會拒絕所有連線並停止發送報告。要換掉它，請先加入新的 admin 金鑰。

## Import keys from a directory：匯入金鑰

從目錄中的公鑰檔加入金鑰。

1. **Directory**：要掃描的目錄，預設是 `/keys`，也就是伺服器 `deploy/server/keys/` 資料夾。直接按 Enter。
2. 畫面會列出目錄中所有 `*.pub` 與 `authorized_keys*` 檔案裡的**每一把**公鑰。一個檔案有多行時逐行列出：

   ```
    Keys in /keys:

   > [ ] admin.pub:1  SHA256:MHP6/...  whatplus-admin
     [ ] team.pub:1   SHA256:sWnt3...  office-pc
     [ ] team.pub:2   SHA256:Ayt7R...  laptop
     (registered) old.pub:1  SHA256:...  already-added
     (skip) id_rsa.pub:1  unsupported ssh-rsa (user@host)
   ```

   - `(registered)`：已經註冊過。
   - `(skip)`：不支援（只支援 `ssh-ed25519`）或無法讀取。
3. 用 `Space` 勾選要加入的金鑰（可以一次勾多把），按 `Enter` 繼續。
4. 選擇角色：`agent`（來源端）或 `admin`（管理客戶端）。同一次勾選的金鑰都會是這個角色；要加入不同角色，就分兩次匯入。
5. 逐把輸入名稱，預設是公鑰的註解。

公鑰檔可以用 `scp` 複製到伺服器的 `deploy/server/keys/`。來源端的金鑰也可以直接在管理客戶端的**來源機器**頁貼上，不必經過這裡。

## Network allowlist：IP 白名單

只有這些範圍內的位址可以連線，其他連線在 SSH 握手前就被關閉。**清單是空的時候拒絕所有連線。**

- `A`：新增範圍，例如：
  - `192.168.0.0/16`：所有 `192.168.x.x`
  - `192.168.1.0/24`：所有 `192.168.1.x`
  - `10.0.0.5`：單一電腦
- `D`（或 `Delete`）：移除游標所在的範圍。

輸入 `0.0.0.0/0` 代表允許**所有**位址，會再確認一次。除非你很清楚自己在做什麼，否則不要這樣設定。

用 Docker Desktop 架伺服器時，伺服器看到的往往是 Docker 內部的閘道位址，見 [System status](#system-status系統狀態) 的 `Last refused`，以及[安裝說明](INSTALL.md#用-docker-desktop-架伺服器)。

## System status：系統狀態

| 項目 | 看什麼 |
|---|---|
| `Listening` | SSH 服務監聽的位址與 port（容器內） |
| `Host fingerprint` | 伺服器指紋 |
| `Time zone` | 報告使用的時區 |
| `Service started` | 服務最後一次啟動的時間，可用來確認重啟是否完成 |
| `Last heartbeat` | 服務最後一次回報的時間。很久沒更新代表服務沒在執行 |
| `Discord` | Bot 的連線狀態。顯示 `ERROR` 時 Token 有問題，見[常見問題](INSTALL.md#常見問題) |
| `Allowlist` | 目前的白名單 |
| `Last refused` | **最近一次被白名單擋掉的位址與時間**。電腦連不上時先看這裡：如果是你的電腦（或 Docker 的閘道位址），把它加入 Network allowlist |
| `Usage records` | 伺服器上的用量紀錄總數 |
| `Last report` | 最近一次發送報告的時間與結果 |
| `Keys` | 每把金鑰的角色、名稱、啟用狀態、紀錄數、最後連線時間與 IP |
| `Schedules` | 每個排程最近一次執行的時間與結果（排程在管理客戶端編輯） |

這個畫面不會自動更新，按任意鍵返回後再進來一次即可。

## Restart service：重啟服務

確認後服務會正常關閉，再由 Docker 自動啟動（`docker-compose.yml` 的 `restart: unless-stopped`），大約需要 10–20 秒。

- 容器重啟時，**這個設定畫面也會一起關閉**（它在同一個容器裡執行），等一下再重新開啟，到 **System status** 看 `Service started` 是否更新。
- 在這個畫面改的設定約 5 秒內就生效，**不需要重啟**。重啟適合用在 Discord 連線異常、更換了 `secrets/discord-token` 之後。
- 重啟**不會**套用新版程式、`docker-compose.yml` 或 `config/config.conf` 的變更，這些請重新執行設定精靈（選 **1) Upgrade**）。
