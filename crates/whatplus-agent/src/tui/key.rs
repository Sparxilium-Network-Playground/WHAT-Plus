//! 選擇私鑰：掃描目錄（預設 /keys）中的 OpenSSH 私鑰，或產生一把新的 Ed25519 專用金鑰。

use std::path::PathBuf;

use anyhow::Result;
use whatplus_ssh::keys;
use whatplus_term::Term;

use super::Ctx;

const TITLE: &str = "Private key";

pub fn screen(term: &mut Term, ctx: &Ctx) -> Result<()> {
    let default_dir = ctx.paths.keys_dir.display().to_string();
    let help = [
        "Directory with the private key for this machine (mounted keys/ folder).",
        "Use a dedicated ssh-ed25519 key without a passphrase; do not reuse the",
        "key you log in to other servers with.",
    ];
    let Some(dir) = term.prompt(TITLE, &help, "Directory", &default_dir)? else {
        return Ok(());
    };
    let dir = PathBuf::from(if dir.is_empty() { default_dir } else { dir });
    let found = keys::scan_private_keys(&dir).unwrap_or_default();

    let mut items: Vec<String> = found
        .iter()
        .map(|k| {
            let note = if !k.supported {
                "  (unsupported type)"
            } else if k.encrypted {
                "  (has a passphrase - not usable here)"
            } else {
                ""
            };
            format!(
                "{}  {}  {}{note}",
                k.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                k.fingerprint,
                k.comment
            )
        })
        .collect();
    items.push("Generate a new key in this directory".into());
    let header = vec![format!(" Private keys in {}:", dir.display())];
    let Some(choice) = term.choose(TITLE, &header, &items, 0)? else {
        return Ok(());
    };

    let path = if choice == found.len() {
        match generate(term, &dir)? {
            Some(p) => p,
            None => return Ok(()),
        }
    } else {
        let k = &found[choice];
        if !k.supported || k.encrypted {
            term.message(TITLE, "This key cannot be used. Choose an ssh-ed25519 key without a passphrase.")?;
            return Ok(());
        }
        k.path.clone()
    };

    let key = keys::load_private_key(&path, None)?;
    let mut cfg = ctx.config()?;
    cfg.key_path = Some(path);
    ctx.save(&cfg)?;
    term.message(
        TITLE,
        &format!(
            "Selected. Register this public key on the server as an agent key:\n\n{}\n\n\
             Then choose \"Verify server and test login\".",
            keys::to_line(key.public_key())
        ),
    )
}

fn generate(term: &mut Term, dir: &std::path::Path) -> Result<Option<PathBuf>> {
    let Some(name) = term.prompt(TITLE, &["File name for the new private key."], "File name", "whatplus_agent_ed25519")? else {
        return Ok(None);
    };
    if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
        term.message(TITLE, "Use a plain file name without slashes.")?;
        return Ok(None);
    }
    let Some(comment) = term.prompt(TITLE, &["A label for the key, e.g. this machine's name."], "Comment", "whatplus-agent")? else {
        return Ok(None);
    };
    let path = dir.join(&name);
    let (private, public) = keys::generate_ed25519(&comment)?;
    keys::write_new_private_key(&path, &private)?;
    // 公鑰另存一份，方便複製到伺服器的 keys/ 目錄。
    let _ = std::fs::write(dir.join(format!("{name}.pub")), format!("{public}\n"));
    Ok(Some(path))
}
