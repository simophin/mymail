use axum::http::StatusCode;
use axum::response::{ErrorResponse, IntoResponse};

pub type HttpResult<T> = Result<T, ErrorResponse>;

pub trait AnyhowHttpError<T>: Sized {
    fn into_error_result(self, error_status_code: StatusCode) -> HttpResult<T>;

    fn into_internal_error_result(self) -> HttpResult<T> {
        self.into_error_result(StatusCode::INTERNAL_SERVER_ERROR)
    }

    fn into_not_found_error_result(self) -> HttpResult<T> {
        self.into_error_result(StatusCode::NOT_FOUND)
    }
}

impl<T> AnyhowHttpError<T> for anyhow::Result<T> {
    fn into_error_result(self, error_status_code: StatusCode) -> HttpResult<T> {
        self.map_err(|err| (error_status_code, format!("{err:?}")).into())
    }
}
