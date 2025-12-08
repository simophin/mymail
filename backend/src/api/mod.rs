use crate::jmap_account::{Account, AccountId};
use crate::jmap_api::JmapApi;
use crate::repo::Repository;
use crate::sync::SyncCommand;
use axum::routing::{any, get, post};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

mod get_blob;
mod proxy;
mod static_file;
mod stream;
mod sync_mail;
mod sync_mailbox;
mod watch_mail;
mod watch_mailboxes;
mod watch_threads;

pub struct AccountState {
    pub account: Account,
    pub command_sender: mpsc::Sender<SyncCommand>,
    pub jmap_api: Arc<JmapApi>,
    pub join_set: JoinSet<anyhow::Result<()>>,
}

#[derive(Clone)]
pub struct ApiState {
    pub repo: Arc<Repository>,
    pub account_states: Arc<RwLock<HashMap<AccountId, AccountState>>>,
    pub http_client: reqwest::Client,
}

pub fn build_api_router() -> axum::Router<ApiState> {
    use axum::Router;

    Router::new()
        .route("/mails/{account_id}", post(watch_mail::watch_mail))
        .route("/blobs/{account_id}/{b lob_id}", get(get_blob::get_blob))
        .route("/mails/sync/{account_id}", get(sync_mail::sync_mail))
        .route(
            "/mailboxes/sync/{account_id}/{mailbox_id}",
            get(sync_mailbox::sync_mailbox),
        )
        .route(
            "/mailboxes/{account_id}",
            get(watch_mailboxes::watch_mailboxes),
        )
        .route("/threads/{account_id}", get(watch_threads::watch_threads))
        .route("/", any(static_file::static_file_or_dev_proxy))
        .route("/{*path}", any(static_file::static_file_or_dev_proxy))
}
