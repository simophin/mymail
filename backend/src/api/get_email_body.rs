use super::ApiState;
use crate::jmap_account::AccountId;
use crate::sync::{FetchEmailDetailsCommand, SyncCommand};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, extract};
use serde::Deserialize;
use tokio::sync::oneshot;

#[derive(Deserialize)]
pub struct Params {
    #[serde(default)]
    pub html: bool,
}

pub async fn get_email_body(
    state: extract::State<ApiState>,
    extract::Path((account_id, email_id)): extract::Path<(AccountId, String)>,
    extract::Query(Params { html }): extract::Query<Params>,
) -> impl IntoResponse {
    let details = match state
        .repo
        .get_email_parts(account_id, &email_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response())
        .and_then(|s| s.ok_or_else(|| (StatusCode::NOT_FOUND, "Email not found").into_response()))
    {
        Ok(details) => details,
        Err(resp) => return resp,
    };

    if details.body_structure().is_none() {
        let Some(sender) = state.sync_command_sender.read().get(&account_id).cloned() else {
            return (StatusCode::NOT_FOUND, "Account not found").into_response();
        };

        let (tx, rx) = oneshot::channel();
        let _ = sender
            .send(SyncCommand::FetchEmailDetails(FetchEmailDetailsCommand {
                email_id,
                callback: tx,
            }))
            .await;

        match rx.await {
            Ok(Ok(email)) => return (StatusCode::OK, Json(email)).into_response(),
            Ok(Err(e)) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Sync task dropped").into_response(),
        }
    }

    (StatusCode::OK, Json(details)).into_response()
}
