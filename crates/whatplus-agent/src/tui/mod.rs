//! 來源端設定 TUI（英文介面）。設定寫入 `agent.json`，daemon 最晚 30 秒內套用。

mod actions;
mod key;
mod server;

use anyhow::Result;
use tokio::runtime::Runtime;
use whatplus_term::Term;

use crate::config::{AgentConfig, Paths};

pub struct Ctx<'a> {
    pub rt: &'a Runtime,
    pub paths: Paths,
}

impl Ctx<'_> {
    pub fn config(&self) -> Result<AgentConfig> {
        self.paths.load_config()
    }

    pub fn save(&self, config: &AgentConfig) -> Result<()> {
        self.paths.save_config(config)
    }
}

pub fn run(rt: &Runtime, paths: Paths) -> Result<()> {
    let ctx = Ctx { rt, paths };
    let mut term = Term::enter("WHAT Plus Agent")?;
    main_menu(&mut term, &ctx)
}

const MENU: [&str; 8] = [
    "Server address",
    "Private key",
    "Verify server and test login",
    "Upload interval",
    "Upload now",
    "Re-send all records",
    "Show local usage sources",
    "Exit",
];

fn header(ctx: &Ctx) -> Result<Vec<String>> {
    let cfg = ctx.config()?;
    let state = ctx.paths.load_state();
    let or_dash = |s: Option<String>| s.unwrap_or_else(|| "-".into());
    let mut lines = vec![
        format!(
            " Server:       {}",
            if cfg.server_host.is_empty() {
                "-".to_string()
            } else {
                format!("{}:{}", cfg.server_host, cfg.server_port)
            }
        ),
        format!(
            " Fingerprint:  {}",
            cfg.server_fingerprint
                .clone()
                .unwrap_or_else(|| "not verified".into())
        ),
        format!(
            " Private key:  {}",
            or_dash(cfg.key_path.as_ref().map(|p| p.display().to_string()))
        ),
        format!(" Interval:     every {} minutes", cfg.interval()),
        format!(
            " Last upload:  {} {}",
            or_dash(state.last_attempt_at),
            or_dash(state.last_result)
        ),
    ];
    let missing = cfg.missing();
    if !missing.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            " ! Not uploading yet. Missing: {}.",
            missing.join(", ")
        ));
    }
    Ok(lines)
}

fn main_menu(term: &mut Term, ctx: &Ctx) -> Result<()> {
    let mut cursor = 0;
    loop {
        let items: Vec<String> = MENU.iter().map(|s| s.to_string()).collect();
        let Some(choice) = term.choose("Main menu", &header(ctx)?, &items, cursor)? else {
            return Ok(());
        };
        cursor = choice;
        let result = match choice {
            0 => server::address(term, ctx),
            1 => key::screen(term, ctx),
            2 => server::verify(term, ctx),
            3 => actions::interval(term, ctx),
            4 => actions::upload(term, ctx, false),
            5 => actions::upload(term, ctx, true),
            6 => actions::sources(term, ctx),
            _ => return Ok(()),
        };
        if let Err(e) = result {
            term.message("Error", &format!("{e:#}"))?;
        }
    }
}

/// 長時間操作前先畫一個等待畫面。
pub fn working(term: &mut Term, title: &str, what: &str) -> Result<()> {
    term.draw(title, &[format!(" {what} ...")], "Please wait")
}
