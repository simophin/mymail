mod sync_account;
mod sync_accounts;
mod sync_mailbox_list;
mod sync_mailboxes;
mod watch_emails;

use serde::Serialize;
use std::fmt::Debug;

pub use sync_mailboxes::WatchMailboxSyncCommand;
pub use watch_emails::WatchEmailSyncCommand;

pub use sync_account::SyncCommand;
pub use sync_accounts::sync_accounts;

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "state")]
pub enum EmailQueryState {
    NotStarted,
    InProgress,
    Error { details: String },
    UpToDate,
}
