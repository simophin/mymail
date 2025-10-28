use crate::jmap_account::AccountId;
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
                "INSERT INTO mailboxes
                    (id, account_id, name, role, sort_order, total_emails, unread_emails, total_threads, unread_threads, my_rights, parent_id)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT DO UPDATE SET
                    name = EXCLUDED.name,
                    role = EXCLUDED.role,
                    sort_order = EXCLUDED.sort_order,
                    total_emails = EXCLUDED.total_emails,
                    unread_emails = EXCLUDED.unread_emails,
                    total_threads = EXCLUDED.total_threads,
                    unread_threads = EXCLUDED.unread_threads,
                    my_rights = EXCLUDED.my_rights,
                    parent_id = EXCLUDED.parent_id",
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
                "DELETE FROM mailboxes WHERE account_id = :account_id AND id = :deleted_id",
                account_id,
                deleted_id
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
        let mut tx = self.pool().begin().await?;

        for email_id in email_ids {
            sqlx::query!(
                "DELETE FROM emails WHERE account_id = ? AND id = ?",
                account_id,
                email_id
            )
            .execute(&mut *tx)
            .await
            .context("Error deleting email")?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn update_emails(&self, account_id: AccountId, emails: &[Email]) -> anyhow::Result<()> {
        let emails = serde_json::to_string(emails).context("Error serializing emails")?;

        sqlx::query!(
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

        Ok(())
    }
}
