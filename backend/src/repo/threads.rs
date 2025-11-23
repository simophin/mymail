use crate::jmap_account::AccountId;
use anyhow::Context;
use serde::Serialize;
use serde_json::value::RawValue;
use std::time::Instant;

#[derive(Debug, Serialize)]
pub struct Thread {
    pub id: String,
    pub emails: Box<RawValue>,
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

        let start = Instant::now();

        let r = sqlx::query!(
            r#"
           WITH threads AS (
                SELECT thread_id, MAX(received_at) AS last_received_at, json_group_array(email_id) AS email_ids
                FROM mailbox_emails
                WHERE account_id = ?1 AND mailbox_id = ?2
                GROUP BY thread_id
                ORDER BY last_received_at DESC, thread_id
                LIMIT ?3, ?4
            )
            SELECT thread_id,
                     (SELECT json_group_array(json(e.jmap_data) ORDER BY e.received_at DESC) FROM emails e WHERE e.account_id = ?1 AND e.id IN (SELECT value FROM json_each(email_ids))) AS "emails!: String"
            FROM threads
            ORDER BY last_received_at DESC, thread_id
            "#,
            account_id,
            mailbox_id,
            offset,
            limit
        )
        .try_map(|r| {
            Ok(Thread {
                id: r.thread_id,
                emails: RawValue::from_string(r.emails)
                    .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            })
        })
        .fetch_all(self.pool())
        .await
        .context("Failed to fetch threads");

        tracing::info!("Fetched threads in {:?}ms", start.elapsed().as_millis());

        r
    }
}
