//! `whatplus-server setup ...`：給 setup.sh / setup.ps1 呼叫的非互動指令。
//!
//! 輸出是給腳本解析的純文字（`key=value` 或以 Tab 分隔的欄位），錯誤印到 stderr 並以非 0 結束。
//! 精靈以 `docker compose run --rm server setup ...` 執行，和 daemon 共用同一個資料庫。
//!
//! - `setup status`                       → `fingerprint=`、`admins=`、`agents=`、`allowlist=`、`last_refused=`
//! - `setup scan`                         → 每把公鑰一行：`state  fingerprint  file:line  comment`
//! - `setup check`                        → stdin 每行 `label<Tab>公鑰行`（精靈在主機上讀取 keys/ 與
//!   ~/.ssh，容器不必掛載 ~/.ssh）；每把公鑰輸出一行：`state  fingerprint  label  comment  公鑰行`
//! - `setup add-key <fingerprint> <admin|agent> <name>`
//! - `setup allow <cidr>`

use std::process::ExitCode;

use anyhow::{Result, anyhow, bail};
use whatplus_proto::Role;
use whatplus_ssh::allow::parse_cidr;
use whatplus_ssh::keys;

use crate::config::Settings;
use crate::db::{NewKey, Store, runtime};
use crate::{hostkey, validate};

pub fn run(settings: Settings, args: &[String]) -> ExitCode {
    match dispatch(&settings, args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ERROR: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn dispatch(settings: &Settings, args: &[String]) -> Result<()> {
    let arg = |i: usize| args.get(i).map(String::as_str).unwrap_or("");
    match arg(0) {
        "status" => status(settings),
        "scan" => scan(settings),
        "check" => check(settings),
        "add-key" => add_key(settings, arg(1), arg(2), arg(3)),
        "allow" => allow(settings, arg(1)),
        other => bail!(
            "unknown setup command {other:?}; use status, scan, add-key <fingerprint> <admin|agent> <name>, or allow <cidr>"
        ),
    }
}

/// host key 還沒產生時在這裡產生，讓精靈在第一次啟動前就能顯示指紋。
fn status(settings: &Settings) -> Result<()> {
    let key = hostkey::load_or_create(&settings.host_key)?;
    let store = Store::open(&settings.database)?;
    let list = store.list_keys()?;
    let count = |role: Role| list.iter().filter(|k| k.enabled && k.role == role).count();
    println!("fingerprint={}", hostkey::fingerprint(&key));
    println!("admins={}", count(Role::Admin));
    println!("agents={}", count(Role::Agent));
    println!("allowlist={}", store.allowed_networks()?.join(","));
    println!(
        "last_refused={}",
        store
            .runtime_get(runtime::LAST_REFUSED)?
            .unwrap_or_default()
    );
    Ok(())
}

fn scan(settings: &Settings) -> Result<()> {
    let store = Store::open(&settings.database)?;
    let registered = store.list_keys()?;
    let found = keys::scan_public_keys(&settings.keys_dir)?;
    for entry in &found {
        let file = entry
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let place = format!("{file}:{}", entry.line);
        let Some(key) = entry.supported_key() else {
            println!("unsupported\t-\t{place}\t{}", entry.describe());
            continue;
        };
        let fp = keys::fingerprint(key);
        let state = key_state(&registered, &fp);
        println!("{state}\t{fp}\t{place}\t{}", one_line(&keys::comment(key)));
    }
    Ok(())
}

/// `state`：`new`、已註冊的角色（`admin` / `agent`）或 `unsupported`。
fn key_state(registered: &[whatplus_proto::KeyInfo], fp: &str) -> &'static str {
    registered
        .iter()
        .find(|k| k.fingerprint == fp)
        .map(|k| k.role.as_str())
        .unwrap_or("new")
}

fn check(settings: &Settings) -> Result<()> {
    let store = Store::open(&settings.database)?;
    let registered = store.list_keys()?;
    for row in check_lines(&registered, std::io::stdin().lock())? {
        println!("{row}");
    }
    Ok(())
}

/// 輸入每行 `label<Tab>公鑰行`；一個檔案有多把公鑰時，精靈會逐行送進來，每把各輸出一行。
fn check_lines(
    registered: &[whatplus_proto::KeyInfo],
    input: impl std::io::BufRead,
) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for line in input.lines() {
        let line = line?;
        // Windows PowerShell 5.1 會在送給原生程式的文字開頭加 BOM。
        let line = line.trim_start_matches('\u{feff}');
        let Some((label, raw)) = line.split_once('\t') else {
            continue;
        };
        let raw = raw.trim();
        if raw.is_empty() || raw.starts_with('#') {
            continue;
        }
        let label = one_line(label);
        out.push(match keys::parse_public_key_line(raw) {
            Ok(key) if keys::is_supported(&key) => {
                let fp = keys::fingerprint(&key);
                // 空欄位會讓 shell 的 `read` 合併相鄰的 Tab，所以沒有註解時輸出 `-`。
                let comment = one_line(keys::comment(&key).trim());
                format!(
                    "{}\t{fp}\t{label}\t{}\t{}",
                    key_state(registered, &fp),
                    if comment.is_empty() { "-" } else { &comment },
                    one_line(&keys::to_line(&key))
                )
            }
            Ok(key) => format!(
                "unsupported\t-\t{label}\t{}\t-",
                one_line(key.algorithm().as_ref())
            ),
            Err(_) => format!("unsupported\t-\t{label}\tunreadable\t-"),
        });
    }
    Ok(out)
}

