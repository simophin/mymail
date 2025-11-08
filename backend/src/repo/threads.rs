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
            SELECT me.account_id,
                   me.thread_id AS "thread_id!",
                   me.mailbox_id,
                   json_group_array(json(e.jmap_data)) as "emails!: String",
                   MAX(me.received_at)           as last_received_at
            FROM mailbox_emails me
            INNER JOIN emails e ON e.account_id = me.account_id AND e.id = me.email_id
            GROUP BY me.account_id, me.mailbox_id, me.thread_id
            ORDER BY last_received_at DESC
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
