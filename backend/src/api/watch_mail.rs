use super::ApiState;
use crate::jmap_account::AccountId;
use crate::jmap_api::EmailQuery;
use crate::jmap_repo::{EmailDbQuery, JmapRepositoryExt};
use crate::jmap_sync::{EmailQueryState, SyncCommand, WatchSyncCommand};
use axum::extract;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use std::sync::Arc;
use tokio::sync::watch;

pub async fn watch_mail(
    account_id: Path<AccountId>,
    state: extract::State<ApiState>,
    query: extract::Query<EmailDbQuery>,
) -> impl IntoResponse {
    let (_, query_rx) = watch::channel(None);
    let (state_tx, state_rx) = watch::channel(EmailQueryState::NotStarted);

    let Some(command_sender) = state.sync_command_sender.read().get(&account_id).cloned() else {
        return (StatusCode::NOT_FOUND, "Account not found").into_response();
    };

    if let Err(e) = command_sender
        .send(SyncCommand::Watch(WatchSyncCommand {
            query: query_rx,
            state_tx,
        }))
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error sending watch command: {e:?}"),
        )
            .into_response();
    }

    let query = Arc::new(query.0.clone());

    super::query_with_db_changes(state.repo.clone(), &["emails"], move |repo| {
        let account_id = account_id.0;
        let query = query.clone();
        async move { repo.get_emails(account_id, &query).await }
    })
    .into_response()
}
