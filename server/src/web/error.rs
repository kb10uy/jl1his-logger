use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

use crate::store::StoreError;

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> ApiError {
        ApiError {
            status,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> ApiError {
        let status = match &e {
            StoreError::NotFound(_) => StatusCode::NOT_FOUND,
            StoreError::Unauthenticated => StatusCode::UNAUTHORIZED,
            StoreError::Forbidden(_) => StatusCode::FORBIDDEN,
            StoreError::Conflict(_) => StatusCode::CONFLICT,
            StoreError::Invalid(_) | StoreError::Qso(_) => StatusCode::BAD_REQUEST,
            StoreError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        ApiError::new(status, e.to_string())
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> ApiError {
        StoreError::from(e).into()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let message = match self.status {
            StatusCode::INTERNAL_SERVER_ERROR => {
                tracing::error!("{}", self.message);
                "internal server error".to_owned()
            }
            _ => self.message,
        };
        (self.status, Json(json!({ "status": "error", "error": message }))).into_response()
    }
}
