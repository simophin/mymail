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
    #[serde(rename = "mailboxId")]
    pub mailbox_id: Option<String>,
    #[serde(rename = "searchKeyword")]
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

    async fn find_missing_email_ids(
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
        query: &EmailDbQuery,
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

    async fn find_missing_email_ids(
        &self,
        account_id: AccountId,
        mail_ids: &[String],
    ) -> anyhow::Result<HashSet<String>> {
        let rows = sqlx::query(
            "SELECT value FROM json_each(?) AS ids
             WHERE NOT EXISTS (SELECT 1 FROM emails e WHERE e.account_id = ? AND e.id = ids.value)",
        )
        .bind(serde_json::to_string(mail_ids)?)
        .bind(account_id)
        .fetch_all(self.pool())
        .await
        .context("Error querying undownloaded email IDs")?;

        Ok(rows.into_iter().map(|row| row.get(0)).collect())
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
        let mut tx = self.pool().begin().await?;

        let emails_as_json = serde_json::to_string(emails).context("Error serializing emails")?;
        let mut changes = 0;

        changes += sqlx::query!(
            "INSERT INTO emails (account_id, id, jmap_data)
            SELECT ?, value->>'$.id', value FROM json_each(?)
            WHERE true
            ON CONFLICT DO UPDATE
                SET jmap_data = EXCLUDED.jmap_data
            ",
            account_id,
            emails_as_json
        )
        .execute(self.pool())
        .await
        .context("Error updating emails")?
        .rows_affected();

        let mailbox_email_ids: Vec<(_, _)> = emails
            .iter()
            .flat_map(|email| {
                email
                    .mailbox_ids()
                    .into_iter()
                    .map(|mailbox_id| (mailbox_id, email.id().unwrap()))
            })
            .collect();

        let mailbox_email_ids_json = serde_json::to_string(&mailbox_email_ids)
            .context("Error serializing mailbox-email ids")?;

        changes += sqlx::query!(
            "INSERT OR IGNORE INTO mailbox_emails (account_id, mailbox_id, email_id)
            SELECT ?, value->>'$[0]', value->>'$[1]' FROM json_each(?)",
            account_id,
            mailbox_email_ids_json
        )
        .execute(&mut *tx)
        .await
        .context("Error updating mailbox_emails")?
        .rows_affected();

        changes += sqlx::query!(
            "DELETE FROM mailbox_emails WHERE
                               (account_id, email_id) IN (SELECT ?1, value->>'$[1]' FROM json_each(?2)) AND
                               (account_id, mailbox_id, email_id) NOT IN (
                                    SELECT ?1, value->>'$[0]', value->>'$[1]' FROM json_each(?2)
                                )
            ",
            account_id,
            mailbox_email_ids_json
        )
        .execute(&mut *tx)
        .await
        .context("Error cleaning up mailbox_emails")?
        .rows_affected();

        tx.commit().await?;

        if changes > 0 {
            self.notify_changes(&["emails", "mailbox_emails"]);
        }

        Ok(())
    }

    async fn get_emails(
        &self,
        account_id: AccountId,
        query: &EmailDbQuery,
    ) -> anyhow::Result<Vec<Email>> {
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
            WHERE account_id = ?1
                AND (
                    ?2 IS NULL OR
                        EXISTS (SELECT 1 FROM mailbox_emails me
                                WHERE me.account_id = ?1
                                  AND me.email_id = emails.id
                                  AND me.mailbox_id = ?2)
                )
                AND (
                    ?3 IS NULL OR
                    subject LIKE '%' || ?3 || '%'
                )
            ORDER BY {sort_clause}
            LIMIT ?4, ?5
        "
        ))
        .bind(account_id)
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
