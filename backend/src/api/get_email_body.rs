use super::ApiState;
use crate::jmap_account::AccountId;
use crate::sync::SyncCommand;
use crate::util::http_error::{AnyhowHttpError, HttpResult};
use anyhow::Context;
use axum::{Json, extract};
use jmap_client::email::Email;
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
) -> HttpResult<Json<Email>> {
    if let Some(details) = state
        .repo
        .get_email_parts(account_id, &email_id)
        .await
        .into_internal_error_result()?
    {
        return Ok(Json(details));
    }

    let sender = state
        .account_states
        .read()
        .get(&account_id)
        .context("Account not found")
        .into_not_found_error_result()?
        .clone();

    // let (tx, rx) = oneshot::channel();
    // let _ = sender
    //     .send(SyncCommand::FetchEmailDetails(FetchBlobCommand {
    //         email_id,
    //         callback: tx,
    //     }))
    //     .await;
    //
    // rx.await
    //     .context("Error waiting for sync command")
    //     .into_internal_error_result()?
    //     .context("Error fetching email details")
    //     .into_internal_error_result()
    //     .map(Json)

    todo!()
}
