mod emails;
mod mailboxes;
mod threads;

use anyhow::Context;
use sqlx::SqlitePool;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteQueryResult};
use std::sync::Arc;
use tokio::sync::broadcast;

pub use emails::EmailDbQuery;
pub use threads::Thread;

#[derive(Clone)]
pub struct Changes {
    pub tables: Arc<[&'static str]>,
}

pub struct Repository {
    pool: SqlitePool,
    changes: broadcast::Sender<Changes>,
}

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

impl Repository {
    pub async fn new(database_file: &str) -> anyhow::Result<Self> {
        let pool = SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(database_file)
                .create_if_missing(true),
        )
        .await
        .context("Failed to connect to the database")?;

        MIGRATOR
            .run(&pool)
            .await
            .context("Failed to run database migrations")?;

        let (changes, _) = broadcast::channel(16);

        Ok(Self { pool, changes })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn notify_changes(&self, tables: &[&'static str]) {
        let _ = self.changes.send(Changes {
            tables: Arc::from(tables),
        });
    }

    pub fn notify_changes_with(&self, result: SqliteQueryResult, tables: &[&'static str]) {
        if result.rows_affected() > 0 {
            self.notify_changes(tables);
        }
    }

    pub fn subscribe_db_changes(&self) -> broadcast::Receiver<Changes> {
        self.changes.subscribe()
    }
}
