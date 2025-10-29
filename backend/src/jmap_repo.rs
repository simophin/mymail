use crate::jmap_account::AccountId;
use crate::jmap_api::EmailQuery;
use crate::repo::Repository;
use anyhow::Context;
use jmap_client::email::Email;
use jmap_client::mailbox::Mailbox;
use std::collections::HashSet;

pub trait JmapRepositoryExt {
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

    async fn find_downloaded_email_ids(
        &self,
        account_id: AccountId,
        mail_ids: &[String],
    ) -> anyhow::Result<HashSet<String>>;

    async fn delete_emails(
        &self,
        account_id: AccountId,
        email_ids: &[String],
    ) -> anyhow::Result<()>;

    async fn update_emails(&self, account_id: AccountId, emails: &[Email]) -> anyhow::Result<()>;

    async fn get_emails(
        &self,
        account_id: AccountId,
        query: &EmailQuery,
    ) -> anyhow::Result<Vec<Email>>;

    async fn get_mailboxes(&self, account_id: AccountId) -> anyhow::Result<Vec<Mailbox>>;
}

impl JmapRepositoryExt for Repository {
    async fn get_mailboxes_sync_state(
        &self,
        account_id: AccountId,
    ) -> anyhow::Result<Option<String>> {
        Ok(sqlx::query!(
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

        let updated = serde_json::to_string(&updated).context("Error serializing mailboxes")?;

        let num_inserted = sqlx::query!(
            "INSERT INTO mailboxes (account_id, id, jmap_data)
            SELECT ?, value->>'$.id', value FROM json_each(?)
            WHERE true
            ON CONFLICT DO UPDATE
                SET jmap_data = EXCLUDED.jmap_data
            ",
            account_id,
            updated
        )
        .execute(&mut *tx)
        .await
        .context("Error updating mailboxes")?
        .rows_affected();

        let deleted = serde_json::to_string(&deleted).context("Error serializing deletion ids")?;
        let num_deleted = sqlx::query!(
            "DELETE FROM mailboxes
            WHERE account_id = ? AND id IN (SELECT value FROM json_each(?))
            ",
            account_id,
            deleted
        )
        .execute(&mut *tx)
        .await
        .context("Error deleting mailboxes")?
        .rows_affected();

        sqlx::query!(
            "UPDATE accounts SET mailboxes_sync_state = ? WHERE id = ?",
            new_state,
            account_id
        )
        .execute(&mut *tx)
        .await
        .context("Error updating mailboxes sync state")?;

        tx.commit().await?;

        if num_inserted > 0 || num_deleted > 0 {
            self.notify_changes(&["mailboxes"]);
        }

        Ok(())
    }

    async fn find_downloaded_email_ids(
        &self,
        account_id: AccountId,
        mail_ids: &[String],
    ) -> anyhow::Result<HashSet<String>> {
        let mail_ids = serde_json::to_string(mail_ids)?;
        let rows = sqlx::query!(
            "SELECT id FROM emails
             WHERE account_id = ? AND id IN (SELECT value FROM json_each(?)) AND jmap_data IS NULL
             ",
            account_id,
            mail_ids,
        )
        .fetch_all(self.pool())
        .await
        .context("Error querying undownloaded email IDs")?;

        Ok(rows.into_iter().map(|row| row.id).collect())
    }

    async fn delete_emails(
        &self,
        account_id: AccountId,
        email_ids: &[String],
    ) -> anyhow::Result<()> {
        let email_ids = serde_json::to_string(email_ids)?;
        let result = sqlx::query!(
            "DELETE FROM emails WHERE account_id = ? AND id IN (SELECT value FROM json_each(?))",
            account_id,
            email_ids
        )
        .execute(self.pool())
        .await
        .context("Error deleting emails")?;

        self.notify_changes_with(result, &["emails"]);
        Ok(())
    }

    async fn update_emails(&self, account_id: AccountId, emails: &[Email]) -> anyhow::Result<()> {
        let emails = serde_json::to_string(emails).context("Error serializing emails")?;

        let changes = sqlx::query!(
            "INSERT INTO emails (account_id, id, jmap_data)
            SELECT ?, value->>'$.id', value FROM json_each(?)
            WHERE true
            ON CONFLICT DO UPDATE
                SET jmap_data = EXCLUDED.jmap_data
                WHERE jmap_data IS NULL
            ",
            account_id,
            emails
        )
        .execute(self.pool())
        .await
        .context("Error updating emails")?;

        self.notify_changes_with(changes, &["emails"]);

        Ok(())
    }

    async fn get_emails(
        &self,
        account_id: AccountId,
        query: &EmailQuery,
    ) -> anyhow::Result<Vec<Email>> {
        //TODO: implement query filtering
        Ok(vec![])
    }

    async fn get_mailboxes(&self, account_id: AccountId) -> anyhow::Result<Vec<Mailbox>> {
        sqlx::query!(
            "SELECT jmap_data FROM mailboxes WHERE account_id = ?",
            account_id
        )
        .fetch_all(self.pool())
        .await
        .context("Error querying mailboxes")?
        .into_iter()
        .map(|r| {
            serde_json::from_str::<Mailbox>(&r.jmap_data).context("Error deserializing mailbox")
        })
        .collect()
    }
}
