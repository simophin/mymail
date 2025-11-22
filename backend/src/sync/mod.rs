mod fetch_blob;
mod sync_account;
mod sync_emails;
mod sync_mailbox;

use crate::jmap_account::{AccountId, AccountRepositoryExt, Credentials};
use crate::jmap_api::JmapApi;
use crate::repo::Repository;
use anyhow::{Context, bail};
use futures::FutureExt;
use futures::future::{Fuse, FusedFuture, try_join_all};
use jmap_client::client::Client;
use serde::Serialize;
use std::fmt::Debug;
use std::future::pending;
use std::pin::Pin;
use tokio::sync::{mpsc, watch};
use tokio::{select, try_join};
use tracing::instrument;
use url::Url;

use crate::util::network::NetworkAvailability;
pub use fetch_blob::FetchBlobCommand;
pub use sync_emails::WatchEmailSyncCommand;
pub use sync_mailbox::WatchMailboxSyncCommand;

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "state")]
pub enum EmailQueryState {
    NotStarted,
    InProgress,
    Error { details: String },
    UpToDate,
}

#[derive(Debug)]
pub enum SyncCommand {
    WatchEmails(WatchEmailSyncCommand),
    WatchMailbox(WatchMailboxSyncCommand),
    FetchEmailDetails(FetchBlobCommand),
}

struct AccountState {
    mailbox_watch_request_tx: mpsc::Sender<(String, sync_mailbox::WatchRequest)>,
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
        Credentials::Basic { username, password } => (username.to_string(), password.to_string()),
    };

    let (mailbox_watch_request_tx, mailbox_watch_request_rx) = mpsc::channel(16);
    let (network_availability_tx, network_availability_rx) =
        watch::channel(NetworkAvailability { online: true });

    let account_state = AccountState {
        mailbox_watch_request_tx,
    };

    let jmap_api = JmapApi::new(url, credentials, network_availability_rx);
    let sync_account = sync_account::sync_account(repo, account_id, &jmap_api);
    let handle_sync_commands = async {
        let mut sync_command_futures: Vec<Fuse<Pin<Box<_>>>> = Vec::new();

        loop {
            let drive_workers = async {
                while !sync_command_futures.is_empty() {
                    let _ = try_join_all(sync_command_futures.iter_mut()).await;
                    sync_command_futures.retain(|fut| !fut.is_terminated());
                }

                pending::<()>().await;
            };

            select! {
                _ = drive_workers => {}
                cmd = sync_commands.recv() => {
                    let Some(cmd) = cmd else {
                        bail!("Command channel closed unexpectedly");
                    };

                    tracing::info!("Handling sync command: {cmd:?}");
                    sync_command_futures.push(
                        Box::pin(handle_sync_command(
                            repo,
                            account_id,
                            &jmap_api,
                            cmd,
                            &account_state,
                        )).fuse());
                }
            }
        }

        anyhow::Ok(())
    };

    let sync_mailboxes =
        sync_mailbox::sync_mailboxes(repo, account_id, &jmap_api, mailbox_watch_request_rx);

    try_join!(sync_account, handle_sync_commands, sync_mailboxes)?;

    Ok(())
}

#[instrument(skip(repo, jmap_api, account_state), ret)]
async fn handle_sync_command(
    repo: &Repository,
    account_id: AccountId,
    jmap_api: &JmapApi,
    sync_command: SyncCommand,
    account_state: &AccountState,
) -> anyhow::Result<()> {
    match sync_command {
        SyncCommand::WatchEmails(cmd) => {
            sync_emails::handle_watch_command(repo, account_id, jmap_api, cmd).await
        }

        SyncCommand::WatchMailbox(cmd) => {
            sync_mailbox::handle_watch_mailbox_command(cmd, account_state).await
        }

        SyncCommand::FetchEmailDetails(FetchBlobCommand { email_id, callback }) => {
            let result =
                fetch_blob::handle_fetch_blob_command(account_id, jmap_api, repo, &email_id).await;

            let _ = callback.send(result);
            Ok(())
        }
    }
}
