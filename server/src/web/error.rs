use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Map, Value};

use crate::{
    domain::qso::{MAX_FIELDS, MAX_VALUE_BYTES, QsoError},
    store::StoreError,
    web::references::LoadError,
};

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

    pub fn bad_request(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::BAD_REQUEST, message)
    }

    pub fn internal(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    pub fn with(mut self, key: &str, value: Value) -> ApiError {
        self.detail.insert(key.to_owned(), value);
        self
    }
}

pub(super) fn status(e: &StoreError) -> StatusCode {
    match e {
        StoreError::NotFound(_) => StatusCode::NOT_FOUND,
        StoreError::Unauthenticated => StatusCode::UNAUTHORIZED,
        StoreError::Forbidden(_) => StatusCode::FORBIDDEN,
        StoreError::Conflict(_) => StatusCode::CONFLICT,
        StoreError::Invalid(_) | StoreError::Qso(_) => StatusCode::BAD_REQUEST,
        StoreError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> ApiError {
        let error = ApiError::new(status(&e), e.to_string());
        match e {
            StoreError::NotFound(what) => error.with("code", "not_found".into()).with("what", what.into()),
            StoreError::Conflict(fault) | StoreError::Invalid(fault) => match fault.code {
                Some(code) => fault
                    .params
                    .into_iter()
                    .fold(error.with("code", code.into()), |error, (key, value)| {
                        error.with(key, value)
                    }),
                None => error,
            },
            StoreError::Qso(e) => qso(error, e),
            _ => error,
        }
    }
}

fn qso(error: ApiError, e: QsoError) -> ApiError {
    match e {
        QsoError::Missing(field) => error.with("code", "qso_missing".into()).with("field", field.into()),
        QsoError::Invalid { name, value } => error
            .with("code", "qso_invalid".into())
            .with("field", name.into())
            .with("value", value.into()),
        QsoError::FieldName(name) => error.with("code", "qso_field_name".into()).with("field", name.into()),
        QsoError::Repeated(name) => error.with("code", "field_repeated".into()).with("field", name.into()),
        QsoError::ColumnField(name) => error.with("code", "qso_column_field".into()).with("field", name.into()),
        QsoError::TooLong { name, bytes } => error
            .with("code", "qso_too_long".into())
            .with("field", name.into())
            .with("bytes", bytes.into())
            .with("max", MAX_VALUE_BYTES.into()),
        QsoError::TooManyFields(count) => error
            .with("code", "qso_too_many_fields".into())
            .with("count", count.into())
            .with("max", MAX_FIELDS.into()),
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
