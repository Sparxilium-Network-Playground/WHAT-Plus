//! Network allowlist：只有這些位址範圍可以連線（握手前檢查）。空清單＝全部拒絕。

use anyhow::Result;
use crossterm::event::KeyCode;
use whatplus_ssh::allow::parse_cidr;

use super::Ctx;
use super::term::{Term, list_lines};

const TITLE: &str = "Network allowlist";

pub fn screen(term: &mut Term, ctx: &Ctx) -> Result<()> {
    let mut cursor = 0usize;
    loop {
        let nets = ctx.store.allowed_networks()?;
        let mut lines = vec![
            " Connections from addresses outside these ranges are closed before the SSH".to_string(),
            " handshake. An empty list refuses everything.".to_string(),
            String::new(),
        ];
        if nets.is_empty() {
            lines.push("   (empty)".into());
        } else {
            cursor = cursor.min(nets.len() - 1);
            lines.extend(list_lines(&nets, cursor));
        }
        term.draw(TITLE, &lines, "A = add   D = remove   Up/Down = move   Esc = back")?;
        match term.key()?.code {
            KeyCode::Up => cursor = cursor.saturating_sub(1),
            KeyCode::Down if !nets.is_empty() => cursor = (cursor + 1).min(nets.len() - 1),
            KeyCode::Char('a') | KeyCode::Char('A') => add(term, ctx)?,
            KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Delete if !nets.is_empty() => {
                let cidr = &nets[cursor];
                if term.confirm(TITLE, &format!("Remove {cidr}?"))? {
                    ctx.store.remove_allowed_network(cidr)?;
                }
            }
            KeyCode::Esc => return Ok(()),
            _ => {}
        }
    }
}

fn add(term: &mut Term, ctx: &Ctx) -> Result<()> {
    let help = [
        "Examples: 192.168.0.0/16 (all 192.168.x.x), 192.168.1.0/24, 10.0.0.5",
        "Note: with Docker Desktop on Windows/macOS, connections may appear to come",
        "from the Docker gateway address; check System status after connecting.",
    ];
    let Some(raw) = term.prompt(TITLE, &help, "Range", "192.168.0.0/16")? else {
        return Ok(());
    };
    match parse_cidr(&raw) {
        Ok(net) => {
            if net.prefix_len() == 0 {
                let ok = term.confirm(TITLE, &format!("{net} allows EVERY address. Are you sure?"))?;
                if !ok {
                    return Ok(());
                }
            }
            ctx.store.add_allowed_network(&net.to_string())
        }
        Err(e) => term.message(TITLE, &e),
    }
}
