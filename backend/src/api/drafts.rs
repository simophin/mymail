use super::ApiState;
use crate::jmap_account::AccountId;
use crate::jmap_api::{EmailDraft, JmapApi};
use crate::repo::{DraftRecord, DraftRepositoryExt, Repository};
use crate::util::http_error::{AnyhowHttpError, HttpResult};
use anyhow::Context;
use axum::Json;
use axum::extract;
use axum::http::StatusCode;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct DraftResponse {
    pub id: String,
    pub jmap_email_id: Option<String>,
    pub data: EmailDraft,
    pub updated_at: i64,
    /// Whether the draft has been synced to the JMAP server.
    pub synced: bool,
}

impl From<DraftRecord> for DraftResponse {
    fn from(r: DraftRecord) -> Self {
        let synced = r.jmap_email_id.is_some();
        DraftResponse {
            id: r.id,
            jmap_email_id: r.jmap_email_id,
            data: r.data,
            updated_at: r.updated_at,
            synced,
        }
    }
}

// ── Create ────────────────────────────────────────────────────────────────────

pub async fn create_draft(
    state: extract::State<ApiState>,
    extract::Path(account_id): extract::Path<AccountId>,
    Json(draft): Json<EmailDraft>,
) -> HttpResult<(StatusCode, Json<DraftResponse>)> {
    // 1. Save to local DB immediately — this always succeeds even when offline.
    let record = state
        .repo
        .create_draft(account_id, &draft)
        .await
        .context("Failed to save draft")
        .into_internal_error_result()?;

    let draft_id = record.id.clone();
    let response = DraftResponse::from(record);

    // 2. Attempt to sync to JMAP server in the background; update jmap_email_id on success.
    if let Some(api) = state
        .account_states
        .read()
        .get(&account_id)
        .map(|s| s.jmap_api.clone())
    {
        tokio::spawn(sync_draft_create(
            api,
            state.repo.clone(),
            account_id,
            draft_id,
            draft,
        ));
    }

    Ok((StatusCode::CREATED, Json(response)))
}

async fn sync_draft_create(
    api: Arc<JmapApi>,
    repo: Arc<Repository>,
    account_id: i64,
    draft_id: String,
    draft: EmailDraft,
) {
    match api.create_jmap_draft(draft).await {
        Ok(jmap_email_id) => {
            if let Err(e) = repo
                .set_draft_jmap_id(account_id, &draft_id, &jmap_email_id)
                .await
            {
                tracing::warn!(?e, "Failed to store jmap_email_id for draft {draft_id}");
            } else {
                tracing::debug!(draft_id, jmap_email_id, "Draft synced to JMAP server");
            }
        }
        Err(e) => {
            tracing::warn!(?e, "Failed to sync draft {draft_id} to JMAP server (will retry on next save)");
        }
    }
}

// ── Update ────────────────────────────────────────────────────────────────────

pub async fn update_draft(
    state: extract::State<ApiState>,
    extract::Path((account_id, draft_id)): extract::Path<(AccountId, String)>,
    Json(draft): Json<EmailDraft>,
) -> HttpResult<Json<DraftResponse>> {
    // Verify the draft exists and fetch the current jmap_email_id.
    let existing = state
        .repo
        .get_draft(account_id, &draft_id)
        .await
        .context("Failed to query draft")
        .into_internal_error_result()?
        .context("Draft not found")
        .into_not_found_error_result()?;

    let old_jmap_id = existing.jmap_email_id;

    // 1. Save updated data to local DB.
    state
        .repo
        .update_draft_data(account_id, &draft_id, &draft)
        .await
        .context("Failed to update draft")
        .into_internal_error_result()?;

    // Re-read to get the fresh updated_at.
    let record = state
        .repo
        .get_draft(account_id, &draft_id)
        .await
        .context("Failed to re-read draft")
        .into_internal_error_result()?
        .context("Draft disappeared after update")
        .into_internal_error_result()?;

    let response = DraftResponse::from(record);

    // 2. Sync to JMAP server in the background.
    //    Because JMAP email bodies are immutable, "update" = create new + destroy old.
    if let Some(api) = state
        .account_states
        .read()
        .get(&account_id)
        .map(|s| s.jmap_api.clone())
    {
        tokio::spawn(sync_draft_update(
            api,
            state.repo.clone(),
            account_id,
            draft_id,
            draft,
            old_jmap_id,
        ));
    }

    Ok(Json(response))
}

async fn sync_draft_update(
    api: Arc<JmapApi>,
    repo: Arc<Repository>,
    account_id: i64,
    draft_id: String,
    draft: EmailDraft,
    old_jmap_id: Option<String>,
) {
    // Create the new JMAP email first, then destroy the old one.
    // Doing it in this order means we never lose the draft if the destroy fails.
    match api.create_jmap_draft(draft).await {
        Ok(new_jmap_id) => {
            if let Err(e) = repo
                .set_draft_jmap_id(account_id, &draft_id, &new_jmap_id)
                .await
            {
                tracing::warn!(?e, "Failed to store updated jmap_email_id for draft {draft_id}");
            }

            // Clean up the old JMAP email (best-effort — a stale copy in Drafts is not critical).
            if let Some(old_id) = old_jmap_id {
                if let Err(e) = api.delete_jmap_email(old_id.clone()).await {
                    tracing::warn!(?e, old_id, "Failed to delete superseded JMAP draft email");
                }
            }
        }
        Err(e) => {
            // Creation failed — clear the stale jmap_email_id so the next save retries from scratch.
            tracing::warn!(?e, "Failed to sync updated draft {draft_id} to JMAP server");
            if let Some(_) = old_jmap_id {
                let _ = repo.clear_draft_jmap_id(account_id, &draft_id).await;
            }
        }
    }
}

// ── Delete ────────────────────────────────────────────────────────────────────

pub async fn delete_draft(
    state: extract::State<ApiState>,
    extract::Path((account_id, draft_id)): extract::Path<(AccountId, String)>,
) -> HttpResult<StatusCode> {
    let existing = state
        .repo
        .get_draft(account_id, &draft_id)
        .await
        .context("Failed to query draft")
        .into_internal_error_result()?
        .context("Draft not found")
        .into_not_found_error_result()?;

    let jmap_email_id = existing.jmap_email_id;

    // 1. Delete from local DB first.
    state
        .repo
        .delete_draft(account_id, &draft_id)
        .await
        .context("Failed to delete draft")
        .into_internal_error_result()?;

    // 2. Clean up the JMAP server copy in the background.
    if let Some(jmap_id) = jmap_email_id {
        if let Some(api) = state
            .account_states
            .read()
            .get(&account_id)
            .map(|s| s.jmap_api.clone())
        {
            tokio::spawn(async move {
                if let Err(e) = api.delete_jmap_email(jmap_id.clone()).await {
                    tracing::warn!(?e, jmap_id, "Failed to delete JMAP draft on discard");
                }
            });
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

// ── List ──────────────────────────────────────────────────────────────────────

pub async fn list_drafts(
    state: extract::State<ApiState>,
    extract::Path(account_id): extract::Path<AccountId>,
) -> HttpResult<Json<Vec<DraftResponse>>> {
    let drafts = state
        .repo
        .list_drafts(account_id)
        .await
        .context("Failed to list drafts")
        .into_internal_error_result()?;

    Ok(Json(drafts.into_iter().map(DraftResponse::from).collect()))
}
