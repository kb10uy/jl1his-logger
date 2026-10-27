use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

use crate::{
    config::ConfigError,
    server::{ApiError, origin::OriginError},
};

#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum CommandError {
    #[error("{detail}")]
    BadUrl { detail: String },
    #[error("the server refused the token")]
    Unauthorized,
    #[error("the token reaches no Logbook")]
    NoLogbook,
    #[error("{detail}")]
    Unreachable { detail: String },
    #[error("the server answered {status}: {detail}")]
    Server { status: u16, detail: String, body: Value },
    #[error("{detail}")]
    Credentials { detail: String },
    #[error("{detail}")]
    Config { detail: String },
    #[error("{detail}")]
    Unexpected { detail: String },
}

impl From<OriginError> for CommandError {
    fn from(e: OriginError) -> CommandError {
        CommandError::BadUrl { detail: e.to_string() }
    }
}

impl From<ApiError> for CommandError {
    fn from(e: ApiError) -> CommandError {
        match e {
            ApiError::Unauthorized => CommandError::Unauthorized,
            ApiError::Unreachable(detail) => CommandError::Unreachable { detail },
            ApiError::Server { status, message, body } => CommandError::Server {
                status,
                detail: message,
                body,
            },
            e @ ApiError::NotApiPath(_) => CommandError::Unexpected { detail: e.to_string() },
        }
    }
}

impl From<keyring::Error> for CommandError {
    fn from(e: keyring::Error) -> CommandError {
        CommandError::Credentials { detail: e.to_string() }
    }
}

impl From<ConfigError> for CommandError {
    fn from(e: ConfigError) -> CommandError {
        CommandError::Config { detail: e.to_string() }
    }
}
