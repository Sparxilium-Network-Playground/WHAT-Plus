//! Import keys：掃描目錄中的 `.pub` / `authorized_keys`（一個檔案可有多把），勾選後指定角色加入。

use std::path::{Path, PathBuf};

use anyhow::Result;
use crossterm::event::KeyCode;
use whatplus_proto::Role;
use whatplus_ssh::keys::{self, FoundPublicKey};

use super::Ctx;
use super::term::{Term, checkbox, list_lines};
use crate::db::NewKey;
use crate::validate;

const TITLE: &str = "Import keys";

pub fn screen(term: &mut Term, ctx: &Ctx) -> Result<()> {
    let default_dir = ctx.settings.keys_dir.display().to_string();
    let help = [
        "Directory to scan for *.pub and authorized_keys files.",
        "In Docker this is the keys/ folder next to docker-compose.yml.",
        "Only ssh-ed25519 keys are supported.",
    ];
    let Some(dir) = term.prompt(TITLE, &help, "Directory", &default_dir)? else {
        return Ok(());
    };
    let dir = PathBuf::from(if dir.is_empty() { default_dir } else { dir });
    let found = keys::scan_public_keys(&dir)?;
    if found.is_empty() {
        term.message(TITLE, &format!("No public keys found in {}.", dir.display()))?;
        return Ok(());
    }
    let registered: Vec<String> = ctx.store.list_keys()?.into_iter().map(|k| k.fingerprint).collect();
    let Some(selected) = pick(term, &dir, &found, &registered)? else {
        return Ok(());
    };
    if selected.is_empty() {
        return Ok(());
    }
    let roles = ["agent  (a machine that uploads usage)", "admin  (the client app that manages settings)"];
    let items: Vec<String> = roles.iter().map(|s| s.to_string()).collect();
    let header = vec![format!(" Role for the {} selected key(s):", selected.len())];
    let Some(role_idx) = term.choose(TITLE, &header, &items, 0)? else {
        return Ok(());
    };
    let role = if role_idx == 1 { Role::Admin } else { Role::Agent };

    let mut added = Vec::new();
    for idx in selected {
        let entry = &found[idx];
        let Some(key) = entry.supported_key() else { continue };
        let suggestion = suggest_name(entry);
        let help = [format!("Key: {}", keys::fingerprint(key)), format!("From: {}:{}", file_name(&entry.path), entry.line)];
        let help: Vec<&str> = help.iter().map(String::as_str).collect();
        let Some(name) = term.prompt(TITLE, &help, "Name", &suggestion)? else {
            continue;
        };
        let name = match validate::name("name", &name) {
            Ok(n) => n,
            Err(e) => {
                term.message(TITLE, &e)?;
                continue;
            }
        };
        let fingerprint = keys::fingerprint(key);
        match ctx.store.add_key(NewKey {
            name: &name,
            role,
            public_key: &keys::to_line(key),
            fingerprint: &fingerprint,
            allowed_cidrs: &[],
        }) {
            Ok(_) => added.push(format!("{name} ({})", role.as_str())),
            Err(e) => term.message(TITLE, &format!("{name}: {e}"))?,
        }
    }
    if !added.is_empty() {
        term.message(TITLE, &format!("Added:\n{}\n\nThe service applies the change within a few seconds.", added.join("\n")))?;
    }
    Ok(())
}

/// 勾選清單。回傳選中的索引；Esc 回傳 `None`。
fn pick(term: &mut Term, dir: &Path, found: &[FoundPublicKey], registered: &[String]) -> Result<Option<Vec<usize>>> {
    let selectable = |e: &FoundPublicKey| {
        e.supported_key()
            .is_some_and(|k| !registered.contains(&keys::fingerprint(k)))
    };
    let mut checked = vec![false; found.len()];
    let mut cursor = 0usize;
    loop {
        let rows: Vec<String> = found
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let mark = if selectable(e) {
                    checkbox(checked[i]).to_string()
                } else if e.supported_key().is_some() {
                    "(registered)".to_string()
                } else {
                    "(skip)".to_string()
                };
                format!("{mark} {}:{}  {}", file_name(&e.path), e.line, e.describe())
            })
            .collect();
        let mut lines = vec![format!(" Keys in {}:", dir.display()), String::new()];
        lines.extend(list_lines(&rows, cursor));
        term.draw(TITLE, &lines, "Up/Down = move   Space = select   Enter = continue   Esc = cancel")?;
        match term.key()?.code {
            KeyCode::Up => cursor = cursor.saturating_sub(1),
            KeyCode::Down => cursor = (cursor + 1).min(found.len() - 1),
            KeyCode::Char(' ') if selectable(&found[cursor]) => checked[cursor] = !checked[cursor],
            KeyCode::Enter => {
                return Ok(Some((0..found.len()).filter(|&i| checked[i]).collect()));
            }
            KeyCode::Esc => return Ok(None),
            _ => {}
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// 預設名稱：公鑰註解，沒有的話用檔名。
fn suggest_name(entry: &FoundPublicKey) -> String {
    let comment = entry.supported_key().map(keys::comment).unwrap_or_default();
    let base = if comment.trim().is_empty() {
        file_name(&entry.path).trim_end_matches(".pub").to_string()
    } else {
        comment
    };
    base.chars().take(validate::MAX_NAME).collect()
}
