//! 上傳間隔、立即上傳、檢視本機來源。

use anyhow::Result;
use whatplus_term::Term;

use super::{Ctx, working};
use crate::config::{MAX_INTERVAL_MINUTES, MIN_INTERVAL_MINUTES};
use crate::{collect, daemon, upload};

pub fn interval(term: &mut Term, ctx: &Ctx) -> Result<()> {
    const TITLE: &str = "Upload interval";
    let mut cfg = ctx.config()?;
    let help = [format!("Minutes between uploads ({MIN_INTERVAL_MINUTES}-{MAX_INTERVAL_MINUTES}). Default 10.")];
    let help: Vec<&str> = help.iter().map(String::as_str).collect();
    let Some(raw) = term.prompt(TITLE, &help, "Minutes", &cfg.interval().to_string())? else {
        return Ok(());
    };
    match raw.parse::<u32>() {
        Ok(n) if (MIN_INTERVAL_MINUTES..=MAX_INTERVAL_MINUTES).contains(&n) => {
            cfg.interval_minutes = n;
            ctx.save(&cfg)
        }
        _ => term.message(TITLE, "Enter a whole number of minutes in the allowed range."),
    }
}

pub fn upload(term: &mut Term, ctx: &Ctx, full: bool) -> Result<()> {
    let title = if full { "Re-send all records" } else { "Upload now" };
    if full
        && !term.confirm(
            title,
            "Send every local record again. The server ignores duplicates, so totals do not change;\nuse this after the server database was reset.",
        )?
    {
        return Ok(());
    }
    let cfg = ctx.config()?;
    working(term, title, "Reading local usage and uploading")?;
    let mut state = ctx.paths.load_state();
    state.last_attempt_at = Some(daemon::now());
    let text = match ctx.rt.block_on(upload::run_once(&cfg, full)) {
        Ok(s) => {
            let msg = format!("OK: sent {} records (server has {})", s.sent, s.server_records);
            state.last_success_at = state.last_attempt_at.clone();
            state.last_result = Some(msg.clone());
            msg
        }
        Err(e) => {
            let msg = format!("FAILED: {e:#}");
            state.last_result = Some(msg.clone());
            msg
        }
    };
    ctx.paths.save_state(&state)?;
    term.message(title, &text)
}

pub fn sources(term: &mut Term, ctx: &Ctx) -> Result<()> {
    const TITLE: &str = "Local usage sources";
    working(term, TITLE, "Reading local usage")?;
    let (_, summaries) = ctx.rt.block_on(async { tokio::task::spawn_blocking(collect::load_all).await })?;
    let mut lines = Vec::new();
    for s in summaries {
        lines.push(format!(" {}: {} records", s.name, s.records));
        if s.dirs.is_empty() {
            lines.push("   no data directory found (check the volume mounts)".into());
        }
        for d in s.dirs {
            lines.push(format!("   {d}"));
        }
        if let Some(e) = s.error {
            lines.push(format!("   error: {e}"));
        }
        lines.push(String::new());
    }
    term.draw(TITLE, &lines, "Any key = back")?;
    term.key()?;
    Ok(())
}
