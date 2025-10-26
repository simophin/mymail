use crate::repo::Repository;
use anyhow::Context;
use serde::{Deserialize, Serialize};

pub struct Account {
    pub server_url: String,
    pub credentials: Credentials,
    pub name: String,
}

pub type AccountId = i64;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Credentials {
    Basic { username: String, password: String },
}

pub trait AccountRepositoryExt {
    async fn get_account(&self, account_id: AccountId) -> anyhow::Result<Option<Account>>;
    async fn list_accounts(&self) -> anyhow::Result<Vec<(AccountId, Account)>>;
    async fn add_account(&self, account: &Account) -> anyhow::Result<AccountId>;
}

impl AccountRepositoryExt for Repository {
    async fn get_account(&self, account_id: AccountId) -> anyhow::Result<Option<Account>> {
        let record = sqlx::query!(
            "SELECT url, credentials, name FROM accounts WHERE id = ?",
            account_id
        )
        .fetch_optional(self.pool())
        .await
        .context("Error querying account")?;

        if let Some(rec) = record {
            Ok(Some(Account {
                server_url: rec.url,
                credentials: serde_json::from_str(&rec.credentials)
                    .context("Error deserializing account credentials")?,
                name: rec.name,
            }))
        } else {
            Ok(None)
        }
    }

    async fn list_accounts(&self) -> anyhow::Result<Vec<(AccountId, Account)>> {
        let records = sqlx::query!("SELECT id, url, credentials, name  FROM accounts")
            .fetch_all(self.pool())
            .await
            .context("Error querying accounts")?;

        Ok(records
            .into_iter()
            .map(|rec| {
                (
                    rec.id,
                    Account {
                        server_url: rec.url,
                        credentials: serde_json::from_str(&rec.credentials)
                            .context("Error deserializing account credentials")
                            .unwrap(),
                        name: rec.name,
                    },
                )
            })
            .collect())
    }

    async fn add_account(&self, account: &Account) -> anyhow::Result<AccountId> {
        let credentials = serde_json::to_string(&account.credentials)
            .context("Error serializing account credentials")?;

        Ok(sqlx::query!(
            "INSERT INTO accounts (url, credentials, name) VALUES (?, ?, ?) RETURNING id",
            account.server_url,
            credentials,
            account.name
        )
        .fetch_one(self.pool())
        .await
        .context("Error inserting account")?
        .id)
    }
}
