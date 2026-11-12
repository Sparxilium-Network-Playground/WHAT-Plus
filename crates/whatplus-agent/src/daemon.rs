//! `run` 模式：設定完成後依間隔上傳；尚未設定時只等待（不連線）。

use std::time::Duration;

use crate::config::{AgentState, Paths};
use crate::upload;

/// 每次檢查設定的間隔；設定在 TUI 改了之後最晚這麼久會生效。
const CHECK: Duration = Duration::from_secs(30);

pub async fn run(paths: Paths) {
    let mut waited = Duration::MAX; // 啟動時立刻上傳一次
    let mut warned = false;
    loop {
        let config = match paths.load_config() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("{e:#}");
                tokio::time::sleep(CHECK).await;
                continue;
            }
        };
        let missing = config.missing();
        if !missing.is_empty() {
            if !warned {
                tracing::warn!(
                    "waiting for configuration ({}); run the admin TUI: docker compose exec agent whatplus-agent admin",
                    missing.join(", ")
                );
                warned = true;
            }
            tokio::time::sleep(CHECK).await;
            continue;
        }
        warned = false;

        let interval = Duration::from_secs(u64::from(config.interval()) * 60);
        if waited >= interval {
            waited = Duration::ZERO;
            let mut state = paths.load_state();
            state.last_attempt_at = Some(now());
            match upload::run_once(&config, false).await {
                Ok(s) => {
                    tracing::info!(sent = s.sent, server_records = s.server_records, "upload finished");
                    state.last_success_at = state.last_attempt_at.clone();
                    state.last_result = Some(format!("OK: sent {} records (server has {})", s.sent, s.server_records));
                }
                Err(e) => {
                    tracing::warn!("upload failed: {e:#}");
                    state.last_result = Some(format!("FAILED: {e:#}"));
                }
            }
            save(&paths, &state);
        }
        tokio::time::sleep(CHECK).await;
        waited = waited.saturating_add(CHECK);
    }
}

fn save(paths: &Paths, state: &AgentState) {
    if let Err(e) = paths.save_state(state) {
        tracing::warn!("{e:#}");
    }
}

pub fn now() -> String {
    let t = jiff::Timestamp::now();
    t.round(jiff::Unit::Second).unwrap_or(t).to_string()
}
