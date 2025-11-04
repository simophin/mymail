use crate::api::ApiState;
use crate::jmap_account::{AccountId, AccountRepositoryExt};
use crate::jmap_sync::SyncCommand;
use anyhow::{Context, format_err};
use futures::TryFutureExt;
use futures::future::try_join_all;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::try_join;
use tower_http::cors::{AllowMethods, AllowOrigin, CorsLayer};

mod api;
mod future_set;
mod jmap_account;
mod jmap_api;
mod jmap_sync;
mod repo;

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt::init();

    let database_file = std::env::var("DATABASE_FILE").unwrap_or(String::from(":memory:"));

    tracing::info!("Using database {database_file}");

    let repo = Arc::new(
        repo::Repository::new(&database_file)
            .await
            .expect("Failed to initialize DB repository"),
    );

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

    let account_sync_commands: Arc<RwLock<HashMap<AccountId, mpsc::Sender<SyncCommand>>>> =
        Default::default();

    let sync_tasks = accounts.into_iter().map(|(account_id, _)| {
        let (commands_tx, commands_rx) = mpsc::channel(10);
        account_sync_commands
            .write()
            .insert(account_id, commands_tx);

        jmap_sync::run_jmap_sync(&repo, account_id, commands_rx)
    });

    let sync_tasks = try_join_all(sync_tasks).map_err(|e| format_err!("Sync task error: {e:?}"));

    // Run axum API server in a separate task
    let serve_axum_app = async {
        let axum_app = api::build_api_router()
            .layer(
                CorsLayer::new()
                    .allow_origin(AllowOrigin::any())
                    .allow_methods(AllowMethods::any()),
            )
            .with_state(ApiState {
                repo: repo.clone(),
                sync_command_sender: account_sync_commands.clone(),
            });
        let listener = TcpListener::bind("127.0.0.1:4000").await?;

        tracing::info!(
            "API server listening on http://{}",
            listener.local_addr().unwrap()
        );

        axum::serve(listener, axum_app)
            .await
            .context("Error serving axum app")
    };

    try_join!(sync_tasks, serve_axum_app).expect("Error running tasks");
}
