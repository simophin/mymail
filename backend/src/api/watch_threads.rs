use crate::jmap_account::AccountId;
use axum::extract;
use axum::response::IntoResponse;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct ThreadQuery {
    mailbox_id: String,
    limit: usize,
    offset: usize,
}

pub async fn watch_threads(
    state: extract::State<super::ApiState>,
    account_id: extract::Path<AccountId>,
    extract::Query(ThreadQuery {
        mailbox_id,
        limit,
        offset,
    }): extract::Query<ThreadQuery>,
    upgrade: extract::ws::WebSocketUpgrade,
) -> impl IntoResponse {
    super::stream::websocket_db_stream(upgrade, state.repo.clone(), &["emails"], move |repo| {
        let account_id = account_id.0;
        let mailbox_id = mailbox_id.clone();
        async move {
            repo.get_threads(account_id, &mailbox_id, offset, limit)
                .await
        }
    })
}
