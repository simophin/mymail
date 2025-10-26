use crate::jmap_account::{AccountId, AccountRepositoryExt, Credentials};
use crate::repo::Repository;
use anyhow::Context;
use futures::StreamExt;
use futures::future::{Either, join_all, select};
use jmap_client::client::Client;
use jmap_client::client_ws::WebSocketMessage;
use jmap_client::core::changes::{ChangesObject, ChangesResponse};
use jmap_client::core::request::Request;
use jmap_client::core::response::{MailboxChangesResponse, TaggedMethodResponse};
use jmap_client::mailbox::Mailbox;
use jmap_client::{DataType, PushObject, mailbox};
use sqlx::query;
use std::collections::HashMap;
use std::pin::pin;
use tokio::sync::{mpsc, oneshot};
use tokio::try_join;
use url::Url;

trait JmapRepositoryExt {
    async fn get_mailboxes_sync_state(
        &self,
        account_id: AccountId,
    ) -> anyhow::Result<Option<String>>;

    async fn update_mailboxes(
        &self,
        account_id: AccountId,
        new_state: &str,
        updated: Vec<Mailbox>,
        deleted: Vec<String>,
    ) -> anyhow::Result<()>;
}

impl JmapRepositoryExt for Repository {
    async fn get_mailboxes_sync_state(
        &self,
        account_id: AccountId,
    ) -> anyhow::Result<Option<String>> {
        Ok(query!(
            "SELECT mailboxes_sync_state FROM accounts WHERE id = ?",
            account_id
        )
        .fetch_optional(self.pool())
        .await
        .context("Error querying mailboxes sync state")?
        .context("Account not found")?
        .mailboxes_sync_state)
    }

    async fn update_mailboxes(
        &self,
        account_id: AccountId,
        new_state: &str,
        updated: Vec<Mailbox>,
        deleted: Vec<String>,
    ) -> anyhow::Result<()> {
        let mut tx = self.pool().begin().await?;

        for updated in updated {
            let id = updated.id();
            let name = updated.name();
            let sort_order = updated.sort_order() as i64;
            let total_emails = updated.total_emails() as i64;
            let unread_emails = updated.unread_emails() as i64;
            let total_threads = updated.total_threads() as i64;
            let unread_threads = updated.unread_threads() as i64;
            let parent_id = updated.parent_id();
            let role = serde_json::to_string(&updated.role()).ok();
            let my_rights = serde_json::to_string(&updated.my_rights()).ok();

            sqlx::query!(
                "INSERT OR REPLACE INTO mailboxes
                    (id, account_id, name, role, sort_order, total_emails, unread_emails, total_threads, unread_threads, my_rights, parent_id)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                id,
                account_id,
                name,
                role,
                sort_order,
                total_emails,
                unread_emails,
                total_threads,
                unread_threads,
                my_rights,
                parent_id,
            ).execute(&mut *tx).await.context("Error updating mailbox")?;
        }

        for deleted_id in deleted {
            sqlx::query!(
                "DELETE FROM mailboxes WHERE id = ? AND account_id = ?",
                deleted_id,
                account_id
            )
            .execute(&mut *tx)
            .await
            .context("Error deleting mailbox")?;
        }

        sqlx::query!(
            "UPDATE accounts SET mailboxes_sync_state = ? WHERE id = ?",
            new_state,
            account_id
        )
        .execute(&mut *tx)
        .await
        .context("Error updating mailboxes sync state")?;

        tx.commit().await?;
        Ok(())
    }
}

#[derive(Debug)]
enum JmapRequest {
    MailboxChanges { since_state: String },
    QueryMailboxes,
    GetMailboxByIds(Vec<String>),
}

impl JmapRequest {
    fn to_request(self, client: &Client) -> Request<'_> {
        match self {
            JmapRequest::MailboxChanges { since_state, .. } => {
                let mut req = client.build();
                req.changes_mailbox(since_state).max_changes(100);
                req
            }

            JmapRequest::QueryMailboxes => {
                let mut req = client.build();
                req.query_mailbox().limit(100);
                req
            }

            JmapRequest::GetMailboxByIds(ids) => {
                let mut req = client.build();
                req.get_mailbox().ids(ids);
                req
            }
        }
    }
}

