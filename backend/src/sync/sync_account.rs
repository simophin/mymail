use super::sync_mailbox_list;
use super::sync_mailboxes;
use super::sync_mailboxes::WatchMailboxSyncCommand;
use super::watch_emails;
use super::watch_emails::WatchEmailSyncCommand;
use crate::jmap_account::AccountId;
use crate::jmap_api::JmapApi;
use crate::repo::Repository;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tracing::instrument;

#[derive(Debug)]
pub enum SyncCommand {
    WatchEmails(WatchEmailSyncCommand),
    WatchMailbox(WatchMailboxSyncCommand),
}

#[instrument(skip(repo, jmap_api, sync_commands), ret, level = "info")]
pub async fn sync_account(
    repo: Arc<Repository>,
    account_id: AccountId,
    jmap_api: Arc<JmapApi>,
    mut sync_commands: mpsc::Receiver<SyncCommand>,
) -> anyhow::Result<()> {
    let (mailbox_watch_request_tx, mailbox_watch_request_rx) = mpsc::channel(16);

    let mut join_set = JoinSet::new();

    join_set.spawn(sync_mailbox_list::sync_mailbox_list(
        repo.clone(),
        account_id,
        jmap_api.clone(),
    ));

    join_set.spawn(sync_mailboxes::sync_mailboxes(
        repo.clone(),
        account_id,
        jmap_api.clone(),
        mailbox_watch_request_rx,
    ));

    while let Some(cmd) = sync_commands.recv().await {
        match cmd {
            SyncCommand::WatchEmails(cmd) => {
                join_set.spawn(watch_emails::handle_watch_command(
                    repo.clone(),
                    account_id,
                    jmap_api.clone(),
                    cmd,
                ));
            }
            SyncCommand::WatchMailbox(watch_cmd) => {
                join_set.spawn(sync_mailboxes::handle_watch_mailbox_command(
                    watch_cmd,
                    mailbox_watch_request_tx.clone(),
                ));
            }
        }
    }

    Ok(())
}
