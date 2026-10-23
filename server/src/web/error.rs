use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Map, Value};

use crate::{domain::qso::QsoError, store::StoreError, web::references::LoadError};

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
    detail: Map<String, Value>,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> ApiError {
        ApiError {
            status,
            message: message.into(),
            detail: Map::new(),
        }
    }

    pub fn internal(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    pub fn with(mut self, key: &str, value: Value) -> ApiError {
        self.detail.insert(key.to_owned(), value);
        self
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

impl From<QsoError> for ApiError {
    fn from(e: QsoError) -> ApiError {
        StoreError::from(e).into()
    }
}

impl From<LoadError> for ApiError {
    fn from(e: LoadError) -> ApiError {
        match e {
            LoadError::Store(e) => e.into(),
            LoadError::File(message) => ApiError::internal(message),
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> ApiError {
        StoreError::from(e).into()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut body = match self.status {
            StatusCode::INTERNAL_SERVER_ERROR => {
                tracing::error!("{}", self.message);
                Map::from_iter([("error".to_owned(), "internal server error".into())])
            }
            _ => {
                let mut body = self.detail;
                body.insert("error".to_owned(), self.message.into());
                body
            }
        };
        body.insert("status".to_owned(), "error".into());
        (self.status, Json(body)).into_response()
    }
}