pub async fn run_jmap_sync(repo: &Repository, account_id: AccountId) -> anyhow::Result<()> {
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

    let mut ws_stream = client
        .connect_ws()
        .await
        .context("Failed to connect to JMAP server with websockets")?;

    client
        .enable_push_ws(
            Some([DataType::Core, DataType::Mailbox]),
            None::<&'static str>,
        )
        .await
        .context("Failed to enable push via ws")?;

    let (pending_requests, mut pending_requests_rx) =
        mpsc::channel::<(JmapRequest, oneshot::Sender<Vec<TaggedMethodResponse>>)>(10);

    let (mailboxes_changed_tx, mut mailboxes_changed) = mpsc::channel::<()>(10);

    let run_ws_stream = async {
        let mut requests: HashMap<String, oneshot::Sender<Vec<TaggedMethodResponse>>> =
            Default::default();

        loop {
            let pending_request = pin!(pending_requests_rx.recv());
            match select(ws_stream.next(), pending_request).await {
                Either::Left((
                    Some(Ok(WebSocketMessage::PushNotification(PushObject::StateChange {
                        changed,
                    }))),
                    _,
                )) if changed
                    .iter()
                    .any(|(_, r)| r.contains_key(&DataType::Mailbox)) =>
                {
                    tracing::info!("Received push notification: {changed:?} ");
                    let _ = mailboxes_changed_tx.send(()).await;
                }

                Either::Left((Some(Ok(WebSocketMessage::PushNotification(obj))), _)) => {
                    tracing::info!("Received push notification: {obj:?}");
                }

                Either::Left((Some(Ok(WebSocketMessage::Response(response))), _)) => {
                    tracing::debug!("Received tagged method response: {response:?}");
                    if let Some(cb) = response.request_id().and_then(|id| requests.remove(id)) {
                        let _ = cb.send(response.unwrap_method_responses());
                        continue;
                    }
                }

                Either::Left((Some(Err(e)), _)) => {
                    return Err(e).context("Error receiving WS message");
                }

                Either::Left((None, _)) => {
                    tracing::info!("WebSocket stream closed");
                    break;
                }

                Either::Right((Some((req, cb)), _)) => {
                    tracing::info!("Sending {req:?}");

                    let request_id = req
                        .to_request(&client)
                        .send_ws()
                        .await
                        .context("Failed to send WS message")?;

                    tracing::debug!("Sent request with request_id = {request_id}");
                    requests.insert(request_id, cb);
                }

                Either::Right((None, _)) => {
                    tracing::info!("Pending requests channel closed");
                    break;
                }
            }
        }

        Ok(())
    };

    let send_ws_request = async |req: JmapRequest| -> anyhow::Result<TaggedMethodResponse> {
        let (callback, resp_rx) = oneshot::channel();

        pending_requests
            .send((req, callback))
            .await
            .context("Error sending WS request")?;

        Ok(resp_rx
            .await
            .context("Error receiving WS response")?
            .into_iter()
            .next()
            .context("No response received")?)
    };

    let sync_mailboxes = async {
        loop {
            let (new_state, updated, deleted) =
                match repo.get_mailboxes_sync_state(account_id).await? {
                    Some(since_state) if !since_state.is_empty() => {
                        let mut resp = send_ws_request(JmapRequest::MailboxChanges { since_state })
                            .await?
                            .unwrap_changes_mailbox()
                            .context("Expecting mailbox changes response")?;

                        let mut updated = resp.take_created();
                        updated.extend(resp.take_updated());
                        (resp.take_new_state(), updated, resp.take_destroyed())
                    }

                    _ => {
                        let mut resp = send_ws_request(JmapRequest::QueryMailboxes)
                            .await?
                            .unwrap_query_mailbox()
                            .context("Expecting mailbox query response")?;

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
                send_ws_request(JmapRequest::GetMailboxByIds(updated))
                    .await
                    .context("Error getting mailboxes")?
                    .unwrap_get_mailbox()
                    .context("Expecting mailbox query response")?
                    .take_list()
            };

            repo.update_mailboxes(account_id, &new_state, updated, deleted)
                .await
                .context("Failed to update mailboxes")?;

            if mailboxes_changed.recv().await.is_none() {
                break;
            }

            tracing::info!("Mailboxes changed")
        }

        Ok(())
    };

    try_join!(run_ws_stream, sync_mailboxes)?;
    Ok(())
}
