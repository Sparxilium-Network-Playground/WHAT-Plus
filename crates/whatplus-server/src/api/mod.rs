//! 請求處理。依連線公鑰的角色決定可執行的請求（agent 只能上傳）。

mod admin_keys;
mod admin_reports;
mod agent;

use whatplus_proto::{Request, Response, Role};

use crate::app::App;
use crate::auth::Identity;

/// 給 log 用的請求名稱（不含內容）。
pub fn request_name(req: &Request) -> &'static str {
    match req {
        Request::Hello { .. } => "hello",
        Request::Upload { .. } => "upload",
        Request::UploadState => "upload_state",
        Request::Status => "status",
        Request::ListKeys => "list_keys",
        Request::AddAgentKey { .. } => "add_agent_key",
        Request::UpdateAgentKey { .. } => "update_agent_key",
        Request::SetAgentKeyEnabled { .. } => "set_agent_key_enabled",
        Request::DeleteAgentKey { .. } => "delete_agent_key",
        Request::ListDiscordChannels => "list_discord_channels",
        Request::ListDestinations => "list_destinations",
        Request::SaveDestination { .. } => "save_destination",
        Request::DeleteDestination { .. } => "delete_destination",
        Request::TestDestination { .. } => "test_destination",
        Request::ListSchedules => "list_schedules",
        Request::SaveSchedule { .. } => "save_schedule",
        Request::DeleteSchedule { .. } => "delete_schedule",
        Request::PreviewReport { .. } => "preview_report",
        Request::SendReport { .. } => "send_report",
    }
}

fn allowed(role: Role, req: &Request) -> bool {
    match req {
        Request::Hello { .. } => true,
        Request::Upload { .. } | Request::UploadState => role == Role::Agent,
        _ => role == Role::Admin,
    }
}

pub async fn handle(app: &App, who: &Identity, req: Request) -> Response {
    if !allowed(who.role, &req) {
        return Response::error("this key is not allowed to perform this request");
    }
    let result = match req {
        Request::Hello { .. } => Ok(Response::error("already connected")),
        Request::Upload { records } => agent::upload(app, who, records),
        Request::UploadState => agent::upload_state(app, who),
        Request::Status => admin_reports::status(app),
        Request::ListKeys => admin_keys::list(app),
        Request::AddAgentKey { key } => admin_keys::add(app, key),
        Request::UpdateAgentKey {
            id,
            name,
            allowed_cidrs,
        } => admin_keys::update(app, id, &name, &allowed_cidrs),
        Request::SetAgentKeyEnabled { id, enabled } => admin_keys::set_enabled(app, id, enabled),
        Request::DeleteAgentKey { id } => admin_keys::delete(app, id),
        Request::ListDiscordChannels => admin_reports::guilds(app).await,
        Request::ListDestinations => admin_reports::destinations(app),
        Request::SaveDestination { destination } => {
            admin_reports::save_destination(app, &destination)
        }
        Request::DeleteDestination { id } => admin_reports::delete_destination(app, id),
        Request::TestDestination { id } => admin_reports::test_destination(app, id).await,
        Request::ListSchedules => admin_reports::schedules(app),
        Request::SaveSchedule { schedule } => admin_reports::save_schedule(app, &schedule),
        Request::DeleteSchedule { id } => admin_reports::delete_schedule(app, id),
        Request::PreviewReport {
            range,
            content,
            language,
        } => admin_reports::preview(app, range, content, language),
        Request::SendReport {
            range,
            content,
            destination_ids,
        } => admin_reports::send(app, range, content, &destination_ids).await,
    };
    result.unwrap_or_else(|e| e.into_response())
}

/// 請求失敗：`User` 的訊息回給客戶端；`Internal` 只寫 log，客戶端收到通用訊息。
pub enum ApiError {
    User(String),
    Internal(anyhow::Error),
}

impl ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::User(m) => Response::error(m),
            ApiError::Internal(e) => {
                tracing::error!("request failed: {e:#}");
                Response::error("internal server error (see server log)")
            }
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError::Internal(e)
    }
}

impl From<String> for ApiError {
    fn from(m: String) -> Self {
        ApiError::User(m)
    }
}

pub type ApiResult = Result<Response, ApiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_cannot_use_admin_requests() {
        assert!(allowed(Role::Agent, &Request::UploadState));
        assert!(!allowed(Role::Agent, &Request::ListKeys));
        assert!(!allowed(Role::Admin, &Request::Upload { records: vec![] }));
        assert!(allowed(Role::Admin, &Request::Status));
    }
}
