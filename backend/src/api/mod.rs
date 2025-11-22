use crate::jmap_account::AccountId;
use crate::repo::Repository;
use crate::sync::SyncCommand;
use axum::routing::{get, post};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

mod get_email_body;
mod stream;
mod sync_mail;
mod sync_mailbox;
mod watch_mail;
mod watch_mailboxes;
mod watch_threads;

#[derive(Clone)]
pub struct ApiState {
    pub repo: Arc<Repository>,
    pub sync_command_sender: Arc<RwLock<HashMap<AccountId, mpsc::Sender<SyncCommand>>>>,
}

pub fn build_api_router() -> axum::Router<ApiState> {
    use axum::Router;

    Router::new()
        .route("/mails/{account_id}", post(watch_mail::watch_mail))
        .route(
            "/mails/{account_id}/{email_id}",
            post(get_email_body::get_email_body),
        )
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
}
