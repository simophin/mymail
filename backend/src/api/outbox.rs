use super::ApiState;
use crate::jmap_account::AccountId;
use crate::repo::DraftRepositoryExt;
use crate::util::http_error::{AnyhowHttpError, HttpResult};
use anyhow::Context;
use axum::Json;
use axum::extract;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct OutboxRequest {
    pub draft_id: String,
    /// The ID of the Sent mailbox. The email will be placed here on the JMAP server.
    pub sent_mailbox_id: String,
}

#[derive(Serialize)]
pub struct SendResponse {
    pub email_id: String,
}

pub async fn send_mail(
    state: extract::State<ApiState>,
    extract::Path(account_id): extract::Path<AccountId>,
    Json(OutboxRequest {
        draft_id,
        sent_mailbox_id,
    }): Json<OutboxRequest>,
) -> HttpResult<(StatusCode, Json<SendResponse>)> {
    // Load the draft — guaranteed to exist locally even if never synced to JMAP.
    let mut draft = state
        .repo
        .get_draft(account_id, &draft_id)
        .await
        .context("Failed to load draft")
        .into_internal_error_result()?
        .context("Draft not found")
        .into_not_found_error_result()?;

    let api = state
        .account_states
        .read()
        .get(&account_id)
        .map(|s| s.jmap_api.clone())
        .context("Account not found")
        .into_not_found_error_result()?;

    let identity_id = draft.data.identity_id.clone();
    let old_jmap_id = draft.jmap_email_id.take();

    // Override the mailbox to the Sent folder — the draft was stored in Drafts.
    draft.data.mailbox_id = sent_mailbox_id;

    // Create a fresh outgoing email (no $draft keyword, Sent mailbox).
    let email_id = api
        .create_email(draft.data)
        .await
        .context("Failed to create email for sending")
        .into_internal_error_result()?;

    // Submit the email via JMAP EmailSubmission.
    api.submit_email(email_id.clone(), identity_id)
        .await
        .context("Failed to submit email")
        .into_internal_error_result()?;

    // Delete the local draft record now that sending succeeded.
    state
        .repo
        .delete_draft(account_id, &draft_id)
        .await
        .context("Failed to delete draft after send")
        .into_internal_error_result()?;

    // Remove the JMAP draft copy in the background (best-effort).
    if let Some(jmap_id) = old_jmap_id {
        tokio::spawn(async move {
            if let Err(e) = api.delete_jmap_email(jmap_id.clone()).await {
                tracing::warn!(?e, jmap_id, "Failed to delete JMAP draft after send");
            }
        });
    }

    Ok((StatusCode::CREATED, Json(SendResponse { email_id })))
}
