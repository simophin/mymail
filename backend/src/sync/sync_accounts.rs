use crate::api::AccountState;
use crate::jmap_account::{AccountId, AccountRepositoryExt};
use crate::jmap_api::JmapApi;
use crate::repo::Repository;
use crate::util::network::NetworkAvailability;
use anyhow::Context;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tracing::{Instrument, info_span, instrument};

#[instrument(skip_all, ret, level = "info")]
pub async fn sync_accounts(
    repo: Arc<Repository>,
    states: Arc<RwLock<HashMap<AccountId, AccountState>>>,
    network_availability_rx: watch::Receiver<NetworkAvailability>,
) -> anyhow::Result<()> {
    let mut changes = repo.subscribe_db_changes();
    loop {
        let accounts: HashMap<_, _> = repo
            .list_accounts()
            .await
            .context("Error getting account list")?
            .into_iter()
            .collect();

        {
            let mut states = states.write();

            // Drop states for accounts that no longer exist
            states.retain(|account_id, account| {
                let retain = accounts.contains_key(account_id);
                if !retain {
                    tracing::info!(account=?account.account, "Stop syncing account");
                }
                retain
            });

            // Add states for new accounts
            for (account_id, account) in accounts {
                match states.get(&account_id) {
                    Some(existing_state) if existing_state.account == account => {
                        // Account already being synced with the same configuration
                        continue;
                    }

                    _ => {}
                }

                tracing::info!(?account, "Start syncing account");

                let jmap_api = Arc::new(JmapApi::new(
                    account.server_url.parse().context("Invalid server URL")?,
                    account.credentials.clone(),
                    network_availability_rx.clone(),
                ));

                let mut join_set = JoinSet::new();

                let (command_sender, command_receiver) = mpsc::channel(16);

                join_set.spawn(
                    super::sync_account::sync_account(
                        repo.clone(),
                        account_id,
                        jmap_api.clone(),
                        command_receiver,
                    )
                    .instrument(info_span!("sync_account")),
                );

                states.insert(
                    account_id,
                    AccountState {
                        command_sender,
                        jmap_api,
                        join_set,
                        account,
                    },
                );
            }
        }

        loop {
            let changes = changes
                .recv()
                .await
                .context("Error receiving changes list")?;

            if changes.tables.contains(&"accounts") {
                break;
            }

            tracing::info!("Account table changed, updating account sync states.");
        }
    }
}
