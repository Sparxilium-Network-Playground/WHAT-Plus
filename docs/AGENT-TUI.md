# 來源端設定畫面使用說明

> 最後修改：2026-11-13

每台來源端電腦上的文字設定畫面，用來查看上傳狀態、修改伺服器位址或金鑰、立即上傳，以及檢查本機找到哪些用量。介面是英文的，這份說明逐一解釋每個項目。

第一次安裝用[設定精靈](INSTALL.md#5-安裝來源端每台要統計的電腦)就能完成所有設定；這個畫面主要用在之後的檢查與調整。

## 開啟設定畫面

在這台電腦的 `deploy/agent/` 目錄執行：

```bash
docker compose exec agent whatplus-agent admin
```

設定存在 `deploy/agent/data/agent.json`。來源端每 30 秒讀一次設定，修改後**不需要重新啟動**。

## 通用按鍵

| 按鍵 | 作用 |
|---|---|
| `Up` / `Down` | 移動游標 |
| `Enter` | 選擇、確認輸入 |
| `Esc` | 返回、取消輸入；在主選單是離開 |
| `y` | 回答確認問題（其他任何鍵代表「否」） |

## 主選單

```
 Server:       192.168.1.10:33445
 Fingerprint:  SHA256:htoOzLsLVKq97YfFvMP3YD4jr0zeag4VQmZyirB7nlY
 Private key:  /keys/whatplus_agent_ed25519
 Interval:     every 10 minutes
 Last upload:  2026-11-13T11:30:12Z OK: sent 312 records (server has 15612)

> Server address
  Private key
  Verify server and test login
  Upload interval
  Upload now
  Re-send all records
  Show local usage sources
  Exit
```

上方是目前的狀態：

| 項目 | 說明 |
|---|---|
| `Server` | 伺服器位址與 port |
| `Fingerprint` | 已確認的伺服器指紋。顯示 `not verified` 時不會上傳 |
| `Private key` | 這台電腦使用的私鑰（容器內的路徑；`/keys` 就是 `deploy/agent/keys/`） |
| `Interval` | 上傳間隔 |
| `Last upload` | 最近一次上傳的時間與結果。`FAILED:` 後面是失敗原因 |

伺服器位址、私鑰、已確認的指紋**缺一項就不會上傳**，畫面會顯示 `! Not uploading yet. Missing: ...`。

## Server address：伺服器位址

輸入伺服器的區網位址（例如 `192.168.1.10`）與 port（預設 `33445`）。伺服器和來源端在同一台 Docker Desktop 上時，填 `host.docker.internal`。

**換了位址或 port，已確認的指紋會被清除**，必須再執行一次 **Verify server and test login**。

## Private key：私鑰

選擇這台電腦連線用的私鑰。

1. **Directory**：要掃描的目錄，預設 `/keys`（即 `deploy/agent/keys/`），直接按 Enter。
2. 選擇清單中的私鑰，或選最後一項 **Generate a new key in this directory** 產生新的金鑰。
   - 產生時輸入檔名（預設 `whatplus_agent_ed25519`）與註解（例如這台電腦的名稱）。
   - 只能使用**沒有 passphrase** 的 `ssh-ed25519` 金鑰，因為來源端在背景自動執行，無法輸入 passphrase。標示 `(has a passphrase - not usable here)` 或 `(unsupported type)` 的不能使用。
3. 選好後畫面會顯示這把金鑰的**公鑰**。到管理客戶端的**來源機器**頁貼上並新增，然後執行 **Verify server and test login**。

建議每台電腦用一把專用金鑰，不要拿平常登入其他主機的金鑰。這把金鑰只能上傳用量，不能修改伺服器設定。

## Verify server and test login：驗證伺服器並測試登入

連線到伺服器並測試登入。

- **第一次連線**：畫面顯示伺服器指紋，與伺服器設定精靈或[伺服器設定畫面](SERVER-TUI.md)主選單上方顯示的指紋**完全相同**才按 `y`。指紋會被記住，之後每次連線都會比對。
- 結果：

| 訊息 | 意思與處理方式 |
|---|---|
| `OK. Logged in as "..."` | 成功，之後會自動上傳 |
| `The server rejected this key` | 這把公鑰還沒註冊（或被停用、IP 不在允許範圍）。畫面會顯示公鑰，到管理客戶端的**來源機器**頁新增 |
| `This key is registered as an ADMIN key` | 這把是管理用的金鑰，請改用另一把 agent 金鑰 |
| `WARNING: the server fingerprint changed` | 伺服器的指紋和記住的不同。確認是自己重裝了伺服器，才重新設定 **Server address**（會清除舊指紋）再驗證；否則**不要連線** |
| `Connection failed: ...` | 連不到伺服器。`the server closed the connection` 通常是 IP 不在白名單內，見[常見問題](INSTALL.md#常見問題) |

## Upload interval：上傳間隔

每隔幾分鐘上傳一次，1–1440 分鐘，預設 10 分鐘。

## Upload now：立即上傳

馬上讀取本機用量並上傳，不必等下一次排程，完成後顯示送出的筆數與伺服器上的總筆數。

上傳時會從伺服器最新一筆紀錄往前 24 小時起重送，伺服器會自動略過重複的紀錄，所以重送不會讓費用重複計算。

## Re-send all records：全部重送

把這台電腦**所有**的紀錄重新送一次。伺服器會略過重複的紀錄，總數不會改變。適合在伺服器的資料庫被重建之後使用；平常不需要。

## Show local usage sources：檢查本機用量

列出每個工具在這台電腦找到的紀錄數與資料夾：

```
 Claude Code: 5489 records
   /home/whatplus/.claude/projects

 Codex: 3137 records
   /home/whatplus/.codex/sessions

 Antigravity: 7007 records
   /tmp/whatplus-antigravity/admin/.gemini_antigravity/conversations
   ...
```

- 顯示 `no data directory found (check the volume mounts)`：找不到資料夾，重新執行設定精靈，在第 1 步確認資料夾路徑。
- Antigravity 的路徑在 `/tmp/whatplus-antigravity/` 底下是正常的：Antigravity 把資料存在 SQLite 資料庫，來源端會先複製一份到容器內再讀取，以免讀到不完整的資料。
- 有 `error:` 時是讀取失敗的原因。
