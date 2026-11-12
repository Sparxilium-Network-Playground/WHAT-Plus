//! 管理客戶端：來源（agent）公鑰。admin 公鑰只能在伺服器 TUI 管理。

use whatplus_proto::{AgentKeyInput, Response, Role};
use whatplus_ssh::keys;

use super::{ApiError, ApiResult};
use crate::app::App;
use crate::db::NewKey;
use crate::validate;

pub fn list(app: &App) -> ApiResult {
    Ok(Response::Keys {
        keys: app.store.list_keys()?,
    })
}

pub fn add(app: &App, input: AgentKeyInput) -> ApiResult {
    let name = validate::name("name", &input.name)?;
    let key = keys::parse_ed25519_public_key(&input.public_key)
        .map_err(|e| ApiError::User(format!("public key: {e}")))?;
    let cidrs = validate::cidrs(&input.allowed_cidrs)?;
    let fingerprint = keys::fingerprint(&key);
    let id = app
        .store
        .add_key(NewKey {
            name: &name,
            role: Role::Agent,
            public_key: &keys::to_line(&key),
            fingerprint: &fingerprint,
            allowed_cidrs: &cidrs,
        })
        .map_err(|e| ApiError::User(e.to_string()))?;
    tracing::info!(%fingerprint, %name, "agent key added by admin client");
    app.reload_auth();
    Ok(Response::Saved { id })
}

/// 只允許修改 agent 公鑰。
fn require_agent(app: &App, id: i64) -> Result<(), ApiError> {
    match app.store.key_role(id)? {
        Some(Role::Agent) => Ok(()),
        Some(Role::Admin) => Err(ApiError::User(
            "admin keys can only be changed in the server admin TUI".into(),
        )),
        None => Err(ApiError::User("key not found".into())),
    }
}

pub fn update(app: &App, id: i64, name: &str, allowed_cidrs: &[String]) -> ApiResult {
    require_agent(app, id)?;
    let name = validate::name("name", name)?;
    let cidrs = validate::cidrs(allowed_cidrs)?;
    app.store.update_key(id, &name, &cidrs)?;
    app.reload_auth();
    Ok(Response::Ok)
}

pub fn set_enabled(app: &App, id: i64, enabled: bool) -> ApiResult {
    require_agent(app, id)?;
    app.store.set_key_enabled(id, enabled)?;
    tracing::info!(key = id, enabled, "agent key toggled by admin client");
    app.reload_auth();
    Ok(Response::Ok)
}

pub fn delete(app: &App, id: i64) -> ApiResult {
    require_agent(app, id)?;
    app.store.delete_key(id)?;
    tracing::info!(
        key = id,
        "agent key and its records deleted by admin client"
    );
    app.reload_auth();
    Ok(Response::Ok)
}
