use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State};
use url::Url;

use crate::{
    config::{Config, Paths, Server},
    error::CommandError,
    listen::Listeners,
    server::{Api, ApiError, Logbook, Method, credentials, events::Watchers, origin::parse_origin},
};

#[derive(Default)]
pub struct Connection(Mutex<Option<Api>>);

impl Connection {
    fn set(&self, api: Option<Api>) {
        *self.0.lock().expect("the connection lock is never poisoned") = api;
    }

    pub fn get(&self) -> Option<Api> {
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
    let logbooks = api.logbooks().await?;
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
pub fn sign_out(
    app: AppHandle,
    paths: State<'_, Paths>,
    connection: State<'_, Connection>,
    watchers: State<'_, Watchers>,
    listening: State<'_, Listeners>,
) -> Result<Session, CommandError> {
    connection.set(None);
    watchers.stop();
    listening.forget_form(&app);
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
