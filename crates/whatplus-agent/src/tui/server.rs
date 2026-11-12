//! 伺服器位址與 host key 驗證（TOFU）。

use anyhow::Result;
use whatplus_proto::Role;
use whatplus_ssh::client::{ConnectError, Connection};
use whatplus_ssh::keys;
use whatplus_term::Term;

use super::{Ctx, working};
use crate::upload;

const TITLE: &str = "Server";

pub fn address(term: &mut Term, ctx: &Ctx) -> Result<()> {
    let mut cfg = ctx.config()?;
    let help = ["Address of the WHAT Plus server on your network, e.g. 192.168.1.10"];
    let Some(host) = term.prompt(TITLE, &help, "Host", &cfg.server_host)? else {
        return Ok(());
    };
    let Some(port) = term.prompt(TITLE, &[], "Port", &cfg.server_port.to_string())? else {
        return Ok(());
    };
    let Ok(port) = port.parse::<u16>() else {
        term.message(TITLE, "Port must be a number between 1 and 65535.")?;
        return Ok(());
    };
    if host != cfg.server_host || port != cfg.server_port {
        // 換了伺服器就必須重新確認指紋。
        cfg.server_fingerprint = None;
    }
    cfg.server_host = host;
    cfg.server_port = port;
    ctx.save(&cfg)?;
    if cfg.server_fingerprint.is_none() && !cfg.server_host.is_empty() {
        term.message(TITLE, "Saved. Next, choose \"Verify server and test login\".")?;
    }
    Ok(())
}

pub fn verify(term: &mut Term, ctx: &Ctx) -> Result<()> {
    let mut cfg = ctx.config()?;
    if cfg.server_host.is_empty() {
        term.message(TITLE, "Set the server address first.")?;
        return Ok(());
    }
    let key = upload::load_key(&cfg)?;
    let public_line = keys::to_line(key.public_key());

    working(term, TITLE, "Connecting")?;
    let mut result = ctx.rt.block_on(Connection::connect(upload::options(&cfg, key.clone(), cfg.server_fingerprint.clone())));

    if let Err(ConnectError::UnknownHost { fingerprint }) = &result {
        let text = format!(
            "First connection to {}:{}.\n\nServer fingerprint:\n  {fingerprint}\n\n\
             Compare it with the fingerprint shown in the server admin TUI main menu.\n\
             Only continue if they are identical.",
            cfg.server_host, cfg.server_port
        );
        if !term.confirm(TITLE, &text)? {
            return Ok(());
        }
        cfg.server_fingerprint = Some(fingerprint.clone());
        ctx.save(&cfg)?;
        working(term, TITLE, "Logging in")?;
        result = ctx.rt.block_on(Connection::connect(upload::options(&cfg, key, cfg.server_fingerprint.clone())));
    }

    let text = match result {
        Ok(conn) => {
            let role = conn.role;
            let name = conn.name.clone();
            ctx.rt.block_on(conn.close());
            if role == Role::Agent {
                format!("OK. Logged in as \"{name}\". Uploads start automatically.")
            } else {
                format!("This key is registered as an ADMIN key (\"{name}\").\nUse a separate key with the agent role for this machine.")
            }
        }
        Err(ConnectError::AuthRejected) => format!(
            "The server rejected this key.\n\nAsk the administrator to add it as an agent key (client app > Machines,\n\
             or the server admin TUI > Import keys). Public key:\n\n{public_line}"
        ),
        Err(ConnectError::HostKeyChanged { expected, actual }) => format!(
            "WARNING: the server fingerprint changed.\n  saved:  {expected}\n  now:    {actual}\n\n\
             If the server was reinstalled, change the server address to reset the saved\n\
             fingerprint and verify again. Otherwise someone may be intercepting the connection."
        ),
        Err(e) => format!("Connection failed: {e}"),
    };
    term.message(TITLE, &text)
}
