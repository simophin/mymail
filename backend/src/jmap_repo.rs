use crate::jmap_account::AccountId;
use crate::repo::Repository;
use jmap_client::mailbox::Mailbox;

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
}

impl JmapRepositoryExt for Repository {
    async fn get_mailboxes_sync_state(
        &self,
        account_id: AccountId,
    ) -> anyhow::Result<Option<String>> {
        Ok(
            sqlx::query!("SELECT mailboxes_sync_state FROM accounts WHERE id = :account_id")
                .fetch_optional(self.pool())
                .await
                .context("Error querying mailboxes sync state")?
                .context("Account not found")?
                .mailboxes_sync_state,
        )
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
                VALUES (:id, :account_id, :name, :role, :sort_order, :total_emails, :unread_emails, :total_threads, :unread_threads, :my_rights, :parent_id)
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
            ).execute(&mut *tx).await.context("Error updating mailbox")?;
        }

        for deleted_id in deleted {
            sqlx::query!(
                "DELETE FROM mailboxes WHERE account_id = :account_id AND id = :deleted_id"
            )
            .execute(&mut *tx)
            .await
            .context("Error deleting mailbox")?;
        }

        sqlx::query!(
            "UPDATE accounts SET mailboxes_sync_state = :new_state WHERE id = :account_id"
        )
        .execute(&mut *tx)
        .await
        .context("Error updating mailboxes sync state")?;

        tx.commit().await?;
        Ok(())
    }
}
