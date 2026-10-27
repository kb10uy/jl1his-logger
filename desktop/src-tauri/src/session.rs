use std::{path::PathBuf, sync::Mutex};

use serde::Serialize;
use serde_json::Value;
use tauri::State;
use thiserror::Error;
use url::Url;

use crate::{
    api::{Api, ApiError, Logbook, Method},
    config::{Config, ConfigError, Server},
    credentials,
    origin::{OriginError, parse_origin},
};

pub struct Paths {
    pub config: PathBuf,
}

#[derive(Default)]
pub struct Connection(Mutex<Option<Api>>);

impl Connection {
    fn set(&self, api: Option<Api>) {
        *self.0.lock().expect("the connection lock is never poisoned") = api;
    }

    fn get(&self) -> Option<Api> {
        self.0.lock().expect("the connection lock is never poisoned").clone()
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Session {
    SignedOut {
        url: Option<String>,
        rejected: bool,
    },
    SignedIn {
        url: String,
        callsign: Option<String>,
        logbooks: Option<Vec<Logbook>>,
    },
}

#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum CommandError {
    #[error("{detail}")]
    BadUrl { detail: String },
    #[error("the server refused the token")]
    Unauthorized,
    #[error("the token is for the Wavelog compatible API")]
    Wl2Token,
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

fn origin_text(origin: &Url) -> String {
    origin.as_str().trim_end_matches('/').to_owned()
}

#[tauri::command]
pub async fn session(paths: State<'_, Paths>, connection: State<'_, Connection>) -> Result<Session, CommandError> {
    connection.set(None);
    let config = Config::load(&paths.config)?;
    let Some(Server { url }) = config.server else {
        return Ok(Session::SignedOut {
            url: None,
            rejected: false,
        });
    };
    let Some(token) = credentials::load(&url)? else {
        return Ok(Session::SignedOut {
            url: Some(url),
            rejected: false,
        });
    };
    let api = Api::new(parse_origin(&url)?, token);
    let session = match api.logbooks().await {
        Ok(logbooks) => Session::SignedIn {
            url,
            callsign: callsign(&api).await,
            logbooks: Some(logbooks),
        },
        Err(ApiError::Unauthorized) => {
            return Ok(Session::SignedOut {
                url: Some(url),
                rejected: true,
            });
        }
        Err(_) => Session::SignedIn {
            url,
            callsign: None,
            logbooks: None,
        },
    };
    connection.set(Some(api));
    Ok(session)
}

async fn callsign(api: &Api) -> Option<String> {
    api.me().await.ok().map(|me| me.callsign)
}

#[tauri::command]
pub async fn sign_in(
    paths: State<'_, Paths>,
    connection: State<'_, Connection>,
    url: String,
    token: String,
) -> Result<Session, CommandError> {
    let origin = parse_origin(&url)?;
    let url = origin_text(&origin);
    let token = token.trim().to_owned();
    let api = Api::new(origin, token.clone());
    let logbooks = api.logbooks().await.map_err(|e| match e {
        ApiError::Unauthorized if token.starts_with("wl2_") => CommandError::Wl2Token,
        e => e.into(),
    })?;
    if logbooks.is_empty() {
        return Err(CommandError::NoLogbook);
    }
    let mut config = Config::load(&paths.config)?;
    credentials::store(&url, &token)?;
    if let Some(previous) = config.server.as_ref().filter(|server| server.url != url) {
        credentials::forget(&previous.url)?;
    }
    config.server = Some(Server { url: url.clone() });
    config.save(&paths.config)?;
    let session = Session::SignedIn {
        url,
        callsign: callsign(&api).await,
        logbooks: Some(logbooks),
    };
    connection.set(Some(api));
    Ok(session)
}

#[tauri::command]
pub fn sign_out(paths: State<'_, Paths>, connection: State<'_, Connection>) -> Result<Session, CommandError> {
    connection.set(None);
    let url = Config::load(&paths.config)?.server.map(|server| server.url);
    if let Some(url) = &url {
        credentials::forget(url)?;
    }
    Ok(Session::SignedOut { url, rejected: false })
}

#[tauri::command]
pub async fn request(
    connection: State<'_, Connection>,
    method: Method,
    path: String,
    body: Option<Value>,
) -> Result<Value, CommandError> {
    let api = connection.get().ok_or(CommandError::Unauthorized)?;
    Ok(api.request(method, &path, body.as_ref()).await?)
}
