mod jmap_sync;

use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteConnectOptions;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt::init();

    let database_file = std::env::var("DATABASE_FILE").unwrap_or(String::from(":memory:"));

    tracing::info!("Using database {database_file}");

    let pool = sqlx::SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .create_if_missing(true)
            .filename(database_file),
    )
    .await
    .expect("Failed to connect to the database");

    MIGRATOR
        .run(&pool)
        .await
        .expect("Failed to run database migrations");

    jmap_sync::run_jmap_sync().await.unwrap();
}
