use crate::jmap_account::AccountId;
use crate::jmap_api::JmapApi;
use crate::repo::Repository;
use anyhow::Context;
use jmap_client::{DataType, PushObject};
use std::sync::Arc;
use tracing::instrument;

#[instrument(skip(repo, jmap_api), ret, level = "info")]
pub async fn sync_mailbox_list(
    repo: Arc<Repository>,
    account_id: AccountId,
    jmap_api: Arc<JmapApi>,
) -> anyhow::Result<()> {
    let mut push_sub = jmap_api.subscribe_pushes();
    loop {
        let (new_state, updated, deleted) = match repo.get_mailboxes_sync_state(account_id).await? {
            Some(since_state) if !since_state.is_empty() => {
                let mut resp = jmap_api.mailboxes_changes(since_state).await?;
                let mut updated = resp.take_created();
                updated.extend(resp.take_updated());
                (resp.take_new_state(), updated, resp.take_destroyed())
            }

            _ => {
                let mut resp = jmap_api.query_mailboxes().await?;
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
            jmap_api
                .get_mailboxes(updated)
                .await
                .context("Error getting mailboxes")?
                .take_list()
        };

        repo.update_mailboxes(account_id, &new_state, updated, deleted)
            .await
            .context("Failed to update mailboxes")?;

        loop {
            match push_sub.recv().await?.as_ref() {
                PushObject::StateChange { changed }
                    if changed
                        .iter()
                        .any(|(_, m)| m.contains_key(&DataType::Mailbox)) =>
                {
                    tracing::info!("Mailboxes changed, restarting sync");
                    break;
                }

                _ => {
                    // Irrelevant push notification
                    continue;
                }
            }
        }
    }
}
