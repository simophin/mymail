use super::ApiState;
use crate::jmap_account::AccountId;
use crate::util::http_error::{AnyhowHttpError, HttpResult};
use anyhow::Context;
use axum::Json;
use axum::body::Bytes;
use axum::extract;
use serde::Deserialize;
use serde::Serialize;

#[derive(Deserialize)]
pub struct Params {
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
}

#[derive(Serialize)]
pub struct UploadResponse {
    pub blob_id: String,
}

pub async fn upload_blob(
    state: extract::State<ApiState>,
    extract::Path(account_id): extract::Path<AccountId>,
    extract::Query(Params { mime_type }): extract::Query<Params>,
    body: Bytes,
) -> HttpResult<Json<UploadResponse>> {
    let api = state
        .account_states
        .read()
        .get(&account_id)
        .map(|s| s.jmap_api.clone())
        .context("Account not found")
        .into_not_found_error_result()?;

    let blob_id = api
        .upload_blob(body.to_vec(), mime_type)
        .await
        .context("Failed to upload blob")
        .into_internal_error_result()?;

    Ok(Json(UploadResponse { blob_id }))
}
