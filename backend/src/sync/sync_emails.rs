use crate::jmap_account::AccountId;
use crate::jmap_api::{EmailQuery, JmapApi};
use crate::repo::Repository;
use crate::sync::EmailQueryState;
use anyhow::Context;
use futures::future::{Either, select};
use jmap_client::{DataType, PushObject};
use std::fmt::{Debug, Formatter};
use std::pin::pin;
use tokio::sync::watch;

pub struct WatchEmailSyncCommand {
    pub query_rx: watch::Receiver<EmailQuery>,
    pub state_tx: watch::Sender<EmailQueryState>,
}

impl Debug for WatchEmailSyncCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WatchSyncCommand").finish()
    }
}

pub async fn handle_watch_command(
    repo: &Repository,
    account_id: AccountId,
    jmap_api: &JmapApi,
    WatchEmailSyncCommand {
        mut query_rx,
        state_tx,
    }: WatchEmailSyncCommand,
) -> anyhow::Result<()> {
    struct LastSyncState {
        state: String,
        total: Option<usize>,
    }

    let mut last_sync_state = None::<LastSyncState>;
    let mut push_sub = jmap_api.subscribe_pushes();

    loop {
        let fetch_results = async {
            state_tx.send(EmailQueryState::InProgress)?;
            let query = query_rx.borrow().clone();

            let (updated, destroyed, new_state) = match &last_sync_state {
                Some(state) => {
                    let mut changes = jmap_api.email_changes(state.state.clone()).await?;
                    let new_total = state
                        .total
                        .map(|total| total + changes.created().len() - changes.destroyed().len());
                    let mut created = changes.take_created();
                    created.extend(changes.take_updated());
                    (
                        created,
                        changes.take_destroyed(),
                        LastSyncState {
                            state: changes.take_new_state(),
                            total: new_total,
                        },
                    )
                }

                _ => {
                    let mut resp = jmap_api.query_emails(query.clone()).await?;
                    (
                        resp.take_ids(),
                        vec![],
                        LastSyncState {
                            state: resp.take_query_state(),
                            total: resp.total(),
                        },
                    )
                }
            };

            let updated = repo
                .find_missing_email_ids(account_id, &updated)
                .await
                .context("Failed to check downloaded emails")?;

            if !updated.is_empty() {
                let emails = jmap_api
                    .get_emails(updated.into_iter().collect(), None)
                    .await?
                    .take_list();

                repo.update_emails(account_id, &emails)
                    .await
                    .context("Failed to update emails")?;
            }

            repo.delete_emails(account_id, &destroyed)
                .await
                .context("Failed to delete emails")?;

            anyhow::Ok(new_state)
        };

        match fetch_results.await {
            Ok(new_state) => {
                state_tx.send(EmailQueryState::UpToDate)?;
                last_sync_state.replace(new_state);
            }

            Err(e) => {
                tracing::error!("Error syncing emails: {e:?}");
                state_tx.send(EmailQueryState::Error {
                    details: e.to_string(),
                })?;
            }
        }

        loop {
            match select(pin!(push_sub.recv()), pin!(query_rx.changed())).await {
                Either::Left((Err(_), _)) => {
                    tracing::info!("JMAP API push notification channel closed");
                    return Ok(());
                }

                Either::Left((Ok(push), _))
                    if matches!(push.as_ref(), PushObject::StateChange { changed }
                    if changed.iter().any(|(_, m)| m.contains_key(&DataType::Email))) =>
                {
                    tracing::info!("Emails changed, restarting sync");
                    break;
                }

                Either::Left((Ok(_), _)) => {
                    // Irrelevant push notification
                    continue;
                }

                Either::Right((Err(_), _)) => {
                    tracing::info!("Query channel closed");
                    return Ok(());
                }

                Either::Right((Ok(_), _)) => {
                    tracing::info!("Email query changed, restarting sync");
                    last_sync_state = None;
                    break;
                }
            }
        }
    }
}
