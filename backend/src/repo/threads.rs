use crate::jmap_account::AccountId;
use anyhow::Context;
use jmap_client::email::Email;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Thread {
    pub id: String,
    pub emails: Vec<Email>,
}

impl super::Repository {
    pub async fn get_threads(
        &self,
        account_id: AccountId,
        mailbox_id: &str,
        offset: usize,
        limit: usize,
    ) -> anyhow::Result<Vec<Thread>> {
        let offset = offset as i64;
        let limit = limit as i64;

        sqlx::query!(
            r#"
            SELECT
                thread_id AS "thread_id!",
                json_group_array(json(jmap_data) ORDER BY received_at, sent_at) AS "emails!: String"
            FROM emails
            WHERE account_id = ?1 AND id IN (
                SELECT me.email_id FROM mailbox_emails me
                WHERE me.account_id = ?1 AND me.mailbox_id = ?2
            )
            GROUP BY thread_id
            ORDER BY MAX(received_at) DESC, thread_id
            LIMIT ?3, ?4
            "#,
            account_id,
            mailbox_id,
            offset,
            limit
        )
        .try_map(|r| {
            Ok(Thread {
                id: r.thread_id,
                emails: serde_json::from_str(&r.emails)
                    .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            })
        })
        .fetch_all(self.pool())
        .await
        .context("Failed to fetch threads")
    }
}
