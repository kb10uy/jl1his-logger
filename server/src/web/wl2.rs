mod catalog;
mod lookup;
mod qso;
mod station;

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{FromRequest, FromRequestParts, Query as AxumQuery, Request},
    http::{StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
    routing::{any, get},
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sqlx::SqliteConnection;

use crate::{
    domain::qso::QsoError,
    store::{
        StoreError,
        access::{self, LogbookAccess},
        account::token::{self, TokenKind},
        logbook::station::{self as stations, Station},
    },
    web::{AppState, api::auth::bearer, error, references::LoadError},
};

const API: &str = "/wl2/index.php/api/v2";

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(&format!("{API}/station"), get(station::list))
        .route(&format!("{API}/qso"), get(qso::list).post(qso::create))
        .route(&format!("{API}/catalog"), get(catalog::get))
        .route(&format!("{API}/lookup"), get(lookup::get))
        .route("/wl2/api/v2/lookup", get(lookup::get))
        .route("/wl2", any(not_found))
        .route("/wl2/{*rest}", any(not_found))
}

#[derive(Debug)]
pub struct Wl2Error {
    status: StatusCode,
    code: &'static str,
    message: String,
    details: Option<Value>,
}

impl Wl2Error {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Wl2Error {
        Wl2Error {
            status,
            code,
            message: message.into(),
            details: None,
        }
    }

    fn with_details(mut self, details: Value) -> Wl2Error {
        self.details = Some(details);
        self
    }

    fn validation(message: impl Into<String>) -> Wl2Error {
        Wl2Error::new(StatusCode::BAD_REQUEST, "validation_error", message)
    }

    fn invalid_token(message: impl Into<String>) -> Wl2Error {
        Wl2Error::new(StatusCode::UNAUTHORIZED, "invalid_token", message)
    }

    fn forbidden(message: impl Into<String>) -> Wl2Error {
        Wl2Error::new(StatusCode::FORBIDDEN, "forbidden", message)
    }

    fn not_found(message: impl Into<String>) -> Wl2Error {
        Wl2Error::new(StatusCode::NOT_FOUND, "not_found", message)
    }
}

impl From<StoreError> for Wl2Error {
    fn from(e: StoreError) -> Wl2Error {
        let status = error::status(&e);
        let code = match status {
            StatusCode::NOT_FOUND => "not_found",
            StatusCode::UNAUTHORIZED => "invalid_token",
            StatusCode::FORBIDDEN => "forbidden",
            StatusCode::CONFLICT => "conflict",
            StatusCode::BAD_REQUEST => "validation_error",
            _ => "internal_error",
        };
        Wl2Error::new(status, code, e.to_string())
    }
}

impl From<QsoError> for Wl2Error {
    fn from(e: QsoError) -> Wl2Error {
        StoreError::from(e).into()
    }
}

impl From<LoadError> for Wl2Error {
    fn from(e: LoadError) -> Wl2Error {
        match e {
            LoadError::Store(e) => e.into(),
            LoadError::File(message) => Wl2Error::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message),
        }
    }
}

impl From<sqlx::Error> for Wl2Error {
    fn from(e: sqlx::Error) -> Wl2Error {
        StoreError::from(e).into()
    }
}

impl IntoResponse for Wl2Error {
    fn into_response(self) -> Response {
        let message = match self.status {
            StatusCode::INTERNAL_SERVER_ERROR => {
                tracing::error!("{}", self.message);
                "internal server error".to_owned()
            }
            _ => self.message,
        };
        let mut error = json!({ "code": self.code, "message": message });
        if let Some(details) = self.details {
            error["details"] = details;
        }
        (self.status, Json(json!({ "error": error }))).into_response()
    }
}

pub struct Wl2Access(pub LogbookAccess);

impl FromRequestParts<Arc<AppState>> for Wl2Access {
    type Rejection = Wl2Error;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> Result<Wl2Access, Wl2Error> {
        let plaintext = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(bearer)
            .ok_or_else(|| Wl2Error::invalid_token("a Bearer API token is required"))?;
        let (principal, token) = token::authenticate(&state.db, plaintext, TokenKind::Wl2)
            .await
            .map_err(|e| match e {
                StoreError::Unauthenticated => Wl2Error::invalid_token("the API token is not valid"),
                e => e.into(),
            })?;
        let [logbook_id] = token.logbook_ids.as_slice() else {
            return Err(Wl2Error::invalid_token("the API token is not fixed to one Logbook"));
        };
        let mut conn = state.db.acquire().await?;
        match access::logbook(&mut conn, &principal, logbook_id).await {
            Ok(access) => Ok(Wl2Access(access)),
            Err(StoreError::NotFound(_)) => {
                Err(Wl2Error::invalid_token("the Logbook of the API token is not available"))
            }
            Err(e) => Err(e.into()),
        }
    }
}

