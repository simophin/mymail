use crate::jmap_account::AccountId;
use crate::jmap_api::JmapApi;
use crate::repo::Repository;
use anyhow::Context;
use derive_more::Debug;
use jmap_client::email::{Email, Property};
use tokio::sync::oneshot;

#[derive(Debug)]
pub struct FetchEmailDetailsCommand {
    pub email_id: String,

    #[debug(skip)]
    pub callback: oneshot::Sender<anyhow::Result<Email>>,
}

pub async fn handle_fetch_email_details_command(
    account_id: AccountId,
    jmap_api: &JmapApi,
    repo: &Repository,
    email_id: &str,
) -> anyhow::Result<Email> {
    let email = jmap_api
        .get_emails(
            vec![email_id.to_string()],
            Some(vec![
                Property::BodyValues,
                Property::Attachments,
                Property::BodyStructure,
            ]),
        )
        .await
        .context("Error fetching email details")?
        .take_list()
        .pop()
        .context("Email not found")?;

    repo.update_email_details(account_id, &email_id, &email)
        .await
        .context("Error updating email")?;

    Ok(email)
}
