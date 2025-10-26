use anyhow::Context;
use futures::FutureExt;
use futures::future::join_all;
use jmap_client::client::Client;
use jmap_client::core::query::Comparator;
use jmap_client::mailbox;

pub async fn run_jmap_sync() -> anyhow::Result<()> {
    let server_url = std::env::var("JMAP_SERVER_URL").context("Missing jmap server url")?;
    let client = Client::new()
        .follow_redirects(["mail.fanchao.dev"])
        .credentials((
            std::env::var("JMAP_USERNAME").context("Missing jmap user name")?,
            std::env::var("JMAP_PASSWORD").context("Missing jmap password")?,
        ))
        .connect(&server_url)
        .await
        .context("Failed to connect to JMAP server")?;

    tracing::info!("Connected to JMAP server");

    let mailboxes = client
        .mailbox_query(
            Option::<mailbox::query::Filter>::None,
            Option::<[Comparator<mailbox::query::Comparator>; 0]>::None,
        )
        .await
        .context("Failed to query mailboxes")?;

    tracing::info!("Found {} mailboxes", mailboxes.ids().len());

    let results = join_all(mailboxes.ids().iter().map(|id| {
        client
            .mailbox_get(&id, Option::<[mailbox::Property; 0]>::None)
            .map(move |r| (id, r))
    }))
    .await;

    for (id, result) in results {
        match result {
            Ok(Some(mailbox)) => {
                tracing::info!("Mailbox: {mailbox:?}");
            }
            Ok(None) => {
                tracing::warn!("Mailbox {id} not found");
            }
            Err(e) => {
                tracing::error!(?e, "Failed to get mailbox {id}");
            }
        }
    }

    Ok(())
}

async fn sync_mailbox(client: &Client, id: &str) -> anyhow::Result<()> {
    let mut state: Option<String> = None;

    loop {
        if let Some(s) = std::mem::take(&mut state) {
            let resp = client
                .mailbox_changes(s, 100)
                .await
                .context("Failed to get mailbox changes")?;
            state.replace(resp.new_state().to_string());
        }
    }

    let mailbox = client
        .mailbox_get(id, Option::<[mailbox::Property; 0]>::None)
        .await
        .context("Failed to get mailbox")?
        .context("Mailbox not found")?;

    tracing::info!("Mailbox: {mailbox:?}");

    Ok(())
}
