use super::ApiState;
use crate::jmap_account::AccountId;
use crate::util::http_error::{AnyhowHttpError, HttpResult};
use anyhow::Context;
use axum::extract::{Path, Query, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use futures::StreamExt;
use reqwest::header::CONTENT_TYPE;
use serde::Deserialize;
use tracing::instrument;
use url::Url;

#[derive(Deserialize)]
pub struct QueryParams {
    pub url: Url,
}

const MAX_RESPONSE_SIZE: u64 = 10 * 1024 * 1024; // 10 MB

#[instrument(skip(state))]
pub async fn proxy(
    State(state): State<ApiState>,
    Path(account_id): Path<AccountId>,
    Query(QueryParams { url }): Query<QueryParams>,
) -> HttpResult<Response> {
    if !url.scheme().eq_ignore_ascii_case("http") && !url.scheme().eq_ignore_ascii_case("https") {
        return Err((
            StatusCode::BAD_REQUEST,
            "Only http and https schemes are supported",
        )
            .into());
    }

    let resp = state
        .http_client
        .get(url.clone())
        .send()
        .await
        .context("Error proxying request")
        .into_internal_error_result()?;

    let content_type = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());

    let content_size = resp.content_length();

    match content_size {
        Some(l) if l > MAX_RESPONSE_SIZE => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Response size {} exceeds maximum allowed size", l),
            )
                .into());
        }

        _ => {}
    }

    let mut data = Vec::new();
    let mut stream = resp.bytes_stream();
    loop {
        match stream.next().await {
            Some(Ok(chunk)) => {
                data.extend_from_slice(&chunk);
                if data.len() as u64 > MAX_RESPONSE_SIZE {
                    return Err((
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "Response size exceeds maximum allowed size",
                    )
                        .into());
                }
            }
            None => break,
            Some(Err(e)) => {
                return Err((
                    StatusCode::BAD_GATEWAY,
                    format!("Error reading proxied response: {}", e),
                )
                    .into());
            }
        }
    }

    state
        .repo
        .put_external_cache(account_id, url.as_str(), &data, content_type.as_deref())
        .await
        .into_internal_error_result()?;

    todo!()
}
