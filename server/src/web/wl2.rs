mod catalog;
mod qso;
mod station;

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{FromRequestParts, Query as AxumQuery},
    http::{StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
    routing::{any, get},
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::{
    store::{
        StoreError,
        access::{self, LogbookAccess},
        token::{self, TokenKind},
    },
    web::AppState,
};

const API: &str = "/wl2/index.php/api/v2";

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(&format!("{API}/station"), get(station::list))
        .route(&format!("{API}/qso"), get(qso::list))
        .route(&format!("{API}/catalog"), get(catalog::get))
        .route("/wl2", any(not_found))
        .route("/wl2/{*rest}", any(not_found))
}

#[derive(Debug)]
pub struct Wl2Error {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl Wl2Error {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Wl2Error {
        Wl2Error {
            status,
            code,
            message: message.into(),
        }
    }

    fn validation(message: impl Into<String>) -> Wl2Error {
        Wl2Error::new(StatusCode::BAD_REQUEST, "validation_error", message)
    }

    fn invalid_token(message: impl Into<String>) -> Wl2Error {
        Wl2Error::new(StatusCode::UNAUTHORIZED, "invalid_token", message)
    }
}

impl From<StoreError> for Wl2Error {
    fn from(e: StoreError) -> Wl2Error {
        let (status, code) = match &e {
            StoreError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            StoreError::Unauthenticated => (StatusCode::UNAUTHORIZED, "invalid_token"),
            StoreError::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
            StoreError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            StoreError::Invalid(_) | StoreError::Qso(_) => (StatusCode::BAD_REQUEST, "validation_error"),
            StoreError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };
        Wl2Error::new(status, code, e.to_string())
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
        (
            self.status,
            Json(json!({ "error": { "code": self.code, "message": message } })),
        )
            .into_response()
    }
}

pub struct Wl2Access(pub LogbookAccess);

impl FromRequestParts<Arc<AppState>> for Wl2Access {
    type Rejection = Wl2Error;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> Result<Wl2Access, Wl2Error> {
        let plaintext = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split_once(' '))
            .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
            .map(|(_, token)| token)
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

fn data(data: Value, meta: Value) -> Json<Value> {
    Json(json!({ "data": data, "meta": meta }))
}

async fn not_found() -> Wl2Error {
    Wl2Error::new(StatusCode::NOT_FOUND, "not_found", "not found")
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;

    use crate::{
        store::{WriteTx, membership, token::TokenKind},
        web::testing::{Auth, Harness},
    };

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
