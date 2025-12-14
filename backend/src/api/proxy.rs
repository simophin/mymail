use super::ApiState;
use crate::util::http_error::{AnyhowHttpError, HttpResult};
use anyhow::Context;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, CONTENT_LENGTH};
use axum::response::Response;
use reqwest::header::CONTENT_TYPE;
use serde::Deserialize;
use tracing::instrument;
use url::Url;

#[derive(Deserialize)]
pub struct QueryParams {
    pub url: Url,
}

#[instrument(skip(state))]
pub async fn proxy(
    State(state): State<ApiState>,
    Query(QueryParams { url }): Query<QueryParams>,
) -> HttpResult<Response> {
    if !url.scheme().eq_ignore_ascii_case("http") && !url.scheme().eq_ignore_ascii_case("https") {
        return Err((
            StatusCode::BAD_REQUEST,
            "Only http and https schemes are supported",
        )
            .into());
    }

    let downloaded_resp = state
        .http_client
        .get(url.clone())
        .send()
        .await
        .context("Error proxying request")
        .into_internal_error_result()?;

    let mut resp_builder = Response::builder();

    if let Some(content_type) = downloaded_resp.headers().get(CONTENT_TYPE) {
        resp_builder = resp_builder.header(CONTENT_TYPE, content_type);
    }

    if let Some(content_length) = downloaded_resp.headers().get(CONTENT_LENGTH) {
        resp_builder = resp_builder.header(CONTENT_LENGTH, content_length);
    }

    // We want the response to be cached indefinitely by the browser
    resp_builder = resp_builder.header(CACHE_CONTROL, "public, max-age=31536000, immutable");

    resp_builder
        .body(Body::from_stream(downloaded_resp.bytes_stream()))
        .context("Error building response")
        .into_internal_error_result()
}
