use super::ApiState;
use crate::util::http_error::AnyhowHttpError;
use anyhow::Context;
use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::Response;
use axum::response::IntoResponse;
use http_body_util::BodyExt;
use tracing::instrument;

#[instrument(skip(state, req))]
pub async fn static_file_or_dev_proxy(
    State(state): State<ApiState>,
    path: Option<Path<String>>,
    req: Request,
) -> impl IntoResponse {
    let path = path.map(|p| p.0).unwrap_or_default();

    let (parts, body) = req.into_parts();
    let req = parts.headers.into_iter().fold(
        state
            .http_client
            .request(parts.method, format!("http://localhost:3000/{path}")),
        |req, (key, value)| {
            if let Some(key) = key {
                req.header(key, value)
            } else {
                req
            }
        },
    );

    async {
        let body = body
            .collect()
            .await
            .context("Collecting body failed")?
            .to_bytes();

        let resp = req
            .body(body)
            .send()
            .await
            .context("Sending request to dev server failed")?;

        let resp_builder = resp
            .headers()
            .iter()
            .fold(Response::builder(), |resp, (key, value)| {
                resp.header(key, value)
            });

        resp_builder
            .body(Body::from_stream(resp.bytes_stream()))
            .context("Building response from dev server failed")
    }
    .await
    .into_internal_error_result()
}
