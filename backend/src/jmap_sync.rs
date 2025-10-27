use crate::jmap_account::{AccountId, AccountRepositoryExt, Credentials};
use crate::jmap_repo::JmapRepositoryExt;
use crate::repo::Repository;
use anyhow::Context;
use futures::StreamExt;
use futures::future::{Either, select};
use jmap_client::client::Client;
use jmap_client::client_ws::WebSocketMessage;
use jmap_client::core::query::{Comparator, Filter};
use jmap_client::core::request::Request;
use jmap_client::core::response::TaggedMethodResponse;
use jmap_client::{DataType, PushObject, email};
use std::collections::HashMap;
use std::pin::pin;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::try_join;
use url::Url;

#[derive(Debug)]
enum JmapRequest {
    MailboxChanges {
        since_state: String,
    },
    QueryMailboxes,
    GetMailboxByIds(Vec<String>),
    QueryEmail {
        anchor_id: Option<String>,
        sort: Vec<Comparator<email::query::Comparator>>,
        filters: Vec<Filter<email::query::Filter>>,
        limit: usize,
    },
    EmailChanges {
        since_state: String,
    },
}

impl JmapRequest {
    fn to_request(self, client: &Client) -> Request<'_> {
        match self {
            Self::MailboxChanges { since_state, .. } => {
                let mut req = client.build();
                req.changes_mailbox(since_state).max_changes(100);
                req
            }

            Self::QueryMailboxes => {
                let mut req = client.build();
                req.query_mailbox().limit(100);
                req
            }

            Self::GetMailboxByIds(ids) => {
                let mut req = client.build();
                req.get_mailbox().ids(ids);
                req
            }

            Self::QueryEmail {
                anchor_id,
                sort,
                filters,
                limit,
            } => {
                let mut req = client.build();
                let query_email = filters
                    .into_iter()
                    .fold(req.query_email(), |r, f| r.filter(f))
                    .limit(limit)
                    .sort(sort);

                if let Some(anchor_id) = anchor_id {
                    query_email.anchor(anchor_id);
                }

                req
            }

            Self::EmailChanges { since_state } => {
                let mut req = client.build();
                req.changes_email(since_state);
                req
            }
        }
    }
}

type JmapRequestCallback = oneshot::Sender<Vec<TaggedMethodResponse>>;

pub enum SyncCommand {
    SubscribeToMailbox { mailbox_id: String },
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
        mpsc::channel::<(JmapRequest, JmapRequestCallback)>(10);

    let (mailboxes_changed_tx, mailboxes_changed) = broadcast::channel::<String>(10);

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
                    for (_, id) in changed
                        .into_iter()
                        .flat_map(|(_, r)| r.into_iter())
                        .filter(|(t, _)| *t == DataType::Mailbox)
                    {
                        let _ = mailboxes_changed_tx.send(id);
                    }
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

    let sync_account = sync_account(repo, account_id, &pending_requests, mailboxes_changed);

    try_join!(run_ws_stream, sync_account)?;
    Ok(())
}

async fn send_ws_request(
    request_sender: &mpsc::Sender<(JmapRequest, JmapRequestCallback)>,
    req: JmapRequest,
) -> anyhow::Result<TaggedMethodResponse> {
    let (callback, resp_rx) = oneshot::channel();

    request_sender
        .send((req, callback))
        .await
        .context("Error sending WS request")?;

    Ok(resp_rx
        .await
        .context("Error receiving WS response")?
        .into_iter()
        .next()
        .context("No response received")?)
}

async fn sync_account(
    repo: &Repository,
    account_id: AccountId,
    request_sender: &mpsc::Sender<(JmapRequest, JmapRequestCallback)>,
    mut mailbox_changed: broadcast::Receiver<String>,
) -> anyhow::Result<()> {
    loop {
        let (new_state, updated, deleted) = match repo.get_mailboxes_sync_state(account_id).await? {
            Some(since_state) if !since_state.is_empty() => {
                let mut resp =
                    send_ws_request(request_sender, JmapRequest::MailboxChanges { since_state })
                        .await?
                        .unwrap_changes_mailbox()
                        .context("Expecting mailbox changes response")?;

                let mut updated = resp.take_created();
                updated.extend(resp.take_updated());
                (resp.take_new_state(), updated, resp.take_destroyed())
            }

            _ => {
                let mut resp = send_ws_request(request_sender, JmapRequest::QueryMailboxes)
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
            send_ws_request(request_sender, JmapRequest::GetMailboxByIds(updated))
                .await
                .context("Error getting mailboxes")?
                .unwrap_get_mailbox()
                .context("Expecting mailbox query response")?
                .take_list()
        };

        repo.update_mailboxes(account_id, &new_state, updated, deleted)
            .await
            .context("Failed to update mailboxes")?;

        if mailbox_changed.recv().await.is_err() {
            break;
        }

        tracing::info!("Mailboxes changed")
    }

    Ok(())
}

async fn async_mailbox(
    repo: &Repository,
    account_id: AccountId,
    request_sender: &mpsc::Sender<(JmapRequest, JmapRequestCallback)>,
    mailbox_id: &str,
) -> anyhow::Result<()> {
    loop {
        match repo
            .get_mailbox_email_sync_state(account_id, mailbox_id)
            .await?
        {
            Some(since_state) if !since_state.is_empty() => {
                let mut resp = send_ws_request(
                    request_sender,
                    JmapRequest::EmailChanges {
                        mailbox_id: mailbox_id.to_string(),
                        since_state,
                    },
                )
                .await
                .context("Error sending WS email changes request")?
                .unwrap_changes_email()
                .context("Expecting email changes response")?;
            }

            _ => {
                let resp = send_ws_request(
                    request_sender,
                    JmapRequest::QueryEmail {
                        mailbox_id: mailbox_id.to_string(),
                    },
                )
                .await
                .context("Error sending WS query email request")?
                .unwrap_query_email()
                .context("Expecting email query response")?;
            }
        }
    }
}
