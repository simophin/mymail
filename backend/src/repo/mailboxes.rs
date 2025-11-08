use crate::jmap_account::AccountId;
use anyhow::Context;
use jmap_client::mailbox::Mailbox;

impl super::Repository {
    pub async fn get_mailboxes_sync_state(
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

    pub async fn update_mailboxes(
        &self,
        account_id: AccountId,
        new_state: &str,
        updated: Vec<Mailbox>,
        deleted: Vec<String>,
    ) -> anyhow::Result<()> {
        let mut tx = self.pool().begin().await?;

        let num_inserted = if updated.is_empty() {
            0
        } else {
            let updated = serde_json::to_string(&updated).context("Error serializing mailboxes")?;
            sqlx::query!(
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
            .rows_affected()
        };

        let num_deleted = if deleted.is_empty() {
            0
        } else {
            let deleted =
                serde_json::to_string(&deleted).context("Error serializing deletion ids")?;
            sqlx::query!(
                "DELETE FROM mailboxes
            WHERE account_id = ? AND id IN (SELECT value FROM json_each(?))
            ",
                account_id,
                deleted
            )
            .execute(&mut *tx)
            .await
            .context("Error deleting mailboxes")?
            .rows_affected()
        };

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

    pub async fn get_mailboxes(&self, account_id: AccountId) -> anyhow::Result<Vec<Mailbox>> {
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

    pub async fn get_mailbox_ids(&self, account_id: AccountId) -> anyhow::Result<Vec<String>> {
        let rows = sqlx::query!("SELECT id FROM mailboxes WHERE account_id = ?", account_id)
            .fetch_all(self.pool())
            .await
            .context("Error querying mailbox IDs")?;

        Ok(rows.into_iter().map(|row| row.id).collect())
    }

    pub async fn get_mailbox_email_sync_state(
        &self,
        account_id: AccountId,
        mailbox_id: &str,
    ) -> anyhow::Result<Option<String>> {
        Ok(sqlx::query!(
            "SELECT email_sync_state FROM mailboxes WHERE account_id = ? AND id = ?",
            account_id,
            mailbox_id
        )
        .fetch_optional(self.pool())
        .await
        .context("Error querying mailbox email sync state")?
        .context("Mailbox not found")?
        .email_sync_state)
    }

    pub async fn set_mailbox_email_sync_state(
        &self,
        account_id: AccountId,
        mailbox_id: &str,
        sync_state: &str,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "UPDATE mailboxes SET email_sync_state = ? WHERE account_id = ? AND id = ?",
            sync_state,
            account_id,
            mailbox_id
        )
        .execute(self.pool())
        .await
        .context("Error updating mailbox email sync state")?;
        Ok(())
    }
}