fn add_key(settings: &Settings, fingerprint: &str, role: &str, name: &str) -> Result<()> {
    let role = Role::parse(role).ok_or_else(|| anyhow!("role must be admin or agent"))?;
    let name = validate::name("name", name).map_err(|e| anyhow!(e))?;
    let found = keys::scan_public_keys(&settings.keys_dir)?;
    let key = found
        .iter()
        .filter_map(|e| e.supported_key())
        .find(|k| keys::fingerprint(k) == fingerprint)
        .ok_or_else(|| {
            anyhow!(
                "no ssh-ed25519 key with fingerprint {fingerprint} in {}",
                settings.keys_dir.display()
            )
        })?;
    let store = Store::open(&settings.database)?;
    store.add_key(NewKey {
        name: &name,
        role,
        public_key: &keys::to_line(key),
        fingerprint,
        allowed_cidrs: &[],
    })?;
    println!("added {name} ({})", role.as_str());
    Ok(())
}

fn allow(settings: &Settings, raw: &str) -> Result<()> {
    let net = parse_cidr(raw).map_err(|e| anyhow!(e))?;
    let store = Store::open(&settings.database)?;
    store.add_allowed_network(&net.to_string())?;
    println!("allowed {net}");
    Ok(())
}

/// 註解可能含 Tab 或換行，會破壞腳本的欄位解析。
fn one_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_lists_every_key_and_strips_options() {
        let a = keys::generate_ed25519("laptop").unwrap().1;
        let b = keys::generate_ed25519("desktop").unwrap().1;
        let input = format!(
            "\u{feff}~/.ssh/team.pub:1\t{a}\n~/.ssh/team.pub:2\t{b}\n\
             ~/.ssh/authorized_keys:1\trestrict,from=\"10.0.0.1\" {b}\n\
             ~/.ssh/id_rsa.pub:1\tssh-rsa AAAAB3NzaC1yc2E bad\n\
             ~/.ssh/x.pub:2\t# comment\n"
        );
        let rows = check_lines(&[], input.as_bytes()).unwrap();
        assert_eq!(rows.len(), 4);
        let cols: Vec<Vec<&str>> = rows.iter().map(|r| r.split('\t').collect()).collect();
        assert_eq!(cols[0][0], "new");
        assert_eq!(cols[0][2], "~/.ssh/team.pub:1");
        assert_eq!(cols[0][3], "laptop");
        assert_eq!(cols[1][3], "desktop");
        // authorized_keys 的選項被去掉，輸出可直接寫成 .pub 的公鑰行。
        assert_eq!(cols[2][4], b.trim());
        assert_eq!(cols[2][1], cols[1][1]);
        assert_eq!(cols[3][0], "unsupported");
    }
}
