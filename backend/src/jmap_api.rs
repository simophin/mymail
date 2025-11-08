use anyhow::Context;
use derive_more::Debug as DeriveDebug;
use futures::StreamExt;
use futures::future::{Either, select};
use jmap_client::client::Client;
use jmap_client::client_ws::WebSocketMessage;
use jmap_client::core::query::{Comparator, Filter, QueryResponse};
use jmap_client::core::request::Request;
use jmap_client::core::response::{
    EmailChangesResponse, EmailGetResponse, MailboxChangesResponse, MailboxGetResponse,
    TaggedMethodResponse,
};
use jmap_client::{DataType, PushObject, email};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::pin::pin;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::instrument;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub enum EmailSortColumn {
    Date,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct EmailSort {
    pub column: EmailSortColumn,
    pub asc: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct EmailQuery {
    pub anchor_id: Option<String>,
    pub mailbox_id: Option<String>,
    pub search_keyword: Option<String>,
    pub sorts: Vec<EmailSort>,
    pub limit: Option<NonZeroUsize>,
}

#[derive(DeriveDebug)]
enum JmapRequest {
    MailboxChanges {
        since_state: String,
    },
    QueryMailboxes,
    GetMailboxByIds(Vec<String>),
    QueryEmail(EmailQuery),
    EmailChanges {
        since_state: String,
    },
    GetEmailByIds {
        #[debug(ignore)]
        ids: Vec<String>,
        partial_properties: Option<Vec<email::Property>>,
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

            Self::QueryEmail(EmailQuery {
                anchor_id,
                mailbox_id,
                search_keyword,
                sorts,
                limit,
            }) => {
                let mut req = client.build();
                let query = req.query_email().calculate_total(true);

                if let Some(limit) = limit {
                    query.limit(limit.get());
                }

                // Construct filters
                let mut filters = Vec::new();
                if let Some(mailbox_id) = mailbox_id {
                    filters.push(email::query::Filter::InMailbox { value: mailbox_id });
                }

                if let Some(search_keyword) = search_keyword {
                    filters.push(email::query::Filter::Text {
                        value: search_keyword,
                    });
                }

                if !filters.is_empty() {
                    query.filter(Filter::and(filters));
                }

                // Sorts
                if !sorts.is_empty() {
                    let jmap_sorts: Vec<_> = sorts
                        .into_iter()
                        .map(|s| {
                            let comparator = match s.column {
                                EmailSortColumn::Date => {
                                    Comparator::new(email::query::Comparator::ReceivedAt)
                                }
                            };

                            if s.asc {
                                comparator.ascending()
                            } else {
                                comparator.descending()
                            }
                        })
                        .collect();
                    query.sort(jmap_sorts);
                }

                // Anchor
                if let Some(anchor_id) = anchor_id {
                    query.anchor(anchor_id);
                }

                req
            }

            Self::EmailChanges { since_state } => {
                let mut req = client.build();
                req.changes_email(since_state);
                req
            }

            Self::GetEmailByIds {
                ids,
                partial_properties,
            } => {
                let mut req = client.build();
                let r = req.get_email().ids(ids);
                if let Some(props) = partial_properties {
                    r.properties(props);
                }
                req
            }
        }
    }
}

type JmapRequestCallback = oneshot::Sender<Vec<TaggedMethodResponse>>;

pub struct JmapApi {
    request_sender: mpsc::Sender<(JmapRequest, JmapRequestCallback)>,
    notification_receiver: broadcast::Receiver<Arc<PushObject>>,
}

impl JmapApi {
    pub async fn new(
        client: Client,
    ) -> anyhow::Result<(
        Self,
        impl Future<Output = anyhow::Result<()>> + Send + Sync + 'static,
    )> {
        let (request_sender, mut pending_requests_rx) =
            mpsc::channel::<(JmapRequest, JmapRequestCallback)>(100);
        let (notification_sender, notification_receiver) =
            broadcast::channel::<Arc<PushObject>>(100);

        let mut ws_stream = client
            .connect_ws()
            .await
            .context("Failed to connect to JMAP server with websockets")?;

        client
            .enable_push_ws(
                Some([DataType::Core, DataType::Mailbox, DataType::Email]),
                None::<&'static str>,
            )
            .await
            .context("Failed to enable push via ws")?;

        let handle_requests = async move {
            let mut requests: HashMap<String, oneshot::Sender<Vec<TaggedMethodResponse>>> =
                Default::default();

            loop {
                let pending_request = pin!(pending_requests_rx.recv());
                match select(ws_stream.next(), pending_request).await {
                    Either::Left((Some(Ok(WebSocketMessage::PushNotification(obj))), _)) => {
                        tracing::info!("Received push notification: {obj:?} ");
                        let _ = notification_sender.send(Arc::new(obj));
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

            anyhow::Ok(())
        };

        Ok((
            Self {
                request_sender,
                notification_receiver,
            },
            handle_requests,
        ))
    }

    pub fn subscribe_pushes(&self) -> broadcast::Receiver<Arc<PushObject>> {
        self.notification_receiver.resubscribe()
    }

    async fn send_ws_request(&self, req: JmapRequest) -> anyhow::Result<TaggedMethodResponse> {
        let (callback, resp_rx) = oneshot::channel();

        self.request_sender
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

    #[instrument(skip(self), ret, level = "debug")]
    pub async fn query_mailboxes(&self) -> anyhow::Result<QueryResponse> {
        self.send_ws_request(JmapRequest::QueryMailboxes)
            .await?
            .unwrap_query_mailbox()
            .context("Expecting mailbox query response")
    }

    #[instrument(skip(self), ret, level = "debug")]
    pub async fn get_mailboxes(&self, ids: Vec<String>) -> anyhow::Result<MailboxGetResponse> {
        self.send_ws_request(JmapRequest::GetMailboxByIds(ids))
            .await?
            .unwrap_get_mailbox()
            .context("Expecting mailbox get response")
    }

    #[instrument(skip(self), ret, level = "debug")]
    pub async fn mailboxes_changes(
        &self,
        since_state: String,
    ) -> anyhow::Result<MailboxChangesResponse> {
        self.send_ws_request(JmapRequest::MailboxChanges { since_state })
            .await?
            .unwrap_changes_mailbox()
            .context("Expecting mailbox changes response")
    }

    #[instrument(skip(self), ret, level = "debug")]
    pub async fn query_emails(&self, query: EmailQuery) -> anyhow::Result<QueryResponse> {
        self.send_ws_request(JmapRequest::QueryEmail(query))
            .await?
            .unwrap_query_email()
            .context("Expecting email query response")
    }

    #[instrument(skip(self), ret, level = "debug")]
    pub async fn email_changes(&self, since_state: String) -> anyhow::Result<EmailChangesResponse> {
        self.send_ws_request(JmapRequest::EmailChanges { since_state })
            .await?
            .unwrap_changes_email()
            .context("Expecting email changes response")
    }

    #[instrument(skip(self), ret, level = "debug")]
    pub async fn get_emails(
        &self,
        ids: Vec<String>,
        partial_properties: Option<Vec<email::Property>>,
    ) -> anyhow::Result<EmailGetResponse> {
        self.send_ws_request(JmapRequest::GetEmailByIds {
            ids,
            partial_properties,
        })
        .await?
        .unwrap_get_email()
        .context("Expecting email get response")
    }
}
