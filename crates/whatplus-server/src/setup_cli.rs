//! `whatplus-server setup ...`：給 setup.sh / setup.ps1 呼叫的非互動指令。
//!
//! 輸出是給腳本解析的純文字（`key=value` 或以 Tab 分隔的欄位），錯誤印到 stderr 並以非 0 結束。
//! 精靈以 `docker compose run --rm server setup ...` 執行，和 daemon 共用同一個資料庫。
//!
//! - `setup status`                       → `fingerprint=`、`admins=`、`agents=`、`allowlist=`、`last_refused=`
//! - `setup scan`                         → 每把公鑰一行：`state  fingerprint  file:line  comment`
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
        let state = registered
            .iter()
            .find(|k| k.fingerprint == fp)
            .map(|k| k.role.as_str())
            .unwrap_or("new");
        println!("{state}\t{fp}\t{place}\t{}", one_line(&keys::comment(key)));
    }
    Ok(())
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
