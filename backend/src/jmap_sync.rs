use crate::future_set::FutureWorkers;
use crate::jmap_account::{AccountId, AccountRepositoryExt, Credentials};
use crate::jmap_api::{EmailQuery, JmapApi};
use crate::jmap_repo::JmapRepositoryExt;
use crate::repo::Repository;
use anyhow::Context;
use futures::future::{Either, select};
use jmap_client::client::Client;
use jmap_client::{DataType, PushObject};
use std::fmt::Debug;
use std::pin::pin;
use tokio::sync::{mpsc, watch};
use tokio::try_join;
use tracing::instrument;
use url::Url;

pub enum EmailQueryState {
    NotStarted,
    InProgress,
    Error(String),
    UpToDate,
}

pub enum SyncCommand {
    Watch {
        query: watch::Receiver<EmailQuery>,
        state_tx: watch::Sender<(EmailQuery, EmailQueryState)>,
    },
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

        jmap_api
            .wait_for_pushes(|o| {
                matches!(o, PushObject::StateChange { changed }
                if changed.iter().any(|(_, m)| m.contains_key(&DataType::Mailbox)))
            })
            .await?;

        tracing::info!("Mailboxes changed")
    }

    Ok(())
}

#[instrument(skip(repo, jmap_api))]
async fn handle_sync_command(
    repo: &Repository,
    account_id: AccountId,
    jmap_api: &JmapApi,
    sync_command: SyncCommand,
) -> anyhow::Result<()> {
    match sync_command {
        SyncCommand::Watch { query, state_tx } => {
            handle_watch_command(repo, account_id, jmap_api, query, state_tx).await
        }
    }
}

async fn handle_watch_command(
    repo: &Repository,
    account_id: AccountId,
    jmap_api: &JmapApi,
    mut query_rx: watch::Receiver<EmailQuery>,
    state_tx: watch::Sender<(EmailQuery, EmailQueryState)>,
) -> anyhow::Result<()> {
    struct LastQuery {
        sync_state: String,
        query: EmailQuery,
    }

    let mut last_query = None::<LastQuery>;

    loop {
        let query = query_rx.borrow().clone();

        let fetch_results = async {
            state_tx.send((query.clone(), EmailQueryState::InProgress))?;

            let (updated, destroyed, new_state) = match &last_query {
                Some(state) if state.query == query => {
                    let mut changes = jmap_api.email_changes(state.sync_state.clone()).await?;
                    let mut created = changes.take_created();
                    created.extend(changes.take_updated());
                    (created, changes.take_destroyed(), changes.take_new_state())
                }

                _ => {
                    let mut resp = jmap_api.query_emails(query_rx.borrow().clone()).await?;
                    (resp.take_ids(), vec![], resp.take_query_state())
                }
            };

            let already_downloaded = repo
                .find_downloaded_email_ids(account_id, &updated)
                .await
                .context("Failed to check downloaded emails")?;

            let updated = updated
                .into_iter()
                .filter(|id| !already_downloaded.contains(id))
                .collect::<Vec<_>>();

            let emails = jmap_api.get_emails(updated).await?;

            repo.update_emails(account_id, emails.list())
                .await
                .context("Failed to update emails")?;

            repo.delete_emails(account_id, &destroyed)
                .await
                .context("Failed to delete emails")?;

            last_query = Some(LastQuery {
                sync_state: new_state.clone(),
                query: query.clone(),
            });

            anyhow::Ok(())
        };

        if let Err(e) = fetch_results.await {
            tracing::error!("Error syncing emails: {e:?}");
            state_tx.send((query.clone(), EmailQueryState::Error(e.to_string())))?;
        } else {
            state_tx.send((query.clone(), EmailQueryState::UpToDate))?;
        }

        match select(
            pin!(query_rx.changed()),
            pin!(jmap_api.wait_for_pushes(|o| {
                matches!(o, PushObject::StateChange { changed }
                if changed.iter().any(|(_, m)| m.contains_key(&DataType::Email)))
            })),
        )
        .await
        {
            Either::Left((Err(_), _)) => {
                tracing::info!("Query watch channel closed");
                break;
            }

            Either::Right((Err(_), _)) => {
                tracing::info!("JMAP API push notification channel closed");
                break;
            }

            Either::Left((Ok(_), _)) => {
                tracing::info!("Query changed, restarting sync");
            }

            Either::Right((Ok(_), _)) => {
                tracing::info!("Emails changed, restarting sync");
            }
        }
    }

    Ok(())
}
