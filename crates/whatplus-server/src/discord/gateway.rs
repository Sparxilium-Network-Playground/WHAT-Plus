//! Discord 規定 Bot 至少要連過一次 Gateway 才能用 REST 發訊息。
//! 這裡只做一次 Identify（intents = 0，不收任何事件），收到 READY 就關閉連線。

use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use tungstenite::Message;

const GATEWAY: &str = "wss://gateway.discord.gg/?v=10&encoding=json";
const TIMEOUT: Duration = Duration::from_secs(20);

pub fn identify_once(token: &str) -> Result<()> {
    let (mut socket, _) =
        tungstenite::connect(GATEWAY).context("cannot connect to the Discord gateway")?;
    if let tungstenite::stream::MaybeTlsStream::Rustls(s) = socket.get_mut() {
        let _ = s.get_mut().set_read_timeout(Some(TIMEOUT));
    }
    let started = Instant::now();
    let mut identified = false;
    while started.elapsed() < TIMEOUT {
        let msg = socket.read().map_err(|e| anyhow!("Discord gateway: {e}"))?;
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(frame) => {
                let code = frame.map(|f| u16::from(f.code)).unwrap_or(0);
                if code == 4004 {
                    bail!("Discord rejected the bot token (gateway close 4004)");
                }
                bail!("Discord gateway closed the connection ({code})");
            }
            _ => continue,
        };
        let payload: Value = serde_json::from_str(&text)?;
        match payload.get("op").and_then(Value::as_u64) {
            Some(10) if !identified => {
                let identify = json!({
                    "op": 2,
                    "d": {
                        "token": token,
                        "intents": 0,
                        "properties": { "os": "linux", "browser": "whatplus", "device": "whatplus" }
                    }
                });
                socket
                    .send(Message::text(identify.to_string()))
                    .map_err(|e| anyhow!("Discord gateway: {e}"))?;
                identified = true;
            }
            Some(0) if payload.get("t").and_then(Value::as_str) == Some("READY") => {
                let _ = socket.close(None);
                return Ok(());
            }
            Some(9) => bail!("Discord gateway rejected the session"),
            _ => {}
        }
    }
    bail!("timed out waiting for the Discord gateway")
}
