//! Admin TUI（英文介面）。與 daemon 共用資料庫；修改經 `Store::mutate`，daemon 約 5 秒內套用。

mod import;
mod keys;
mod network;
mod status;
mod term;

use std::path::Path;

use anyhow::Result;

use crate::config::Settings;
use crate::db::{Store, runtime};
use crate::hostkey;
use term::Term;

pub struct Ctx {
    pub store: Store,
    pub settings: Settings,
}

impl Ctx {
    /// daemon 寫入的指紋；daemon 尚未啟動過時直接讀 host key 檔。
    pub fn host_fingerprint(&self) -> String {
        if let Ok(Some(fp)) = self.store.runtime_get(runtime::HOST_FINGERPRINT) {
            return fp;
        }
        match whatplus_ssh::keys::load_private_key(Path::new(&self.settings.host_key), None) {
            Ok(k) => hostkey::fingerprint(&k),
            Err(_) => "(not generated yet; start the service once)".into(),
        }
    }

    /// 主選單上方的警告。
    pub fn warnings(&self) -> Vec<String> {
        let mut out = Vec::new();
        let keys = self.store.list_keys().unwrap_or_default();
        if !keys.iter().any(|k| k.enabled && k.role == whatplus_proto::Role::Admin) {
            out.push(" ! No admin key is registered. The service refuses all connections until".into());
            out.push("   you import one (Import keys) and give it the admin role.".into());
        }
        if self.store.allowed_networks().unwrap_or_default().is_empty() {
            out.push(" ! The network allowlist is empty. Every connection is refused until you".into());
            out.push("   add a range such as 192.168.0.0/16 (Network allowlist).".into());
        }
        out
    }
}

pub fn run(settings: Settings) -> Result<()> {
    let store = Store::open(&settings.database)?;
    let ctx = Ctx { store, settings };
    let mut term = Term::enter()?;
    main_menu(&mut term, &ctx)
}

const MENU: [&str; 6] = [
    "Authorized keys",
    "Import keys from a directory",
    "Network allowlist",
    "System status",
    "Restart service",
    "Exit",
];

fn main_menu(term: &mut Term, ctx: &Ctx) -> Result<()> {
    let mut cursor = 0;
    loop {
        let mut header = vec![format!(" Host key fingerprint: {}", ctx.host_fingerprint())];
        let warnings = ctx.warnings();
        if !warnings.is_empty() {
            header.push(String::new());
            header.extend(warnings);
        }
        let items: Vec<String> = MENU.iter().map(|s| s.to_string()).collect();
        let Some(choice) = term.choose("Main menu", &header, &items, cursor)? else {
            return Ok(());
        };
        cursor = choice;
        let result = match choice {
            0 => keys::screen(term, ctx),
            1 => import::screen(term, ctx),
            2 => network::screen(term, ctx),
            3 => status::screen(term, ctx),
            4 => restart(term, ctx),
            _ => return Ok(()),
        };
        if let Err(e) = result {
            term.message("Error", &format!("{e:#}"))?;
        }
    }
}

fn restart(term: &mut Term, ctx: &Ctx) -> Result<()> {
    let text = "The service process will exit and Docker will start it again (about 10-20 s).\n\
                Settings in the database are kept. Changes to config.conf, the image or\n\
                docker-compose.yml are NOT applied by this; use docker compose up -d instead.";
    if term.confirm("Restart service", text)? {
        ctx.store.request_restart()?;
        term.message("Restart service", "Restart requested. The service restarts within a few seconds.")?;
    }
    Ok(())
}
