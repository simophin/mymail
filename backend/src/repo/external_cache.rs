use crate::jmap_account::AccountId;
use anyhow::Context;

pub struct ExternalCache {
    pub data: Vec<u8>,
    pub mime_type: Option<String>,
}

impl super::Repository {
    pub async fn get_external_cache(
        &self,
        account_id: AccountId,
        url: &str,
    ) -> anyhow::Result<Option<ExternalCache>> {
        sqlx::query_as!(
            ExternalCache,
            "UPDATE external_cache SET last_accessed = CURRENT_TIMESTAMP WHERE account_id = ? AND url = ? RETURNING value AS data, mime_type",
            account_id,
            url
        ).fetch_optional(self.pool())
            .await
            .context("Failed to fetch external cache from database")
    }

    pub async fn put_external_cache(
        &self,
        account_id: AccountId,
        url: &str,
        data: &[u8],
        mime_type: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "INSERT OR REPLACE INTO external_cache (account_id, url, value, mime_type, last_accessed)
             VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP)",
            account_id,
            url,
            data,
            mime_type
        )
        .execute(self.pool())
        .await
        .context("Failed to insert external cache into database")?;

        Ok(())
    }
}
