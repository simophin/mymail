use super::ApiState;
use crate::jmap_account::AccountId;
use axum::extract;
use axum::response::IntoResponse;

pub async fn watch_mailboxes(
    account_id: extract::Path<AccountId>,
    state: extract::State<ApiState>,
    upgrade: extract::ws::WebSocketUpgrade,
) -> impl IntoResponse {
    let account_id = account_id.0;
    super::stream::websocket_db_stream(
        upgrade,
        state.repo.clone(),
        &["mailboxes"],
        move |repo| async move { repo.get_mailboxes(account_id).await },
    )
}
