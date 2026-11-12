//! 來源端請求：上傳紀錄。

use what_core::UsageRecord;
use whatplus_proto::{MAX_UPLOAD_BATCH, Response};

use super::{ApiError, ApiResult};
use crate::app::App;
use crate::auth::Identity;

pub fn upload(app: &App, who: &Identity, records: Vec<UsageRecord>) -> ApiResult {
    if records.len() > MAX_UPLOAD_BATCH {
        return Err(ApiError::User(format!(
            "batch too large ({} > {MAX_UPLOAD_BATCH})",
            records.len()
        )));
    }
    let accepted = app.store.upsert_records(who.key_id, &records)?;
    tracing::info!(agent = %who.name, accepted, "records uploaded");
    Ok(Response::Uploaded { accepted })
}

pub fn upload_state(app: &App, who: &Identity) -> ApiResult {
    let (latest, records) = app.store.upload_state(who.key_id)?;
    Ok(Response::UploadState { latest, records })
}
