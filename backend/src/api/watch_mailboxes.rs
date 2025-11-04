use super::ApiState;
use crate::jmap_account::AccountId;
use axum::extract;
use axum::response::IntoResponse;

pub async fn watch_mailboxes(
    account_id: extract::Path<AccountId>,
    state: extract::State<ApiState>,
) -> impl IntoResponse {
    super::query_with_db_changes(state.repo.clone(), &["mailboxes"], move |repo| {
        let account_id = account_id.0;
        async move { repo.get_mailboxes(account_id).await }
    })
}
