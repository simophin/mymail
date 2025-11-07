use crate::repo::Repository;
use anyhow::Context;
use axum::extract::WebSocketUpgrade;
use axum::extract::ws::Message;
use axum::response::IntoResponse;
use futures::{FutureExt, StreamExt, TryStream, TryStreamExt};
use serde::Serialize;
use std::sync::Arc;

pub fn db_stream<T, F, Fut>(
    repo: Arc<Repository>,
    tables: &'static [&'static str],
    query: F,
) -> impl TryStream<Ok = String, Error = anyhow::Error> + Send + 'static
where
    T: Serialize + 'static,
    F: Fn(Arc<Repository>) -> Fut + Send + Clone + 'static,
    Fut: Future<Output = anyhow::Result<T>> + Send + 'static,
{
    use futures::{StreamExt, TryStreamExt, stream};
    use std::future::ready;
    use tokio_stream::wrappers::BroadcastStream;

    stream::select(
        BroadcastStream::new(repo.subscribe_db_changes()).filter_map(move |item| {
            ready(match item {
                Ok(changes) if tables.iter().any(|t| changes.tables.contains(t)) => Some(Ok(())),
                Ok(_) => None,
                Err(e) => Some(Err(e)),
            })
        }),
        stream::iter([Ok(())]),
    )
    .map_err(|e| anyhow::format_err!("DB subscription error: {e:?}"))
    .and_then(move |_| {
        let repo = repo.clone();
        let query = query.clone();
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
            let mut resp = serde_json::to_string(&data).context("Error serializing response")?;
            resp.push('\n');
            Ok(resp)
        }
        Err(e) => Err(e),
    })
}

pub fn websocket_db_stream<T, F, Fut>(
    upgrade: WebSocketUpgrade,
    repo: Arc<Repository>,
    tables: &'static [&'static str],
    query: F,
) -> impl IntoResponse
where
    T: Serialize + 'static,
    F: Fn(Arc<Repository>) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = anyhow::Result<T>> + Send + 'static,
{
    upgrade.on_upgrade(move |ws| {
        let repo = repo.clone();
        db_stream(repo, tables, query)
            .map_ok(Message::text)
            .map_err(|e| axum::Error::new(e))
            .forward(ws)
            .map(|r| {
                if let Err(e) = r {
                    tracing::error!(?e, "Error in websocket_db_stream");
                }
            })
    })
}
