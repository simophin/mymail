use super::EmailQueryState;
use crate::jmap_account::AccountId;
use crate::jmap_api::{EmailQuery, EmailSort, EmailSortColumn, JmapApi};
use crate::repo::Repository;
use anyhow::{Context, bail};
use futures::FutureExt;
use futures::future::{Either, FusedFuture, select, try_join_all};
use itertools::Itertools;
use jmap_client::{DataType, PushObject, email};
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::future::pending;
use std::pin::pin;
use std::sync::Arc;
use tokio::select;
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tracing::instrument;

pub struct WatchMailboxSyncCommand {
    pub mailbox_id: String,
    pub state_tx: watch::Sender<EmailQueryState>,
}

impl Debug for WatchMailboxSyncCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WatchMailboxSyncCommand")
            .field("mailbox_id", &self.mailbox_id)
            .finish()
    }
}

pub async fn handle_watch_mailbox_command(
    WatchMailboxSyncCommand {
        mailbox_id,
        state_tx,
    }: WatchMailboxSyncCommand,
    state: &super::AccountState,
) -> anyhow::Result<()> {
    let (tx, rx) = oneshot::channel();
    state
        .mailbox_watch_request_tx
        .send((mailbox_id, tx))
        .await
        .context("Failed to send mailbox watch request")?;

    let mut rx = rx.await.context("Mailbox watch request cancelled")?;

    loop {
        state_tx.send(rx.borrow().clone())?;
        rx.changed().await?;
    }
}

pub async fn sync_mailboxes(
    repo: &Repository,
    account_id: AccountId,
    jmap_api: &JmapApi,
    mut mailbox_watch_request_rx: mpsc::Receiver<(String, WatchRequest)>,
) -> anyhow::Result<()> {
    let mut sub = repo.subscribe_db_changes();
    let push_notification = jmap_api.subscribe_pushes();

    struct MailboxSyncState<F> {
        watch_request_sender: mpsc::Sender<WatchRequest>,
        worker: F,
    }

    let mut mailbox_workers: HashMap<String, MailboxSyncState<_>> = Default::default();

    loop {
        let mailboxes: HashSet<String> = repo
            .get_mailbox_ids(account_id)
            .await?
            .into_iter()
            .collect();

        // Drop all workers for mailboxes that no longer exist
        mailbox_workers.retain(|mailbox_id, _| mailboxes.contains(mailbox_id));

        // Start workers for new mailboxes
        for mailbox_id in mailboxes {
            if !mailbox_workers.contains_key(&mailbox_id) {
                let (watch_request_sender, watch_request_rx) = mpsc::channel(10);
                mailbox_workers.insert(
                    mailbox_id.clone(),
                    MailboxSyncState {
                        watch_request_sender,
                        worker: Box::pin(sync_mailbox(
                            repo,
                            account_id,
                            mailbox_id,
                            jmap_api,
                            push_notification.resubscribe(),
                            watch_request_rx,
                        ))
                        .fuse(),
                    },
                );
            }
        }

        loop {
            let drive_workers = async {
                while !mailbox_workers.is_empty() {
                    let _ = try_join_all(mailbox_workers.values_mut().map(|w| &mut w.worker)).await;
                    mailbox_workers.retain(|_, value| !value.worker.is_terminated());
                }

                pending::<()>().await;
            };

            select! {
                _ = drive_workers => {}
                r = mailbox_watch_request_rx.recv() => {
                    let Some((mailbox_id, watch_request)) = r else {
                        bail!("Mailbox watch request channel closed unexpectedly");
                    };

                    let Some(worker) = mailbox_workers.get(&mailbox_id) else {
                        tracing::debug!("No worker for mailbox {mailbox_id}, cannot handle watch request");
                        continue;
                    };

                    if let Err(e) = worker.watch_request_sender.try_send(watch_request) {
                        tracing::debug!(?e, "Failed to send watch request to mailbox {mailbox_id} worker, channel full");
                    }
                }
                m = sub.recv() => {
                    match m {
                        Ok(change) if change.tables.contains(&"mailboxes") => {
                            tracing::debug!("Mailbox database changed, updating workers");
                            break;
                        }

                        Ok(_) => continue,
                        Err(e) => {
                            tracing::error!(?e, "Database change subscription error");
                            return Err(e.into());
                        }
                    }
                }
            }
        }
    }
}

pub type WatchRequest = oneshot::Sender<watch::Receiver<EmailQueryState>>;

