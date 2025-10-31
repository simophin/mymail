use crate::jmap_account::AccountId;
use crate::jmap_sync::SyncCommand;
use crate::repo::Repository;
use anyhow::Context;
use axum::body::Body;
use axum::routing::{get, post};
use parking_lot::RwLock;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

mod sync_mail;
mod watch_mail;
mod watch_mailboxes;

#[derive(Clone)]
pub struct ApiState {
    pub repo: Arc<Repository>,
    pub sync_command_sender: Arc<RwLock<HashMap<AccountId, mpsc::Sender<SyncCommand>>>>,
}

pub fn build_api_router() -> axum::Router<ApiState> {
    use axum::Router;

    Router::new()
        .route("/mails/{account_id}", post(watch_mail::watch_mail))
        .route("/mails/sync/{account_id}", post(sync_mail::sync_mail))
        .route(
            "/mailboxes/{account_id}",
            get(watch_mailboxes::watch_mailboxes),
        )
}

fn query_with_db_changes<T, F, Fut>(
    repo: Arc<Repository>,
    tables: &'static [&'static str],
    query: F,
) -> Body
where
    T: Serialize + 'static,
    F: FnMut(Arc<Repository>) -> Fut + Send + Clone + 'static,
    Fut: Future<Output = anyhow::Result<T>> + Send + 'static,
{
    use axum::body::Body;
    use futures::{StreamExt, TryStreamExt, stream};
    use std::future::ready;
    use tokio_stream::wrappers::BroadcastStream;

    Body::from_stream(
        stream::select(
            BroadcastStream::new(repo.subscribe_db_changes()).filter_map(move |item| {
                ready(match item {
                    Ok(changes) if tables.iter().any(|t| changes.tables.contains(t)) => {
                        Some(Ok(()))
                    }
                    Ok(_) => None,
                    Err(e) => Some(Err(e)),
                })
            }),
            stream::iter([Ok(())]),
        )
        .map_err(|e| anyhow::format_err!("DB subscription error: {e:?}"))
        .and_then(move |_| {
            let repo = repo.clone();
            let mut query = query.clone();
            async move {
                let r = query(repo).await;
                if let Err(e) = &r {
                    tracing::error!(?e, "Error executing query");
                }
                r
            }
        })
        .map(|r| match r {
            Ok(data) => {
                let mut resp =
                    serde_json::to_string(&data).context("Error serializing response")?;
                resp.push('\n');
                Ok(resp)
            }
            Err(e) => Err(e),
        }),
    )
}
