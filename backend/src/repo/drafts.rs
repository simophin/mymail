use crate::jmap_api::EmailDraft;
use crate::repo::Repository;
use anyhow::Context;
use uuid::Uuid;

pub struct DraftRecord {
    pub id: String,
    /// Set after the draft has been successfully synced to the JMAP server.
    pub jmap_email_id: Option<String>,
    pub data: EmailDraft,
    pub updated_at: i64,
}

pub trait DraftRepositoryExt {
    async fn create_draft(
        &self,
        account_id: i64,
        data: &EmailDraft,
    ) -> anyhow::Result<DraftRecord>;

    async fn update_draft_data(
        &self,
        account_id: i64,
        id: &str,
        data: &EmailDraft,
    ) -> anyhow::Result<()>;

    async fn set_draft_jmap_id(
        &self,
        account_id: i64,
        id: &str,
        jmap_email_id: &str,
    ) -> anyhow::Result<()>;

    async fn clear_draft_jmap_id(&self, account_id: i64, id: &str) -> anyhow::Result<()>;

    async fn get_draft(
        &self,
        account_id: i64,
        id: &str,
    ) -> anyhow::Result<Option<DraftRecord>>;

    async fn list_drafts(&self, account_id: i64) -> anyhow::Result<Vec<DraftRecord>>;

    async fn delete_draft(&self, account_id: i64, id: &str) -> anyhow::Result<()>;
}

impl DraftRepositoryExt for Repository {
    async fn create_draft(
        &self,
        account_id: i64,
        data: &EmailDraft,
    ) -> anyhow::Result<DraftRecord> {
        let id = Uuid::new_v4().to_string();
        let data_json = serde_json::to_string(data).context("Failed to serialize draft")?;

        let updated_at = sqlx::query_scalar!(
            "INSERT INTO drafts (id, account_id, data) VALUES (?, ?, ?) RETURNING updated_at",
            id,
            account_id,
            data_json,
        )
        .fetch_one(self.pool())
        .await
        .context("Failed to insert draft")?;

        Ok(DraftRecord {
            id,
            jmap_email_id: None,
            data: data.clone(),
            updated_at,
        })
    }

    async fn update_draft_data(
        &self,
        account_id: i64,
        id: &str,
        data: &EmailDraft,
    ) -> anyhow::Result<()> {
        let data_json = serde_json::to_string(data).context("Failed to serialize draft")?;

        let rows = sqlx::query!(
            "UPDATE drafts SET data = ?, updated_at = unixepoch()
             WHERE id = ? AND account_id = ?",
            data_json,
            id,
            account_id,
        )
        .execute(self.pool())
        .await
        .context("Failed to update draft")?
        .rows_affected();

        if rows == 0 {
            anyhow::bail!("Draft not found: {id}");
        }

        Ok(())
    }

    async fn set_draft_jmap_id(
        &self,
        account_id: i64,
        id: &str,
        jmap_email_id: &str,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "UPDATE drafts SET jmap_email_id = ? WHERE id = ? AND account_id = ?",
            jmap_email_id,
            id,
            account_id,
        )
        .execute(self.pool())
        .await
        .context("Failed to set draft jmap_email_id")?;

        Ok(())
    }

    async fn clear_draft_jmap_id(&self, account_id: i64, id: &str) -> anyhow::Result<()> {
        sqlx::query!(
            "UPDATE drafts SET jmap_email_id = NULL WHERE id = ? AND account_id = ?",
            id,
            account_id,
        )
        .execute(self.pool())
        .await
        .context("Failed to clear draft jmap_email_id")?;

        Ok(())
    }

    async fn get_draft(
        &self,
        account_id: i64,
        id: &str,
    ) -> anyhow::Result<Option<DraftRecord>> {
        let rec = sqlx::query!(
            "SELECT id, jmap_email_id, data, updated_at
             FROM drafts WHERE id = ? AND account_id = ?",
            id,
            account_id,
        )
        .fetch_optional(self.pool())
        .await
        .context("Failed to query draft")?;

        rec.map(|r| {
            Ok(DraftRecord {
                id: r.id,
                jmap_email_id: r.jmap_email_id,
                data: serde_json::from_str(&r.data)
                    .context("Failed to deserialize draft data")?,
                updated_at: r.updated_at,
            })
        })
        .transpose()
    }

    async fn list_drafts(&self, account_id: i64) -> anyhow::Result<Vec<DraftRecord>> {
        let recs = sqlx::query!(
            "SELECT id, jmap_email_id, data, updated_at
             FROM drafts WHERE account_id = ?
             ORDER BY updated_at DESC",
            account_id,
        )
        .fetch_all(self.pool())
        .await
        .context("Failed to list drafts")?;

        recs.into_iter()
            .map(|r| {
                Ok(DraftRecord {
                    id: r.id,
                    jmap_email_id: r.jmap_email_id,
                    data: serde_json::from_str(&r.data)
                        .context("Failed to deserialize draft data")?,
                    updated_at: r.updated_at,
                })
            })
            .collect()
    }

    async fn delete_draft(&self, account_id: i64, id: &str) -> anyhow::Result<()> {
        sqlx::query!(
            "DELETE FROM drafts WHERE id = ? AND account_id = ?",
            id,
            account_id,
        )
        .execute(self.pool())
        .await
        .context("Failed to delete draft")?;

        Ok(())
    }
}
