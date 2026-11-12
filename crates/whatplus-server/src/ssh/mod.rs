//! SSH 服務：自己接受 TCP 連線，先檢查 IP 白名單，通過才交給 russh 握手。

mod handler;
mod session;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use russh::MethodKind;
use russh::server::Config;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use whatplus_ssh::PrivateKey;

use crate::app::App;
use handler::ConnHandler;

/// 同時連線數上限。
const MAX_CONNECTIONS: usize = 32;

fn config(host_key: PrivateKey) -> Config {
    Config {
        methods: (&[MethodKind::PublicKey][..]).into(),
        keys: vec![host_key],
        auth_rejection_time: Duration::from_secs(1),
        auth_rejection_time_initial: Some(Duration::from_millis(50)),
        max_auth_attempts: 3,
        inactivity_timeout: Some(Duration::from_secs(900)),
        keepalive_interval: Some(Duration::from_secs(60)),
        nodelay: true,
        ..Default::default()
    }
}

pub async fn serve(app: Arc<App>, host_key: PrivateKey) -> Result<()> {
    let addr = format!("{}:{}", app.settings.listen, app.settings.port);
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("cannot listen on {addr}"))?;
    tracing::info!("SSH service listening on {addr}");
    serve_on(listener, app, host_key).await
}

pub async fn serve_on(listener: TcpListener, app: Arc<App>, host_key: PrivateKey) -> Result<()> {
    let config = Arc::new(config(host_key));
    let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    let mut warned_no_admin = false;

    loop {
        let (socket, peer) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("accept failed: {e}");
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
        };
        let auth = app.auth.current();
        if !auth.has_admin() {
            if !warned_no_admin {
                tracing::warn!("no admin key is registered; refusing all connections (add one in the admin TUI)");
                warned_no_admin = true;
            }
            continue; // socket 在這裡被 drop，連線直接關閉
        }
        warned_no_admin = false;
        if !auth.ip_allowed(peer.ip()) {
            tracing::warn!(%peer, "connection from an address outside the allowlist was refused");
            continue;
        }
        let Ok(permit) = slots.clone().try_acquire_owned() else {
            tracing::warn!(%peer, "too many connections; refused");
            continue;
        };
        let _ = socket.set_nodelay(true);
        let config = config.clone();
        let handler = ConnHandler::new(app.clone(), Some(peer));
        tokio::spawn(async move {
            let _permit = permit;
            match russh::server::run_stream(config, socket, handler).await {
                Ok(running) => {
                    if let Err(e) = running.await {
                        tracing::debug!(%peer, "connection ended: {e:#}");
                    }
                }
                Err(e) => tracing::debug!(%peer, "handshake failed: {e:#}"),
            }
        });
    }
}

#[cfg(test)]
mod tests;
