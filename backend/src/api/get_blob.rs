use super::ApiState;
use crate::jmap_account::AccountId;
use crate::repo::Blob;
use crate::util::http_error::{AnyhowHttpError, HttpResult};
use anyhow::Context;
use axum::body::Body;
use axum::extract;
use axum::http::{HeaderValue, header};
use axum::response::Response;
use serde::Deserialize;
use tracing::instrument;

#[derive(Deserialize)]
pub struct Params {
    pub name: Option<String>,
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,

    #[serde(default, rename = "blockImages")]
    pub block_images: bool,
}

#[instrument(skip(state))]
pub async fn get_blob(
    state: extract::State<ApiState>,
    extract::Path((account_id, blob_id)): extract::Path<(AccountId, String)>,
    extract::Query(Params {
        name,
        mime_type,
        block_images,
    }): extract::Query<Params>,
) -> HttpResult<Response> {
    let blob = match state
        .repo
        .get_blob(account_id, &blob_id)
        .await
        .context("Error querying blob")
        .into_internal_error_result()?
    {
        Some(blob) => blob,
        None => {
            tracing::info!("Fecthing blob from remote source");

            let api = state
                .account_states
                .read()
                .get(&account_id)
                .map(|s| s.jmap_api.clone())
                .context("Account not found")
                .into_not_found_error_result()?;

            let data = api
                .download_blob(&blob_id)
                .await
                .context("Error downloading blob")
                .into_internal_error_result()?;

            let blob = Blob {
                name,
                mime_type,
                data,
            };

            state
                .repo
                .save_blob(account_id, &blob_id, &blob)
                .await
                .context("Error saving downloaded blob")
                .into_internal_error_result()?;

            blob
        }
    };

    let mut response = Response::builder()
        .header(
            header::CONTENT_TYPE,
            blob.mime_type
                .as_ref()
                .map(|s| s.as_str())
                .unwrap_or("application/octet-stream"),
        )
        .header(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        )
        .header(
            header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
            HeaderValue::from_static("true"),
        )
        .header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        )
        .header(header::CONTENT_LENGTH, blob.data.len().to_string());

    if let Some(name) = &blob.name {
        response = response.header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{name}\""),
        );
    }

    if block_images {
        response = response.header(header::CONTENT_SECURITY_POLICY, "img-src 'none';");
    }

    response
        .body(Body::from(blob.data))
        .context("Error creating response from body")
        .into_internal_error_result()
}
