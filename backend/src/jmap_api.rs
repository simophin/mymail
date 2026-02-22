use crate::util::network::NetworkAvailability;
use anyhow::{Context, bail, format_err};
use derive_more::Debug as DeriveDebug;
use futures::StreamExt;
use futures::future::{Either, select};
use jmap_client::client::{Client, ClientBuilder, Credentials};
use jmap_client::client_ws::WebSocketMessage;
use jmap_client::core::query::{Comparator, Filter, QueryResponse};
use jmap_client::core::request::Request;
use jmap_client::core::response::{
    EmailChangesResponse, EmailGetResponse, MailboxChangesResponse, MailboxGetResponse,
    TaggedMethodResponse,
};
use jmap_client::identity::Identity;
use jmap_client::{DataType, PushObject, email};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::pin::pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::task::JoinSet;
use tokio::time::sleep_until;
use tracing::{Instrument, instrument};
use url::Url;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAddressInput {
    pub name: Option<String>,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentInput {
    pub blob_id: String,
    pub name: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailDraft {
    pub identity_id: String,
    pub mailbox_id: String,
    pub from: Vec<EmailAddressInput>,
    pub to: Vec<EmailAddressInput>,
    pub cc: Vec<EmailAddressInput>,
    pub bcc: Vec<EmailAddressInput>,
    pub subject: String,
    pub text_body: Option<String>,
    pub html_body: Option<String>,
    pub in_reply_to: Vec<String>,
    pub references: Vec<String>,
    pub attachments: Vec<AttachmentInput>,
}

type JmapRequestBuilder = Box<dyn FnOnce(&mut Request<'_>) + Send + Sync>;

type JmapRequestCallback = oneshot::Sender<anyhow::Result<TaggedMethodResponse>>;

#[derive(DeriveDebug)]
pub enum ClientState {
    Disconnected {
        last_error: Option<anyhow::Error>,
        #[debug(skip)]
        delay_connect_until: Option<Instant>,
    },
    Connnecting,
    Connected(#[debug(skip)] Arc<Client>),
}

pub struct JmapApi {
    client_state: watch::Receiver<ClientState>,
    request_sender: mpsc::Sender<(JmapRequestBuilder, JmapRequestCallback)>,
    notification_receiver: broadcast::Receiver<Arc<PushObject>>,
    tasks: JoinSet<()>,
}

impl JmapApi {
    #[instrument(skip(credentials, network_availability), level = "debug")]
    pub fn new(
        server_url: Url,
        credentials: impl Into<Credentials> + Clone + Send + Sync + 'static,
        network_availability: watch::Receiver<NetworkAvailability>,
    ) -> Self {
        let (request_sender, mut pending_requests_rx) =
            mpsc::channel::<(JmapRequestBuilder, JmapRequestCallback)>(100);
        let (notification_sender, notification_receiver) =
            broadcast::channel::<Arc<PushObject>>(100);

        let (client_state_tx, client_state) = watch::channel(ClientState::Disconnected {
            last_error: None,
            delay_connect_until: None,
        });

        let mut tasks = JoinSet::new();

        // Establish initial connection
        tasks.spawn({
            let mut network_availability = network_availability.clone();
            let span = tracing::info_span!("jmap_connect", server_url = server_url.as_str());

            async move {
                while network_availability.wait_for(|a| a.online).await.is_ok() {
                    let delay_connect_until = {
                        match &*client_state_tx.borrow() {
                            ClientState::Disconnected {
                                delay_connect_until,
                                ..
                            } => *delay_connect_until,
                            _ => None,
                        }
                    };

                    if let Some(deadline) = delay_connect_until {
                        sleep_until(deadline.into()).await;
                    };

                    let connect = async {
                        let _ = client_state_tx.send(ClientState::Connnecting);

                        let client = ClientBuilder::new()
                            .credentials(credentials.clone())
                            .follow_redirects([server_url.host_str().unwrap_or_default()])
                            .connect(server_url.as_str().trim_end_matches('/'))
                            .await
                            .context("Failed to connect to JMAP server")?;

                        let ws = client
                            .connect_ws()
                            .await
                            .context("Failed to connect to JMAP server")?;

                        client
                            .enable_push_ws(
                                Some([DataType::Email, DataType::Core, DataType::Mailbox]),
                                None::<&'static str>,
                            )
                            .await
                            .context("Failed to enable ws push")?;

                        anyhow::Ok((Arc::new(client), ws))
                    };

                    let (client, mut ws) = match connect
                        .await
                        .context("Failed to connect to JMAP server")
                    {
                        Ok(v) => {
                            tracing::info!("Connected to JMAP server");
                            let _ = client_state_tx.send(ClientState::Connected(v.0.clone()));
                            v
                        }

                        Err(e) => {
                            tracing::error!(?e, "Failed to connect");
                            let _ = client_state_tx.send(ClientState::Disconnected {
                                last_error: Some(e),
                                delay_connect_until: Some(Instant::now() + Duration::from_secs(10)),
                            });
                            continue;
                        }
                    };

                    // Handle websocket messages
                    let mut callbacks: HashMap<String, JmapRequestCallback> = Default::default();

                    loop {
                        match select(pin!(ws.next()), pin!(pending_requests_rx.recv())).await {
                            Either::Left((Some(Ok(WebSocketMessage::Response(res))), _)) => {
                                if let Some(callback) =
                                    res.request_id().and_then(|r| callbacks.remove(r))
                                {
                                    if let Some(res) = res.unwrap_method_responses().pop() {
                                        let _ = callback.send(Ok(res));
                                    } else {
                                        let _ = callback.send(Err(format_err!(
                                            "No method responses in tagged response"
                                        )));
                                    }
                                } else {
                                    tracing::warn!("Unable to find a callback for a response");
                                }
                            }

                            Either::Left((
                                Some(Ok(WebSocketMessage::PushNotification(push))),
                                _,
                            )) => {
                                let _ = notification_sender.send(Arc::new(push));
                            }

                            Either::Left((Some(Err(e)), _)) => {
                                tracing::error!(?e, "Error receiving WS message, reconnecting...");
                                let _ = client_state_tx.send(ClientState::Disconnected {
                                    last_error: Some(e.into()),
                                    delay_connect_until: Some(
                                        Instant::now() + Duration::from_secs(10),
                                    ),
                                });
                                break;
                            }

                            Either::Left((None, _)) | Either::Right((None, _)) => {
                                tracing::info!("WS stream or request channel closed, aborting...");
                                return;
                            }

                            Either::Right((Some((req_builder, callback)), _)) => {
                                let mut req = client.build();
                                req_builder(&mut req);
                                match req.send_ws().await {
                                    Ok(request_id) => {
                                        callbacks.insert(request_id, callback);
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            ?e,
                                            "Error sending WS message to JMAP server"
                                        );
                                        let e = Arc::new(e);
                                        let _ = client_state_tx.send(ClientState::Disconnected {
                                            last_error: Some(e.clone().into()),
                                            delay_connect_until: Some(
                                                Instant::now() + Duration::from_secs(10),
                                            ),
                                        });
                                        let _ = callback
                                            .send(Err(e).context("Error queueing ws request"));
                                        break;
                                    }
                                };
                            }
                        }
                    }
                }
            }
            .instrument(span)
        });

        Self {
            client_state,
            request_sender,
            notification_receiver,
            tasks,
        }
    }

    pub fn subscribe_pushes(&self) -> broadcast::Receiver<Arc<PushObject>> {
        self.notification_receiver.resubscribe()
    }

    pub fn subscribe_client_state(&self) -> watch::Receiver<ClientState> {
        self.client_state.clone()
    }

    async fn send_ws_request(
        &self,
        req: impl FnOnce(&mut Request<'_>) + Send + Sync + 'static,
    ) -> anyhow::Result<TaggedMethodResponse> {
        let (callback, resp_rx) = oneshot::channel();

        if self
            .request_sender
            .send((Box::new(req), callback))
            .await
            .is_err()
        {
            bail!("Queueing request failed");
        }

        Ok(resp_rx
            .await
            .context("Error receiving WS response")?
            .into_iter()
            .next()
            .context("No response received")?)
    }

    #[instrument(skip(self), ret, level = "debug")]
    pub async fn query_mailboxes(&self) -> anyhow::Result<QueryResponse> {
        self.send_ws_request(|r| {
            r.query_mailbox();
        })
        .await?
        .unwrap_query_mailbox()
        .context("Expecting mailbox query response")
    }

    #[instrument(skip(self), ret, level = "debug")]
    pub async fn get_mailboxes(&self, ids: Vec<String>) -> anyhow::Result<MailboxGetResponse> {
        self.send_ws_request(move |r| {
            r.get_mailbox().ids(ids);
        })
        .await?
        .unwrap_get_mailbox()
        .context("Expecting mailbox get response")
    }

    #[instrument(skip(self), ret, level = "debug")]
    pub async fn mailboxes_changes(
        &self,
        since_state: String,
    ) -> anyhow::Result<MailboxChangesResponse> {
        self.send_ws_request(move |r| {
            r.changes_mailbox(since_state);
        })
        .await?
        .unwrap_changes_mailbox()
        .context("Expecting mailbox changes response")
    }

    #[instrument(skip(self), ret, level = "debug")]
    pub async fn query_emails(&self, query: EmailQuery) -> anyhow::Result<QueryResponse> {
        self.send_ws_request(move |req| {
            let EmailQuery {
                anchor_id,
                mailbox_id,
                search_keyword,
                sorts,
                limit,
            } = query;

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
        })
        .await?
        .unwrap_query_email()
        .context("Expecting email query response")
    }

    #[instrument(skip(self), ret, level = "debug")]
    pub async fn email_changes(&self, since_state: String) -> anyhow::Result<EmailChangesResponse> {
        self.send_ws_request(move |r| {
            r.changes_email(since_state);
        })
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
        self.send_ws_request(move |r| {
            let req = r.get_email().ids(ids);
            if let Some(props) = partial_properties {
                req.properties(props);
            }
        })
        .await?
        .unwrap_get_email()
        .context("Expecting email get response")
    }

    async fn wait_for_client(&self) -> Arc<Client> {
        let mut receiver = self.client_state.clone();

        loop {
            if let ClientState::Connected(client) = &*receiver.borrow() {
                return client.clone();
            }

            receiver.changed().await.ok();
        }
    }

    #[instrument(skip(self), err, level = "debug")]
    pub async fn download_blob(&self, blob_id: &str) -> anyhow::Result<Vec<u8>> {
        self.wait_for_client()
            .await
            .download(blob_id)
            .await
            .context("Download blob failed")
    }

    #[instrument(skip(self), ret, level = "debug")]
    pub async fn get_identities(&self) -> anyhow::Result<Vec<Identity>> {
        self.send_ws_request(|r| {
            r.get_identity();
        })
        .await?
        .unwrap_get_identity()
        .context("Expecting identity get response")
        .map(|mut r| r.take_list())
    }

    #[instrument(skip(self, draft), ret, level = "debug")]
    pub async fn create_email(&self, draft: EmailDraft) -> anyhow::Result<String> {
        self.create_email_inner(draft, false).await
    }

    /// Creates the email on the JMAP server with the `$draft` keyword set, placing it
    /// in the mailbox specified by `draft.mailbox_id` (should be the Drafts mailbox).
    #[instrument(skip(self, draft), ret, level = "debug")]
    pub async fn create_jmap_draft(&self, draft: EmailDraft) -> anyhow::Result<String> {
        self.create_email_inner(draft, true).await
    }

    async fn create_email_inner(&self, draft: EmailDraft, is_draft: bool) -> anyhow::Result<String> {
        use jmap_client::email::EmailBodyPart;

        self.send_ws_request(move |r| {
            let email = r.set_email().create();

            email.mailbox_ids([draft.mailbox_id]);

            if is_draft {
                email.keywords(["$draft"]);
            }

            if !draft.from.is_empty() {
                email.from(draft.from.iter().map(email_addr_to_jmap));
            }
            if !draft.to.is_empty() {
                email.to(draft.to.iter().map(email_addr_to_jmap));
            }
            if !draft.cc.is_empty() {
                email.cc(draft.cc.iter().map(email_addr_to_jmap));
            }
            if !draft.bcc.is_empty() {
                email.bcc(draft.bcc.iter().map(email_addr_to_jmap));
            }
            if !draft.in_reply_to.is_empty() {
                email.in_reply_to(draft.in_reply_to);
            }
            if !draft.references.is_empty() {
                email.references(draft.references);
            }
            email.subject(draft.subject);

            match (draft.text_body, draft.html_body) {
                (Some(text), Some(html)) => {
                    email.body_value("text".to_string(), text.as_str());
                    email.body_value("html".to_string(), html.as_str());
                    email.text_body(
                        EmailBodyPart::new()
                            .part_id("text")
                            .content_type("text/plain"),
                    );
                    email.html_body(
                        EmailBodyPart::new()
                            .part_id("html")
                            .content_type("text/html"),
                    );
                }
                (Some(text), None) => {
                    email.body_value("text".to_string(), text.as_str());
                    email.text_body(
                        EmailBodyPart::new()
                            .part_id("text")
                            .content_type("text/plain"),
                    );
                }
                (None, Some(html)) => {
                    email.body_value("html".to_string(), html.as_str());
                    email.html_body(
                        EmailBodyPart::new()
                            .part_id("html")
                            .content_type("text/html"),
                    );
                }
                (None, None) => {
                    email.body_value("text".to_string(), "");
                    email.text_body(
                        EmailBodyPart::new()
                            .part_id("text")
                            .content_type("text/plain"),
                    );
                }
            }

            for att in draft.attachments {
                let mut part = EmailBodyPart::new().blob_id(att.blob_id);
                if let Some(name) = att.name {
                    part = part.name(name);
                }
                if let Some(mime) = att.mime_type {
                    part = part.content_type(mime);
                }
                email.attachment(part);
            }
        })
        .await?
        .unwrap_set_email()
        .context("Expecting email set response")
        .and_then(|mut resp| {
            let create_id = resp
                .created_ids()
                .context("Email set returned no created entries")?
                .next()
                .map(|s| s.clone())
                .context("Email set created map is empty")?;
            resp.created(&create_id)
                .context("Email set created entry not found")
                .and_then(|email| {
                    email
                        .id()
                        .map(|s| s.to_string())
                        .context("Created email has no ID")
                })
        })
    }

    /// Destroys an email on the JMAP server. Used to clean up superseded draft emails.
    #[instrument(skip(self), ret, level = "debug")]
    pub async fn delete_jmap_email(&self, jmap_email_id: String) -> anyhow::Result<()> {
        self.send_ws_request(move |r| {
            r.set_email().destroy([jmap_email_id]);
        })
        .await?
        .unwrap_set_email()
        .context("Expecting email set response")
        .and_then(|resp| {
            let destroyed_ids: Vec<String> = resp
                .destroyed_ids()
                .into_iter()
                .flatten()
                .cloned()
                .collect();
            destroyed_ids
                .first()
                .context("Email destroy returned no destroyed IDs")
                .map(|_| ())
        })
    }

    #[instrument(skip(self), ret, level = "debug")]
    pub async fn submit_email(
        &self,
        email_id: String,
        identity_id: String,
    ) -> anyhow::Result<()> {
        self.send_ws_request(move |r| {
            r.set_email_submission()
                .create()
                .email_id(email_id)
                .identity_id(identity_id);
        })
        .await?
        .unwrap_set_email_submission()
        .context("Expecting email submission set response")
        .and_then(|mut resp| {
            let create_id = resp
                .created_ids()
                .context("Email submission set returned no created entries")?
                .next()
                .map(|s| s.clone())
                .context("Email submission created map is empty")?;
            resp.created(&create_id)
                .context("Email submission created entry not found")
        })?;
        Ok(())
    }

    #[instrument(skip(self, data), level = "debug")]
    pub async fn upload_blob(
        &self,
        data: Vec<u8>,
        content_type: Option<String>,
    ) -> anyhow::Result<String> {
        self.wait_for_client()
            .await
            .upload(None, data, content_type.as_deref())
            .await
            .context("Blob upload failed")
            .map(|mut r| r.take_blob_id())
    }
}

fn email_addr_to_jmap(addr: &EmailAddressInput) -> jmap_client::email::EmailAddress {
    if let Some(name) = &addr.name {
        (name.clone(), addr.email.clone()).into()
    } else {
        addr.email.clone().into()
    }
}
