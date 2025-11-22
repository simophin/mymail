use crate::api::ApiState;
use crate::jmap_account::AccountRepositoryExt;
use crate::util::network::NetworkAvailability;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tower_http::cors::{AllowMethods, AllowOrigin, CorsLayer};

mod api;
mod jmap_account;
mod jmap_api;
mod repo;
mod sync;
mod util;

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

    if repo
        .list_accounts()
        .await
        .expect("Failed to list accounts")
        .is_empty()
    {
        tracing::info!("No accounts found in the database.");
        let server_url =
            std::env::var("JMAP_SERVER_URL").expect("Missing JMAP_SERVER_URL environment variable");
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
    }

    let api_state = ApiState {
        repo: repo.clone(),
        account_states: Default::default(),
    };

    let axum_app = api::build_api_router()
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::any())
                .allow_methods(AllowMethods::any()),
        )
        .with_state(api_state.clone());

    let listener = TcpListener::bind("127.0.0.1:4000")
        .await
        .expect("Failed to bind TCP listener");

    tracing::info!(
        "API server listening on http://{}",
        listener.local_addr().unwrap()
    );

    let (network_availability_tx, network_availability_rx) =
        watch::channel(NetworkAvailability { online: true });

    tokio::spawn(sync::sync_accounts(
        repo,
        api_state.account_states,
        network_availability_rx,
    ));

    axum::serve(listener, axum_app)
        .await
        .expect("Error serving axum app")
}
