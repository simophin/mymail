use crate::jmap_account::AccountRepositoryExt;
use futures::future::try_join_all;

mod jmap_account;
mod jmap_repo;
mod jmap_sync;
mod repo;

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt::init();

    let database_file = std::env::var("DATABASE_FILE").unwrap_or(String::from(":memory:"));

    tracing::info!("Using database {database_file}");

    let repo = repo::Repository::new(&database_file)
        .await
        .expect("Failed to initialize DB repository");

    let accounts = match repo.list_accounts().await.expect("Failed to list accounts") {
        accounts if accounts.is_empty() => {
            tracing::info!("No accounts found in the database.");
            let server_url = std::env::var("JMAP_SERVER_URL")
                .expect("Missing JMAP_SERVER_URL environment variable");
            let username =
                std::env::var("JMAP_USERNAME").expect("Missing JMAP_USERNAME environment variable");
            let password =
                std::env::var("JMAP_PASSWORD").expect("Missing JMAP_PASSWORD environment variable");

            let account = jmap_account::Account {
                server_url: server_url.clone(),
                credentials: jmap_account::Credentials::Basic { username, password },
                name: String::from("default"),
            };
            repo.add_account(&account)
                .await
                .expect("Failed to add account");

            repo.list_accounts().await.expect("Failed to list accounts")
        }

        accounts => accounts,
    };

    let sync_tasks = accounts
        .into_iter()
        .map(|(account_id, _)| jmap_sync::run_jmap_sync(&repo, account_id));

    try_join_all(sync_tasks)
        .await
        .expect("Failed to sync accounts");
}