#[instrument(
    skip(repo, jmap_api, email_notification, watcher_requests),
    level = "info"
)]
pub async fn sync_mailbox(
    repo: &Repository,
    account_id: AccountId,
    mailbox_id: String,
    jmap_api: &JmapApi,
    mut email_notification: broadcast::Receiver<Arc<PushObject>>,
    mut watcher_requests: mpsc::Receiver<WatchRequest>,
) -> anyhow::Result<()> {
    let (state_tx, _state_rx) = watch::channel(EmailQueryState::NotStarted);

    let mut wait_for_push_notification = async || {
        if state_tx.receiver_count() > 1 {
            loop {
                match email_notification.recv().await?.as_ref() {
                    PushObject::StateChange { changed }
                        if changed
                            .values()
                            .any(|m| m.get(&DataType::Mailbox) == Some(&mailbox_id)) =>
                    {
                        break Ok(());
                    }

                    _ => continue,
                }
            }
        } else {
            futures::future::pending::<()>().await;
            Ok(())
        }
    };

    loop {
        match select(
            pin!(wait_for_push_notification()),
            pin!(watcher_requests.recv()),
        )
        .await
        {
            Either::Left((Ok(_), _)) => {
                tracing::debug!("Received push notification");
                if state_tx.receiver_count() < 2 {
                    tracing::info!("No active watchers, not syncing");
                    continue;
                }
            }

            Either::Left((Err(e), _)) => {
                tracing::error!(?e, "Error receiving push notification");
                return Err(e);
            }

            Either::Right((Some(watch_request), _)) => {
                if watch_request.send(state_tx.subscribe()).is_ok() {
                    tracing::info!("Received a watcher request for mailbox");
                } else {
                    continue;
                }
            }

            Either::Right((None, _)) => {
                tracing::debug!("Watcher requests channel closed, stop syncing",);
            }
        }

        tracing::info!("Start syncing mailbox");

        let _ = state_tx.send(EmailQueryState::InProgress);

        if let Err(e) = sync_mailbox_once(repo, account_id, &mailbox_id, jmap_api).await {
            tracing::error!(?e, "Sync failed");
            let _ = state_tx.send(EmailQueryState::Error {
                details: format!("Sync failed: {e:?}"),
            });
            continue;
        }

        let _ = state_tx.send(EmailQueryState::UpToDate);
    }
}

#[instrument(skip(repo, jmap_api), ret, level = "debug")]
pub async fn sync_mailbox_once(
    repo: &Repository,
    account_id: AccountId,
    mailbox_id: &str,
    jmap_api: &JmapApi,
) -> anyhow::Result<()> {
    let mut updated = vec![];
    let mut deleted = vec![];
    let new_state: String;
    match repo
        .get_mailbox_email_sync_state(account_id, &mailbox_id)
        .await
        .context("Error getting mailbox email sync state")?
    {
        Some(last_state) => loop {
            let mut changes = jmap_api
                .email_changes(last_state.clone())
                .await
                .context("Error updating email changes")?;
            updated.extend(changes.take_updated());
            updated.extend(changes.take_created());
            deleted.extend(changes.take_destroyed());

            if !changes.has_more_changes() {
                new_state = changes.take_new_state();
                break;
            }
        },

        None => {
            let mut emails = jmap_api
                .query_emails(EmailQuery {
                    anchor_id: None,
                    mailbox_id: Some(mailbox_id.to_string()),
                    search_keyword: None,
                    sorts: vec![EmailSort {
                        column: EmailSortColumn::Date,
                        asc: false,
                    }],
                    limit: None,
                })
                .await
                .context("Error querying emails")?;

            new_state = emails.take_query_state();
            updated.extend(emails.take_ids());
        }
    }

    for chunk in &updated.into_iter().chunks(200) {
        let emails = jmap_api
            .get_emails(
                chunk.collect(),
                None,
                // Some(vec![
                //     email::Property::ReceivedAt,
                //     email::Property::ThreadId,
                //     email::Property::MailboxIds,
                // ]),
            )
            .await
            .context("Error getting emails")?
            .take_list();

        tracing::debug!("Adding {} emails", emails.len());

        repo.update_emails(account_id, &emails)
            .await
            .context("Error updating emails")?;
    }

    if !deleted.is_empty() {
        tracing::debug!("Deleting {} emails", deleted.len());

        repo.delete_emails(account_id, &deleted)
            .await
            .context("Error deleting emails")?;
    }

    repo.set_mailbox_email_sync_state(account_id, mailbox_id, &new_state)
        .await
        .context("Error setting mailbox email sync state")?;

    Ok(())
}
