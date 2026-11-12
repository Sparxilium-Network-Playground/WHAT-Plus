//! 連線頁：選私鑰、連線、第一次連線確認伺服器指紋（TOFU）。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use slint::ComponentHandle;
use whatplus_ssh::client::{ConnectError, ConnectOptions};
use whatplus_ssh::keys::{self, KeyError};

use crate::convert::strings;
use crate::ctx::Ctx;
use crate::settings::{default_ssh_dir, host_key};
use crate::{AppWindow, ConnData, Nav, Page, StatusBar};

/// 等待使用者確認指紋的連線參數。
type Pending = Arc<Mutex<Option<ConnectOptions>>>;

pub fn init(ctx: &Ctx, ui: &AppWindow) {
    let data = ui.global::<ConnData>();
    let s = ctx.settings();
    data.set_host(s.host.clone().into());
    data.set_port(s.port.to_string().into());
    data.set_key_path(s.key_path.clone().into());
    refresh_known(ctx, ui);

    let found = default_ssh_dir()
        .and_then(|d| keys::scan_private_keys(&d).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|k| k.supported)
        .collect::<Vec<_>>();
    // 第 0 項是「其他檔案」，代表手動輸入或瀏覽選擇的路徑。
    let mut labels = vec![ctx.t("(other file)", "（其他檔案）")];
    labels.extend(found.iter().map(|k| {
        let name = k.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        format!("{name}  {}{}", k.comment, if k.encrypted { "  🔒" } else { "" })
    }));
    data.set_key_files(strings(labels));
    let paths: Vec<PathBuf> = found.into_iter().map(|k| k.path).collect();
    let current = paths.iter().position(|p| p.display().to_string() == s.key_path);
    data.set_key_index(current.map(|i| i as i32 + 1).unwrap_or(0));
    {
        let ui_weak = ui.as_weak();
        data.on_pick_key(move |i| {
            let Some(idx) = (i as usize).checked_sub(1) else { return };
            if let (Some(ui), Some(p)) = (ui_weak.upgrade(), paths.get(idx)) {
                ui.global::<ConnData>().set_key_path(p.display().to_string().into());
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        data.on_browse(move || {
            let mut dialog = rfd::FileDialog::new();
            if let Some(dir) = default_ssh_dir() {
                dialog = dialog.set_directory(dir);
            }
            if let (Some(ui), Some(path)) = (ui_weak.upgrade(), dialog.pick_file()) {
                ui.global::<ConnData>().set_key_path(path.display().to_string().into());
            }
        });
    }

    let pending: Pending = Arc::new(Mutex::new(None));
    {
        let ctx = ctx.clone();
        let pending = pending.clone();
        data.on_connect(move || connect(&ctx, &pending));
    }
    {
        let ctx = ctx.clone();
        let pending = pending.clone();
        data.on_trust(move || trust(&ctx, &pending));
    }
    {
        let ctx = ctx.clone();
        data.on_reject(move || {
            pending.lock().unwrap().take();
            if let Some(ui) = ctx.ui.upgrade() {
                ui.global::<ConnData>().set_pending_fingerprint("".into());
            }
        });
    }
    {
        let ctx = ctx.clone();
        data.on_forget(move || {
            let Some(ui) = ctx.ui.upgrade() else { return };
            let (host, port) = address(&ui);
            ctx.update_settings(|s| {
                s.known_hosts.remove(&host_key(&host, port));
            });
            refresh_known(&ctx, &ui);
        });
    }
    {
        let ctx = ctx.clone();
        data.on_disconnect(move || {
            let ctx2 = ctx.clone();
            ctx.backend.spawn(ctx.backend.disconnect(), move |_| {
                if let Some(ui) = ctx2.ui.upgrade() {
                    ui.global::<StatusBar>().set_connected(false);
                    ui.global::<Nav>().set_page(Page::Connection);
                }
                ctx2.info(ctx2.t("Disconnected.", "已中斷連線。"));
            });
        });
    }
}

fn address(ui: &AppWindow) -> (String, u16) {
    let data = ui.global::<ConnData>();
    let host = data.get_host().trim().to_string();
    let port = data.get_port().trim().parse().unwrap_or(0);
    (host, port)
}

fn refresh_known(ctx: &Ctx, ui: &AppWindow) {
    let (host, port) = address(ui);
    let known = ctx.settings().known_hosts.get(&host_key(&host, port)).cloned().unwrap_or_default();
    ui.global::<ConnData>().set_known_fingerprint(known.into());
}

fn connect(ctx: &Ctx, pending: &Pending) {
    let Some(ui) = ctx.ui.upgrade() else { return };
    let data = ui.global::<ConnData>();
    let (host, port) = address(&ui);
    if host.is_empty() || port == 0 {
        ctx.error(ctx.t("Enter the server address and a valid port.", "請輸入伺服器位址與正確的 port。"));
        return;
    }
    let key_path = data.get_key_path().trim().to_string();
    let passphrase = data.get_passphrase().to_string();
    let key = match keys::load_private_key(key_path.as_ref(), Some(passphrase.as_str()).filter(|p| !p.is_empty())) {
        Ok(k) => Arc::new(k),
        Err(KeyError::NeedsPassphrase) => {
            ctx.error(ctx.t("This key has a passphrase; enter it below.", "這把金鑰有 passphrase，請在下方輸入。"));
            return;
        }
        Err(e) => {
            ctx.error(format!("{}: {e}", ctx.t("Cannot use this key", "無法使用這把金鑰")));
            return;
        }
    };
    data.set_key_info(format!("{}  {}", keys::fingerprint(key.public_key()), keys::to_line(key.public_key())).into());
    ctx.update_settings(|s| {
        s.host = host.clone();
        s.port = port;
        s.key_path = key_path.clone();
    });
    refresh_known(ctx, &ui);
    let known = ctx.settings().known_hosts.get(&host_key(&host, port)).cloned();
    let opts = ConnectOptions {
        host,
        port,
        key,
        known_fingerprint: known,
        client_name: format!("whatplus-client/{}", env!("CARGO_PKG_VERSION")),
    };
    start(ctx, pending, opts);
}

fn trust(ctx: &Ctx, pending: &Pending) {
    let Some(ui) = ctx.ui.upgrade() else { return };
    let fp = ui.global::<ConnData>().get_pending_fingerprint().to_string();
    ui.global::<ConnData>().set_pending_fingerprint("".into());
    let Some(mut opts) = pending.lock().unwrap().take() else { return };
    ctx.update_settings(|s| {
        s.known_hosts.insert(host_key(&opts.host, opts.port), fp.clone());
    });
    refresh_known(ctx, &ui);
    opts.known_fingerprint = Some(fp);
    start(ctx, pending, opts);
}

fn start(ctx: &Ctx, pending: &Pending, opts: ConnectOptions) {
    ctx.info(ctx.t("Connecting…", "連線中…"));
    if let Some(ui) = ctx.ui.upgrade() {
        ui.global::<StatusBar>().set_busy(true);
    }
    let ctx2 = ctx.clone();
    let pending = pending.clone();
    let public_line = keys::to_line(opts.key.public_key());
    let host = opts.host.clone();
    let fut = ctx.backend.connect(opts.clone());
    ctx.backend.spawn(fut, move |result| {
        let Some(ui) = ctx2.ui.upgrade() else { return };
        let bar = ui.global::<StatusBar>();
        bar.set_busy(false);
        match result {
            Ok(info) => {
                bar.set_connected(true);
                bar.set_connected_as(format!("{} @ {host}", info.name).into());
                ui.global::<ConnData>().set_passphrase("".into());
                ctx2.info(format!("{} {} (server {})", ctx2.t("Connected as", "已連線，身分："), info.name, info.server_version));
                ui.global::<Nav>().set_page(Page::Status);
                super::refresh_all(&ctx2);
            }
            Err(ConnectError::UnknownHost { fingerprint }) => {
                *pending.lock().unwrap() = Some(opts);
                ui.global::<ConnData>().set_pending_fingerprint(fingerprint.into());
                ctx2.info(ctx2.t("Confirm the server fingerprint.", "請確認伺服器指紋。"));
            }
            Err(ConnectError::HostKeyChanged { expected, actual }) => ctx2.error(format!(
                "{}\n{expected}\n{actual}",
                ctx2.t(
                    "WARNING: the server fingerprint changed. If the server was reinstalled, click Forget and connect again; otherwise do not connect.",
                    "警告：伺服器指紋改變了。如果伺服器重新安裝過，按「忘記」後重新連線；否則請不要連線。"
                )
            )),
            Err(ConnectError::AuthRejected) => ctx2.error(format!(
                "{}\n{public_line}",
                ctx2.t(
                    "The server rejected this key. Register it as an admin key in the server admin TUI (Import keys):",
                    "伺服器拒絕這把金鑰。請在伺服器的 Admin TUI（Import keys）把它加為 admin："
                )
            )),
            Err(e) => ctx2.error(e.to_string()),
        }
    });
}
