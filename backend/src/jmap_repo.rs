use crate::jmap_account::AccountId;
use crate::jmap_api::{EmailSort, EmailSortColumn};
use crate::repo::Repository;
use anyhow::Context;
use itertools::Itertools;
use jmap_client::email::Email;
use jmap_client::mailbox::Mailbox;
use serde::Deserialize;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;
use std::collections::HashSet;

#[derive(Debug, Deserialize, Clone)]
pub struct EmailDbQuery {
    pub account_id: AccountId,
    pub mailbox_id: Option<String>,
    pub search_keyword: Option<String>,
    pub sorts: Vec<EmailSort>,
    pub limit: usize,
    pub offset: usize,
}

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

    async fn get_emails(&self, query: &EmailDbQuery) -> anyhow::Result<Vec<Email>>;

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

    async fn get_emails(&self, query: &EmailDbQuery) -> anyhow::Result<Vec<Email>> {
        let sort_clause = query
            .sorts
            .iter()
            .map(|sort| (sort.column.to_sql_column(), sort.asc))
            .chain(std::iter::once(("id", true)))
            .map(|(column, asc)| {
                if asc {
                    column.to_string()
                } else {
                    format!("{column} DESC")
                }
            })
            .join(", ");

        //language=sqlite
        sqlx::query(&format!(
            "
            SELECT jmap_data FROM emails
            WHERE account_id = :account_id
                AND (
                    :mailbox_id IS NULL OR
                        EXISTS (SELECT 1 FROM mailbox_emails me
                                WHERE me.account_id = :account_id
                                  AND me.email_id = emails.id
                                  AND me.mailbox_id = :mailbox_id)
                )
                AND (
                    :search_keyword IS NULL OR
                    subject LIKE '%' || :search_keyword || '%'
                )
            ORDER BY {sort_clause}
            LIMIT :offset, :limit
        "
        ))
        .bind(query.account_id)
        .bind(query.mailbox_id.as_ref())
        .bind(query.search_keyword.as_ref())
        .bind(query.offset as i64)
        .bind(query.limit as i64)
        .try_map(|row: SqliteRow| {
            serde_json::from_str::<Email>(&row.get::<String, _>(0))
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))
        })
        .fetch_all(self.pool())
        .await
        .context("Error querying emails")
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

impl EmailSortColumn {
    fn to_sql_column(&self) -> &'static str {
        match self {
            Self::Date => "received_at",
        }
    }
}
