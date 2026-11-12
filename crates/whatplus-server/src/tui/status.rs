//! System status：daemon 狀態、Discord、紀錄數、排程。

use anyhow::Result;

use super::Ctx;
use whatplus_term::Term;
use crate::db::runtime;

pub fn screen(term: &mut Term, ctx: &Ctx) -> Result<()> {
    let s = &ctx.store;
    let get = |k: &str| s.runtime_get(k).ok().flatten().unwrap_or_else(|| "-".into());
    let keys = s.list_keys()?;
    let mut lines = vec![
        format!(" Listening:        {}:{}", ctx.settings.listen, ctx.settings.port),
        format!(" Host fingerprint: {}", ctx.host_fingerprint()),
        format!(" Time zone:        {}", ctx.settings.timezone_name),
        format!(" Service started:  {}", get(runtime::DAEMON_STARTED_AT)),
        format!(" Last heartbeat:   {}", get(runtime::DAEMON_HEARTBEAT)),
        format!(" Discord:          {}", get(runtime::DISCORD_STATUS)),
        format!(" Allowlist:        {}", s.allowed_networks()?.join(", ")),
        format!(" Usage records:    {}", s.record_count()?),
        format!(" Last report:      {} {}", get(runtime::LAST_REPORT_AT), get(runtime::LAST_REPORT_RESULT)),
        String::new(),
        " Keys:".into(),
    ];
    for k in &keys {
        lines.push(format!(
            "   {:<5} {:<20} {} records={} last={} {}",
            k.role.as_str(),
            k.name.chars().take(20).collect::<String>(),
            if k.enabled { "on " } else { "off" },
            k.record_count,
            k.last_seen_at.as_deref().unwrap_or("never"),
            k.last_seen_ip.as_deref().unwrap_or("")
        ));
    }
    lines.push(String::new());
    lines.push(" Schedules (edit them in the client app):".into());
    let schedules = s.list_schedules()?;
    if schedules.is_empty() {
        lines.push("   (none)".into());
    }
    for sc in schedules {
        let result = s.schedule_run(sc.id)?.map(|r| r.result).unwrap_or_else(|| "-".into());
        lines.push(format!(
            "   {} {:<20} {:?}  last: {} {}",
            if sc.enabled { "on " } else { "off" },
            sc.name.chars().take(20).collect::<String>(),
            sc.kind,
            sc.last_run_at.as_deref().unwrap_or("never"),
            result
        ));
    }
    term.draw("System status", &lines, "Any key = back")?;
    term.key()?;
    Ok(())
}
