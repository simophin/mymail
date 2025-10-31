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
    query: extract::Json<EmailQuery>,
) -> impl IntoResponse {
    let state_rx = match async {
        let channel = state
            .sync_command_sender
            .write()
            .get_mut(&account_id.0)
            .cloned()
            .context("Account not found")?;

        let (callback, callback_rx) = oneshot::channel();

        channel
            .send(SyncCommand::Watch(WatchSyncCommand {
                query: query.0,
                callback,
            }))
            .await
            .context("Error sending sync command")?;

        callback_rx
            .await
            .context("Error receiving sync command response")
    }
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(?e, "Error initiating mail sync");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Error initiating mail sync: {e}"),
            )
                .into_response();
        }
    };

    Body::from_stream(
        WatchStream::new(state_rx)
            .map(|state| serde_json::to_string(&state))
            .map_ok(|mut r| {
                r.push('\n');
                r
            }),
    )
    .into_response()
}
