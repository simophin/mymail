use crate::jmap_account::AccountId;
use crate::sync::{EmailQueryState, SyncCommand, WatchMailboxSyncCommand};
use axum::extract;
use axum::extract::ws::Message;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use tokio::sync::watch;

pub async fn sync_mailbox(
    state: extract::State<super::ApiState>,
    extract::Path((account_id, mailbox_id)): extract::Path<(AccountId, String)>,
    upgrade: extract::WebSocketUpgrade,
) -> impl IntoResponse {
    let Some(sender) = state.sync_command_sender.read().get(&account_id).cloned() else {
        return (StatusCode::NOT_FOUND, "Account not found").into_response();
    };

    let (state_tx, mut state_rx) = watch::channel(EmailQueryState::NotStarted);

    if let Err(e) = sender
        .send(SyncCommand::WatchMailbox(WatchMailboxSyncCommand {
            mailbox_id,
            state_tx,
        }))
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to send sync command: {:?}", e),
        )
            .into_response();
    };

    upgrade
        .on_upgrade(async move |mut ws| {
            while state_rx.changed().await.is_ok() {
                let text = serde_json::to_string(&*state_rx.borrow()).unwrap();
                if let Err(e) = ws.send(Message::text(text)).await {
                    tracing::error!(?e, "WebSocket send error");
                    break;
                }
            }
        })
        .into_response()
}
