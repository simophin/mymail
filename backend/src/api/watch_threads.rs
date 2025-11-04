use crate::jmap_account::AccountId;
use axum::extract;
use axum::response::IntoResponse;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Pagination {
    limit: usize,
    offset: usize,
}

pub async fn watch_threads(
    state: extract::State<super::ApiState>,
    account_id: extract::Path<AccountId>,
    mailbox_id: extract::Path<String>,
    pagination: extract::Query<Pagination>,
) -> impl IntoResponse {
    let limit = pagination.limit;
    let offset = pagination.offset;

    super::query_with_db_changes(state.repo.clone(), &["emails"], move |repo| {
        let account_id = account_id.0;
        let mailbox_id = mailbox_id.0.clone();

        async move {
            repo.get_threads(account_id, &mailbox_id, offset, limit)
                .await
        }
    })
    .into_response()
}
