use super::ApiState;
use crate::jmap_account::AccountId;
use crate::util::http_error::{AnyhowHttpError, HttpResult};
use anyhow::Context;
use axum::Json;
use axum::extract;
use jmap_client::identity::Identity;

pub async fn get_identities(
    state: extract::State<ApiState>,
    extract::Path(account_id): extract::Path<AccountId>,
) -> HttpResult<Json<Vec<Identity>>> {
    let api = state
        .account_states
        .read()
        .get(&account_id)
        .map(|s| s.jmap_api.clone())
        .context("Account not found")
        .into_not_found_error_result()?;

    let identities = api
        .get_identities()
        .await
        .context("Failed to fetch identities")
        .into_internal_error_result()?;

    Ok(Json(identities))
}
