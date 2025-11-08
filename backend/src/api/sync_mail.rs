use super::ApiState;
use crate::jmap_account::AccountId;
use crate::jmap_api::EmailQuery;
use crate::sync::{EmailQueryState, SyncCommand, WatchEmailSyncCommand};
use anyhow::Context;
use axum::extract;
use axum::extract::ws::{Message, WebSocket};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::de::DeserializeOwned;
use tokio::select;
use tokio::sync::{mpsc, watch};

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

    upgrade.on_upgrade(async move |mut websocket| {
        if let Err(e) = handle_sync_mail_websocket(&mut websocket, &tx).await {
            tracing::error!(?e, "Error in sync_mail websocket");
        }
    })
}

async fn handle_sync_mail_websocket(
    websocket: &mut WebSocket,
    command_sender: &mpsc::Sender<SyncCommand>,
) -> anyhow::Result<()> {
    // Wait for the first command to set up the watch
    let initial_query: EmailQuery = receive_json(websocket)
        .await
        .context("Failed to receive initial email query")?;

    let (query_tx, query_rx) = watch::channel(initial_query);
    let (state_tx, mut state_rx) = watch::channel(EmailQueryState::NotStarted);

    command_sender
        .send(SyncCommand::WatchEmails(WatchEmailSyncCommand {
            query_rx,
            state_tx,
        }))
        .await
        .context("Failed to send watch sync command")?;

    loop {
        select! {
            new_query = receive_json::<EmailQuery>(websocket) => {
                let query = new_query.context("Failed to receive updated email query")?;
                tracing::debug!(?query, "New email query");
                query_tx
                    .send(query)
                    .context("Failed to send updated email query")?;
            }

            changed = state_rx.changed() => {
                changed.context("Failed to receive email query state change")?;
                let state = serde_json::to_string(&*state_rx.borrow())
                    .context("Failed to serialize email query state")?;
                tracing::debug!(?state, "Email sync state");
                websocket
                    .send(Message::text(state))
                    .await
                    .context("Failed to send email query state over websocket")?;
            }
        }
    }
}

async fn receive_json<T: DeserializeOwned>(ws: &mut WebSocket) -> anyhow::Result<T> {
    loop {
        let msg = ws
            .recv()
            .await
            .context("Failed to receive message from websocket")?
            .context("Websocket closed unexpectedly")?;

        match msg {
            Message::Text(text) => {
                let data = serde_json::from_str(&text)
                    .context("Failed to deserialize JSON message from websocket")?;
                return Ok(data);
            }
            _ => continue,
        }
    }
}
