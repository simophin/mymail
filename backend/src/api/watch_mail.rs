use super::ApiState;
use crate::jmap_account::AccountId;
use crate::jmap_repo::{EmailDbQuery, JmapRepositoryExt};
use axum::extract;
use axum::extract::Path;
use axum::response::IntoResponse;
use std::sync::Arc;

pub async fn watch_mail(
    account_id: Path<AccountId>,
    state: extract::State<ApiState>,
    query: extract::Json<EmailDbQuery>,
) -> impl IntoResponse {
    let query = Arc::new(query.0.clone());

    super::query_with_db_changes(state.repo.clone(), &["emails"], move |repo| {
        let account_id = account_id.0;
        let query = query.clone();
        async move { repo.get_emails(account_id, &query).await }
    })
    .into_response()
}
