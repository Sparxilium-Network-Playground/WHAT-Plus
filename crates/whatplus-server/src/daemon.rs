//! `run` 模式：SSH 服務、驗證設定輪詢、排程、Discord、重啟監看。

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use crate::app::App;
use crate::auth::{AuthHolder, AuthSnapshot};
use crate::config::Settings;
use crate::db::{Store, now_rfc3339, runtime};
use crate::discord::Discord;
use crate::{hostkey, scheduler, ssh};

pub async fn run(settings: Settings) -> Result<()> {
    let store = Arc::new(Store::open(&settings.database)?);
    let host_key = hostkey::load_or_create(&settings.host_key)?;
    let host_fingerprint = hostkey::fingerprint(&host_key);
    store.runtime_set(runtime::HOST_FINGERPRINT, &host_fingerprint)?;
    store.runtime_set(runtime::DAEMON_STARTED_AT, &now_rfc3339())?;
    tracing::info!("host key fingerprint: {host_fingerprint}");

    let auth = Arc::new(AuthHolder::new(AuthSnapshot::load(&store)?));
    if !auth.current().has_admin() {
        tracing::warn!(
            "no admin key is registered yet; the service refuses all connections until one is added in the admin TUI"
        );
    }

    let token = settings.discord_token();
    let token_for_identify = token.as_ref().ok().cloned();
    let discord = Discord::new(token, store.clone());
    let interval = Duration::from_millis(settings.reload_interval_ms.max(1000));
    let app = Arc::new(App {
        store: store.clone(),
        auth: auth.clone(),
        discord,
        settings,
        host_fingerprint,
    });

    tokio::spawn(auth.poll(store.clone(), interval));
    tokio::spawn(scheduler::run(app.clone()));
    {
        let app = app.clone();
        tokio::spawn(async move { app.discord.ensure_identified(token_for_identify).await });
    }
    tokio::spawn(watch_restart(store.clone(), interval));

    tokio::select! {
        r = ssh::serve(app.clone(), host_key) => r,
        _ = shutdown_signal() => {
            tracing::info!("shutting down");
            Ok(())
        }
    }
}

/// TUI 的 Restart Service 會寫入新的 token；token 改變就結束 process，由 Docker 重新啟動。
async fn watch_restart(store: Arc<Store>, interval: Duration) {
    let initial = store
        .runtime_get(runtime::DAEMON_RESTART_REQUEST)
        .ok()
        .flatten();
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        let _ = store.runtime_set(runtime::DAEMON_HEARTBEAT, &now_rfc3339());
        let current = store
            .runtime_get(runtime::DAEMON_RESTART_REQUEST)
            .ok()
            .flatten();
        if current != initial {
            tracing::info!("restart requested from the admin TUI; exiting");
            std::process::exit(0);
        }
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
