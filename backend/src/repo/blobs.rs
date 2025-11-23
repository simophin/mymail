use crate::jmap_account::AccountId;
use anyhow::Context;

pub struct Blob {
    pub name: Option<String>,
    pub mime_type: Option<String>,
    pub data: Vec<u8>,
}

impl super::Repository {
    pub async fn get_blob(
        &self,
        account_id: AccountId,
        blob_id: &str,
    ) -> anyhow::Result<Option<Blob>> {
        sqlx::query_as!(
            Blob,
            "UPDATE blobs SET last_accessed = CURRENT_TIMESTAMP
             WHERE account_id = ? AND id = ?
             RETURNING name, mime_type, data",
            account_id,
            blob_id
        )
        .fetch_optional(self.pool())
        .await
        .context("Failed to fetch blob details")
    }

    pub async fn save_blob(
        &self,
        account_id: AccountId,
        id: &str,
        Blob {
            name,
            mime_type,
            data,
        }: &Blob,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "INSERT INTO blobs (account_id, id, name, mime_type, data, last_accessed)
             VALUES (?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
             ON CONFLICT DO UPDATE SET last_accessed = CURRENT_TIMESTAMP
            ",
            account_id,
            id,
            name,
            mime_type,
            data
        )
        .execute(self.pool())
        .await
        .context("Failed to save blob")?;
        Ok(())
    }
}