pub struct Wl2Query<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequestParts<S> for Wl2Query<T> {
    type Rejection = Wl2Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Wl2Query<T>, Wl2Error> {
        match AxumQuery::<T>::from_request_parts(parts, state).await {
            Ok(AxumQuery(value)) => Ok(Wl2Query(value)),
            Err(e) => Err(Wl2Error::validation(e.body_text())),
        }
    }
}

pub struct Wl2Json<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for Wl2Json<T> {
    type Rejection = Wl2Error;

    async fn from_request(req: Request, state: &S) -> Result<Wl2Json<T>, Wl2Error> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(Wl2Json(value)),
            Err(e) => Err(Wl2Error::validation(e.body_text())),
        }
    }
}

fn present(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

fn mapped<'a>(stations: &'a [Station], wavelog_id: &str) -> Result<&'a Station, Wl2Error> {
    let wavelog_id = wavelog_id.trim();
    stations
        .iter()
        .find(|s| s.wavelog_id.is_some_and(|id| id.to_string() == wavelog_id))
        .ok_or_else(|| Wl2Error::forbidden(format!("Station {wavelog_id:?} is not a Station of this Logbook")))
}

async fn station_ids(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    wavelog_ids: Option<&str>,
) -> Result<Option<Vec<String>>, Wl2Error> {
    let Some(list) = wavelog_ids else {
        return Ok(None);
    };
    let stations = stations::list(conn, access).await?;
    list.split(',')
        .map(|wavelog_id| Ok(mapped(&stations, wavelog_id)?.id.clone()))
        .collect::<Result<_, _>>()
        .map(Some)
}

fn data(data: Value, meta: Value) -> Json<Value> {
    Json(json!({ "data": data, "meta": meta }))
}

async fn not_found() -> Wl2Error {
    Wl2Error::not_found("not found")
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use tempfile::TempDir;

    use crate::{
        store::{WriteTx, account::token::TokenKind, logbook::membership, reference::ReferenceKind},
        web::testing::{Auth, Harness},
    };

    pub(super) struct Fixture {
        pub(super) h: Harness,
        token: String,
        _dir: TempDir,
    }

    impl Fixture {
        pub(super) async fn new() -> Fixture {
            let (h, _dir) = Harness::with_references().await;
            let token = h.token(&h.world.operator, TokenKind::Wl2, &[&h.world.logbook.id]).await;
            Fixture { h, token, _dir }
        }

        pub(super) async fn install(&self, kind: ReferenceKind, text: &str, version: &str) {
            self.h.install_reference(kind, text, version).await;
        }

        pub(super) async fn get(&self, uri: &str) -> (StatusCode, Value) {
            self.h.send("GET", uri, Auth::Bearer(&self.token), None).await
        }

        pub(super) async fn post(&self, uri: &str, body: Value) -> (StatusCode, Value) {
            self.h.send("POST", uri, Auth::Bearer(&self.token), Some(body)).await
        }
    }

    #[tokio::test]
    async fn accepts_only_wl2_tokens_of_a_member() {
        let h = Harness::new().await;
        let logbook = h.world.logbook.id.as_str();
        let uri = "/wl2/index.php/api/v2/station";
        let wl2 = h.token(&h.world.operator, TokenKind::Wl2, &[logbook]).await;
        let native = h.token(&h.world.operator, TokenKind::Native, &[logbook]).await;
        let session = h.session(&h.world.operator).await;

        let (status, _) = h.send("GET", uri, Auth::Bearer(&wl2), None).await;
        assert_eq!(status, StatusCode::OK);
        for auth in [Auth::Anonymous, Auth::Bearer(&native), Auth::Session(&session)] {
            let (status, body) = h.send("GET", uri, auth, None).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED);
            assert_eq!(body["error"]["code"], "invalid_token", "{body}");
        }

        let owner = h.world.access(&h.world.owner).await;
        let mut tx = WriteTx::begin(&h.world.db).await.unwrap();
        membership::remove(&mut tx, &owner.owner().unwrap(), &h.world.operator.id)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let (status, body) = h.send("GET", uri, Auth::Bearer(&wl2), None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "invalid_token", "{body}");
    }

    #[tokio::test]
    async fn answers_unknown_paths_in_the_wavelog_shape() {
        let h = Harness::new().await;
        for uri in ["/wl2", "/wl2/index.php/api/v2/nothing", "/wl2/api/v2/station"] {
            let (status, body) = h.send("GET", uri, Auth::Anonymous, None).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
            assert_eq!(
                body,
                json!({ "error": { "code": "not_found", "message": "not found" } }),
                "{uri}"
            );
        }
    }
}
