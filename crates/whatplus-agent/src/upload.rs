//! 上傳：連線 → 詢問伺服器最新一筆的時間 → 送出之後（含重疊區間）的紀錄。
//!
//! 伺服器以紀錄內容去重，所以重疊區間重送不會重複計算；伺服器資料庫重建時會自動全部重送。

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use jiff::{SignedDuration, Timestamp};
use whatplus_proto::{MAX_UPLOAD_BATCH, Request, Response};
use whatplus_ssh::client::{ConnectOptions, Connection, unexpected};
use whatplus_ssh::keys::{self, KeyError};

use crate::collect;
use crate::config::AgentConfig;

/// 從伺服器最新一筆往前重送的範圍：涵蓋晚寫入的日誌（例如同步較慢的來源）。
const OVERLAP: SignedDuration = SignedDuration::from_hours(24);

#[derive(Debug, Clone)]
pub struct UploadSummary {
    pub sent: usize,
    pub server_records: u64,
}

pub fn load_key(config: &AgentConfig) -> Result<Arc<whatplus_ssh::PrivateKey>> {
    let path = config
        .key_path
        .as_ref()
        .context("no private key selected")?;
    match keys::load_private_key(path, None) {
        Ok(k) => Ok(Arc::new(k)),
        Err(KeyError::NeedsPassphrase) => bail!(
            "{} is protected by a passphrase; the agent runs unattended and needs a key without one",
            path.display()
        ),
        Err(e) => Err(e.into()),
    }
}

pub fn options(
    config: &AgentConfig,
    key: Arc<whatplus_ssh::PrivateKey>,
    known: Option<String>,
) -> ConnectOptions {
    ConnectOptions {
        host: config.server_host.trim().to_string(),
        port: config.server_port,
        key,
        known_fingerprint: known,
        client_name: format!("whatplus-agent/{}", env!("CARGO_PKG_VERSION")),
    }
}

/// `full = true` 時忽略伺服器狀態，全部重送。
pub async fn run_once(config: &AgentConfig, full: bool) -> Result<UploadSummary> {
    let missing = config.missing();
    if !missing.is_empty() {
        bail!("not configured: {}", missing.join(", "));
    }
    let key = load_key(config)?;
    let mut conn =
        Connection::connect(options(config, key, config.server_fingerprint.clone())).await?;

    let latest = match conn.request(Request::UploadState).await? {
        Response::UploadState { latest, .. } => latest,
        other => return Err(unexpected(other).into()),
    };
    let since: Option<Timestamp> = if full {
        None
    } else {
        latest.and_then(|t| t.checked_sub(OVERLAP).ok())
    };

    let (records, summaries) = tokio::task::spawn_blocking(collect::load_all).await?;
    for s in summaries.iter().filter(|s| s.error.is_some()) {
        tracing::warn!(source = %s.name, "reading usage failed: {}", s.error.as_deref().unwrap_or(""));
    }
    let records = collect::since(records, since);
    let sent = records.len();
    for chunk in records.chunks(MAX_UPLOAD_BATCH) {
        match conn
            .request(Request::Upload {
                records: chunk.to_vec(),
            })
            .await?
        {
            Response::Uploaded { .. } => {}
            other => return Err(unexpected(other).into()),
        }
    }
    let server_records = match conn.request(Request::UploadState).await? {
        Response::UploadState { records, .. } => records,
        _ => 0,
    };
    conn.close().await;
    Ok(UploadSummary {
        sent,
        server_records,
    })
}
