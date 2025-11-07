use super::ApiState;
use crate::jmap_account::AccountId;
use crate::repo::EmailDbQuery;
use axum::extract;
use axum::extract::Path;
use axum::response::IntoResponse;
use std::sync::Arc;

pub async fn watch_mail(
    account_id: Path<AccountId>,
    state: extract::State<ApiState>,
    query: extract::Query<EmailDbQuery>,
    upgrade: extract::ws::WebSocketUpgrade,
) -> impl IntoResponse {
    let query = Arc::new(query.0);

    super::stream::websocket_db_stream(upgrade, state.repo.clone(), &["emails"], move |repo| {
        let account_id = account_id.0;
        let query = query.clone();
        async move { repo.get_emails(account_id, &query).await }
    })
}
