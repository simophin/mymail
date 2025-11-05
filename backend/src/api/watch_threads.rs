use crate::jmap_account::AccountId;
use crate::jmap_api::{EmailQuery, EmailSort, EmailSortColumn};
use crate::jmap_sync::{SyncCommand, WatchSyncCommand};
use anyhow::Context;
use axum::extract;
use axum::response::IntoResponse;
use serde::Deserialize;
use tokio::sync::{oneshot, watch};

#[derive(Deserialize)]
pub struct ThreadQuery {
    mailbox_id: String,
    limit: usize,
    offset: usize,
}

pub async fn watch_threads(
    state: extract::State<super::ApiState>,
    account_id: extract::Path<AccountId>,
    extract::Query(ThreadQuery {
        mailbox_id,
        limit,
        offset,
    }): extract::Query<ThreadQuery>,
) -> impl IntoResponse {
    let (sync_tx, mut sync_rx) = watch::channel(None::<EmailQuery>);
    let Some(command_sender) = state.sync_command_sender.write().get(&account_id).cloned() else {
        return (axum::http::StatusCode::NOT_FOUND, "Account not found").into_response();
    };

    tokio::spawn(async move {
        let mut sync_state = None;
        loop {
            let query = sync_rx.borrow().clone();
            if let Some(query) = query {
                let (callback, callback_rx) = oneshot::channel();
                command_sender
                    .send(SyncCommand::Watch(WatchSyncCommand { query, callback }))
                    .await
                    .context("Error sending account command")?;

                sync_state.replace(
                    callback_rx
                        .await
                        .context("Error receiving state callback")?,
                );
            }

            if sync_rx.changed().await.is_err() {
                break;
            }
        }

        anyhow::Ok(())
    });

    super::query_with_db_changes(state.repo.clone(), &["emails"], move |repo| {
        let account_id = account_id.0;
        let mailbox_id = mailbox_id.clone();
        let sync_tx = sync_tx.clone();

        async move {
            let r = repo
                .get_threads(account_id, &mailbox_id, offset, limit)
                .await;

            if let Ok(threads) = &r {
                if let Some(newest_email) = threads
                    .iter()
                    .flat_map(|t| t.emails.iter())
                    .max_by_key(|e| e.received_at().unwrap_or_default())
                    .and_then(|e| e.id())
                {
                    let limit = limit * 10;

                    sync_tx.send_if_modified(|old| match old {
                        Some(q)
                            if q.anchor_id.as_ref().map(String::as_str) == Some(newest_email)
                                && limit == q.limit =>
                        {
                            false
                        }

                        _ => {
                            *old = Some(EmailQuery {
                                anchor_id: Some(newest_email.to_string()),
                                limit,
                                mailbox_id: Some(mailbox_id.clone()),
                                search_keyword: None,
                                sorts: vec![EmailSort {
                                    column: EmailSortColumn::Date,
                                    asc: false,
                                }],
                            });
                            true
                        }
                    });
                }
            };

            r
        }
    })
    .into_response()
}
