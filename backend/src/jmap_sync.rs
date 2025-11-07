use crate::future_set::FutureWorkers;
use crate::jmap_account::{AccountId, AccountRepositoryExt, Credentials};
use crate::jmap_api::{EmailQuery, JmapApi};
use crate::repo::Repository;
use anyhow::{Context, bail};
use futures::future::{Either, select};
use jmap_client::client::Client;
use jmap_client::{DataType, PushObject};
use serde::Serialize;
use std::fmt::Debug;
use std::pin::pin;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::try_join;
use tracing::instrument;
use url::Url;

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "state")]
pub enum EmailQueryState {
    NotStarted,
    InProgress,
    Error { details: String },
    UpToDate,
}

pub struct WatchSyncCommand {
    pub query_rx: watch::Receiver<EmailQuery>,
    pub state_tx: watch::Sender<EmailQueryState>,
}

pub enum SyncCommand {
    Watch(WatchSyncCommand),
}

impl Debug for SyncCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncCommand::Watch { .. } => f.debug_struct("Watch").finish(),
        }
    }
}

pub async fn run_jmap_sync(
    repo: &Repository,
    account_id: AccountId,
    mut sync_commands: mpsc::Receiver<SyncCommand>,
) -> anyhow::Result<()> {
    let account = repo
        .get_account(account_id)
        .await?
        .context("Account not found")?;

    let url = Url::parse(&account.server_url).context("Failed to parse JMAP server url")?;
    let credentials = match &account.credentials {
        Credentials::Basic { username, password } => (username.as_str(), password.as_str()),
    };

    let client = Client::new()
        .follow_redirects([url.host_str().unwrap()])
        .credentials(credentials)
        .connect(&account.server_url)
        .await
        .context("Failed to connect to JMAP server")?;

    tracing::info!("Connected to JMAP server");

    let (jmap_api, jmap_api_worker) = JmapApi::new(client).await?;
    let sync_account = sync_account(repo, account_id, &jmap_api);
    let handle_sync_commands = async {
        let mut sync_command_workers = FutureWorkers::new();

        loop {
            match select(pin!(&mut sync_command_workers), pin!(sync_commands.recv())).await {
                Either::Left((_, _)) => {
                    // One of the sync command workers finished
                }

                Either::Right((Some(sync_command), _)) => {
                    tracing::info!("Handling sync command: {sync_command:?}");

                    let worker = handle_sync_command(repo, account_id, &jmap_api, sync_command);
                    sync_command_workers.add_future(Box::pin(worker));
                }

                Either::Right((None, _)) => {
                    tracing::info!("Sync commands channel closed");
                    break;
                }
            }
        }

        anyhow::Ok(())
    };

    try_join!(jmap_api_worker, sync_account, handle_sync_commands)?;

    Ok(())
}

async fn sync_account(
    repo: &Repository,
    account_id: AccountId,
    jmap_api: &JmapApi,
) -> anyhow::Result<()> {
    let mut push_sub = jmap_api.subscribe_pushes();
    loop {
        let (new_state, updated, deleted) = match repo.get_mailboxes_sync_state(account_id).await? {
            Some(since_state) if !since_state.is_empty() => {
                let mut resp = jmap_api.mailboxes_changes(since_state).await?;
                let mut updated = resp.take_created();
                updated.extend(resp.take_updated());
                (resp.take_new_state(), updated, resp.take_destroyed())
            }

            _ => {
                let mut resp = jmap_api.query_mailboxes().await?;
                tracing::info!("Got mailbox query: {resp:?}");
                (resp.take_query_state(), resp.take_ids(), vec![])
            }
        };

        tracing::info!(
            "Updating {} mailboxes, deleted {}",
            updated.len(),
            deleted.len()
        );

        // Fetch all updated mailboxes details
        let updated = if updated.is_empty() {
            vec![]
        } else {
            jmap_api
                .get_mailboxes(updated)
                .await
                .context("Error getting mailboxes")?
                .take_list()
        };

        repo.update_mailboxes(account_id, &new_state, updated, deleted)
            .await
            .context("Failed to update mailboxes")?;

        loop {
            match push_sub.recv().await?.as_ref() {
                PushObject::StateChange { changed }
                    if changed
                        .iter()
                        .any(|(_, m)| m.contains_key(&DataType::Mailbox)) =>
                {
                    tracing::info!("Mailboxes changed, restarting sync");
                    break;
                }

                _ => {
                    // Irrelevant push notification
                    continue;
                }
            }
        }
    }
}

#[instrument(skip(repo, jmap_api), ret)]
async fn handle_sync_command(
    repo: &Repository,
    account_id: AccountId,
    jmap_api: &JmapApi,
    sync_command: SyncCommand,
) -> anyhow::Result<()> {
    match sync_command {
        SyncCommand::Watch(cmd) => handle_watch_command(repo, account_id, jmap_api, cmd).await,
    }
}

async fn handle_watch_command(
    repo: &Repository,
    account_id: AccountId,
    jmap_api: &JmapApi,
    WatchSyncCommand {
        mut query_rx,
        state_tx,
    }: WatchSyncCommand,
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
                let emails = jmap_api.get_emails(updated.into_iter().collect()).await?;

                repo.update_emails(account_id, emails.list())
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
