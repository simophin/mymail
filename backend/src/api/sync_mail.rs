use super::ApiState;
use crate::jmap_account::AccountId;
use crate::jmap_api::EmailQuery;
use crate::jmap_sync::{SyncCommand, WatchSyncCommand};
use anyhow::Context;
use axum::body::Body;
use axum::extract;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use futures::TryStreamExt;
use tokio::sync::oneshot;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::WatchStream;

pub async fn sync_mail(
    state: extract::State<ApiState>,
    account_id: extract::Path<AccountId>,
    upgrade: extract::ws::WebSocketUpgrade,
) -> impl IntoResponse {
    let Some(tx) = state.sync_command_sender.read().get(&account_id.0).cloned() else {
        return (
            StatusCode::NOT_FOUND,
            format!("Account {} not found", account_id.0),
        )
            .into_response();
    };

    upgrade.on_upgrade(move |websocket| async move {})
}
