//! Authorized keys：列出已註冊的公鑰，啟用／停用、改名、改角色、限制 IP、刪除。

use anyhow::Result;
use whatplus_term::KeyCode;
use whatplus_proto::{KeyInfo, Role};

use super::Ctx;
use whatplus_term::{Term, checkbox, list_lines};
use crate::validate;

const TITLE: &str = "Authorized keys";

fn row(k: &KeyInfo) -> String {
    let cidrs = if k.allowed_cidrs.is_empty() {
        "any allowed IP".to_string()
    } else {
        k.allowed_cidrs.join(",")
    };
    let seen = k.last_seen_at.as_deref().unwrap_or("never");
    format!(
        "{} {:<5} {:<20} {}  [{}]  last seen {}",
        checkbox(k.enabled),
        k.role.as_str(),
        k.name.chars().take(20).collect::<String>(),
        k.fingerprint,
        cidrs,
        seen
    )
}

pub fn screen(term: &mut Term, ctx: &Ctx) -> Result<()> {
    let mut cursor = 0usize;
    loop {
        let keys = ctx.store.list_keys()?;
        let mut lines = vec![
            " [*] = enabled. Admin keys can change everything from the client app;".to_string(),
            " agent keys can only upload usage records.".to_string(),
            String::new(),
        ];
        if keys.is_empty() {
            lines.push(" No keys yet. Use \"Import keys from a directory\" in the main menu.".into());
            term.draw(TITLE, &lines, "Esc = back")?;
            term.key()?;
            return Ok(());
        }
        cursor = cursor.min(keys.len() - 1);
        let rows: Vec<String> = keys.iter().map(row).collect();
        lines.extend(list_lines(&rows, cursor));
        term.draw(TITLE, &lines, "Up/Down = move   Space = enable/disable   Enter = edit   Esc = back")?;
        match term.key()?.code {
            KeyCode::Up => cursor = cursor.saturating_sub(1),
            KeyCode::Down => cursor = (cursor + 1).min(keys.len() - 1),
            KeyCode::Char(' ') => toggle(term, ctx, &keys[cursor])?,
            KeyCode::Enter => edit(term, ctx, &keys[cursor])?,
            KeyCode::Esc => return Ok(()),
            _ => {}
        }
    }
}

fn enabled_admins(ctx: &Ctx) -> Result<usize> {
    Ok(ctx
        .store
        .list_keys()?
        .iter()
        .filter(|k| k.enabled && k.role == Role::Admin)
        .count())
}

fn toggle(term: &mut Term, ctx: &Ctx, key: &KeyInfo) -> Result<()> {
    if key.enabled && key.role == Role::Admin && enabled_admins(ctx)? == 1 {
        let ok = term.confirm(
            TITLE,
            "This is the last enabled admin key.\nWithout an admin key the service refuses ALL connections and stops sending reports.",
        )?;
        if !ok {
            return Ok(());
        }
    }
    ctx.store.set_key_enabled(key.id, !key.enabled)
}

fn edit(term: &mut Term, ctx: &Ctx, key: &KeyInfo) -> Result<()> {
    let header = vec![
        format!(" Name:        {}", key.name),
        format!(" Role:        {}", key.role.as_str()),
        format!(" Fingerprint: {}", key.fingerprint),
        format!(" Public key:  {}", key.public_key.chars().take(70).collect::<String>()),
        format!(" Records:     {}", key.record_count),
        format!(" Last seen:   {} {}", key.last_seen_at.as_deref().unwrap_or("never"), key.last_seen_ip.as_deref().unwrap_or("")),
    ];
    let items: Vec<String> = vec![
        "Rename".into(),
        format!("Change role to {}", other(key.role).as_str()),
        "Restrict to IP ranges".into(),
        if key.enabled { "Disable".into() } else { "Enable".into() },
        "Delete (also deletes this machine's usage records)".into(),
        "Back".into(),
    ];
    let Some(choice) = term.choose(TITLE, &header, &items, 0)? else {
        return Ok(());
    };
    match choice {
        0 => {
            if let Some(name) = term.prompt(TITLE, &[], "New name", &key.name)? {
                let name = validate::name("name", &name).map_err(anyhow::Error::msg)?;
                ctx.store.update_key(key.id, &name, &key.allowed_cidrs)?;
            }
        }
        1 => {
            let to = other(key.role);
            if key.role == Role::Admin && key.enabled && enabled_admins(ctx)? == 1 {
                term.message(TITLE, "This is the last enabled admin key; add another admin first.")?;
            } else if term.confirm(TITLE, &format!("Change \"{}\" to {}?", key.name, to.as_str()))? {
                ctx.store.set_key_role(key.id, to)?;
            }
        }
        2 => {
            let help = [
                "Comma-separated CIDR ranges this key may connect from, e.g. 192.168.1.20/32.",
                "Leave empty to allow any address in the network allowlist.",
            ];
            if let Some(raw) = term.prompt(TITLE, &help, "IP ranges", &key.allowed_cidrs.join(","))? {
                let items: Vec<String> = raw.split(',').map(|s| s.trim().to_string()).collect();
                let cidrs = validate::cidrs(&items).map_err(anyhow::Error::msg)?;
                ctx.store.update_key(key.id, &key.name, &cidrs)?;
            }
        }
        3 => toggle(term, ctx, key)?,
        4 => {
            if key.role == Role::Admin && key.enabled && enabled_admins(ctx)? == 1 {
                term.message(TITLE, "This is the last enabled admin key; add another admin first.")?;
            } else if term.confirm(
                TITLE,
                &format!(
                    "Delete \"{}\" and its {} usage records? This cannot be undone.",
                    key.name, key.record_count
                ),
            )? {
                ctx.store.delete_key(key.id)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn other(role: Role) -> Role {
    match role {
        Role::Admin => Role::Agent,
        Role::Agent => Role::Admin,
    }
}
