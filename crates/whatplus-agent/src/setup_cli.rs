//! `whatplus-agent setup ...`：給 setup.sh / setup.ps1 呼叫的非互動指令。
//!
//! 輸出是給腳本解析的 `key=value`，錯誤印到 stderr。結束代碼：
//! 0 成功、1 錯誤、3 伺服器拒絕這把金鑰、4 這把金鑰是 admin、5 伺服器指紋改變。
//!
//! - `setup status`                → `server=`、`port=`、`key=`、`public_key=`、`fingerprint=`、`interval=`
//! - `setup server <host> [port]`  → 換伺服器時清除已確認的指紋
//! - `setup key <comment>`         → 沿用 keys/ 中的專用金鑰，沒有就產生；印出 `public_key=`
//! - `setup probe`                 → 連線但不登入，印出伺服器的 `fingerprint=`
//! - `setup trust <fingerprint>`   → 記住指紋並測試登入
//! - `setup interval <minutes>`
//! - `setup sources`               → 每個工具一行：`name  records  dirs  error`

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow, bail};
use tokio::runtime::Runtime;
use whatplus_proto::Role;
use whatplus_ssh::client::{ConnectError, Connection};
use whatplus_ssh::keys;

use crate::collect;
use crate::config::{AgentConfig, MAX_INTERVAL_MINUTES, MIN_INTERVAL_MINUTES, Paths};
use crate::upload;

/// setup 產生的專用金鑰檔名（在 keys/ 目錄）。
pub const KEY_FILE: &str = "whatplus_agent_ed25519";

pub fn run(rt: &Runtime, paths: Paths, args: &[String]) -> ExitCode {
    let arg = |i: usize| args.get(i).map(String::as_str).unwrap_or("");
    let result = match arg(0) {
        "status" => status(&paths).map(|_| 0),
        "server" => server(&paths, arg(1), arg(2)).map(|_| 0),
        "key" => key(&paths, arg(1)).map(|_| 0),
        "probe" => probe(rt, &paths).map(|_| 0),
        "trust" => trust(rt, &paths, arg(1)),
        "interval" => interval(&paths, arg(1)).map(|_| 0),
        "sources" => sources().map(|_| 0),
        other => Err(anyhow!(
            "unknown setup command {other:?}; use status, server, key, probe, trust, interval or sources"
        )),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("ERROR: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn status(paths: &Paths) -> Result<()> {
    let cfg = paths.load_config()?;
    let public = upload::load_key(&cfg)
        .map(|k| keys::to_line(k.public_key()))
        .unwrap_or_default();
    println!("server={}", cfg.server_host);
    println!("port={}", cfg.server_port);
    println!(
        "key={}",
        cfg.key_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    );
    println!("public_key={public}");
    println!(
        "fingerprint={}",
        cfg.server_fingerprint.clone().unwrap_or_default()
    );
    println!("interval={}", cfg.interval());
    Ok(())
}

fn server(paths: &Paths, host: &str, port: &str) -> Result<()> {
    let host = host.trim();
    if host.is_empty() || host.contains(char::is_whitespace) {
        bail!("give the server address, e.g. 192.168.1.10");
    }
    let port = if port.is_empty() {
        whatplus_proto::DEFAULT_PORT
    } else {
        port.parse::<u16>()
            .ok()
            .filter(|p| *p > 0)
            .context("port must be a number between 1 and 65535")?
    };
    let mut cfg = paths.load_config()?;
    if cfg.server_host != host || cfg.server_port != port {
        cfg.server_fingerprint = None;
    }
    cfg.server_host = host.to_string();
    cfg.server_port = port;
    paths.save_config(&cfg)?;
    println!("server={host}");
    println!("port={port}");
    Ok(())
}

fn key(paths: &Paths, comment: &str) -> Result<()> {
    let path: PathBuf = paths.keys_dir.join(KEY_FILE);
    let key = if path.exists() {
        keys::load_private_key(&path, None)
            .with_context(|| format!("{} exists but cannot be used", path.display()))?
    } else {
        let comment = if comment.trim().is_empty() {
            "whatplus-agent"
        } else {
            comment.trim()
        };
        let (private, public) = keys::generate_ed25519(comment)?;
        keys::write_new_private_key(&path, &private)?;
        let _ = std::fs::write(
            paths.keys_dir.join(format!("{KEY_FILE}.pub")),
            format!("{public}\n"),
        );
        println!("generated=yes");
        keys::load_private_key(&path, None)?
    };
    let mut cfg = paths.load_config()?;
    cfg.key_path = Some(path.clone());
    paths.save_config(&cfg)?;
    println!("key={}", path.display());
    println!("public_key={}", keys::to_line(key.public_key()));
    Ok(())
}

fn ready(paths: &Paths) -> Result<AgentConfig> {
    let cfg = paths.load_config()?;
    if cfg.server_host.is_empty() {
        bail!("set the server address first");
    }
    Ok(cfg)
}

fn probe(rt: &Runtime, paths: &Paths) -> Result<()> {
    let cfg = ready(paths)?;
    let key = upload::load_key(&cfg)?;
    match rt.block_on(Connection::connect(upload::options(&cfg, key, None))) {
        Err(ConnectError::UnknownHost { fingerprint }) => {
            println!("fingerprint={fingerprint}");
            Ok(())
        }
        Ok(_) => bail!("unexpected: logged in without a verified fingerprint"),
        Err(e) => Err(e.into()),
    }
}

fn trust(rt: &Runtime, paths: &Paths, fingerprint: &str) -> Result<u8> {
    if !fingerprint.starts_with("SHA256:") {
        bail!("the fingerprint looks like SHA256:...");
    }
    let mut cfg = ready(paths)?;
    let key = upload::load_key(&cfg)?;
    cfg.server_fingerprint = Some(fingerprint.to_string());
    paths.save_config(&cfg)?;
    let result = rt.block_on(Connection::connect(upload::options(
        &cfg,
        key,
        cfg.server_fingerprint.clone(),
    )));
    match result {
        Ok(conn) => {
            let (role, name) = (conn.role, conn.name.clone());
            rt.block_on(conn.close());
            println!("name={name}");
            if role == Role::Agent {
                println!("login=ok");
                Ok(0)
            } else {
                println!("login=admin");
                Ok(4)
            }
        }
        Err(ConnectError::AuthRejected) => {
            println!("login=rejected");
            Ok(3)
        }
        Err(ConnectError::HostKeyChanged { actual, .. }) => {
            println!("login=changed");
            println!("actual={actual}");
            Ok(5)
        }
        Err(e) => Err(e.into()),
    }
}

fn interval(paths: &Paths, raw: &str) -> Result<()> {
    let minutes: u32 = raw
        .parse()
        .ok()
        .filter(|m| (MIN_INTERVAL_MINUTES..=MAX_INTERVAL_MINUTES).contains(m))
        .with_context(|| {
            format!("interval must be {MIN_INTERVAL_MINUTES}-{MAX_INTERVAL_MINUTES} minutes")
        })?;
    let mut cfg = paths.load_config()?;
    cfg.interval_minutes = minutes;
    paths.save_config(&cfg)?;
    println!("interval={minutes}");
    Ok(())
}

fn sources() -> Result<()> {
    let (_, summaries) = collect::load_all();
    for s in summaries {
        let dirs = if s.dirs.is_empty() {
            "-".to_string()
        } else {
            s.dirs.join(",")
        };
        let error = s.error.unwrap_or_default().replace(['\t', '\n'], " ");
        println!("{}\t{}\t{dirs}\t{error}", s.name, s.records);
    }
    Ok(())
}
