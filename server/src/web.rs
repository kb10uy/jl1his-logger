mod auth;
mod error;
mod events;
mod export;
mod extract;
mod import;
mod logbook;
mod lookup;
mod member;
mod qso;
mod query;
mod references;
mod station;
#[cfg(test)]
mod testing;
mod token;
mod wl2;

use std::{path::Path, sync::Arc};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Request, State},
    http::{HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, delete, get, post, put},
};
use serde_json::json;
use tokio::sync::watch;
use tower_http::{
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeader,
    trace::TraceLayer,
};
use url::Url;
use webauthn_rs::{Webauthn, WebauthnBuilder, prelude::WebauthnError};

use self::{auth::Ceremonies, error::ApiError, references::References};
use crate::{config::Config, db::Db};

pub struct AppState {
    pub db: Db,
    pub config: Config,
    origin: String,
    webauthn: Webauthn,
    ceremonies: Ceremonies,
    references: References,
    writes: watch::Sender<u64>,
    closing: watch::Sender<bool>,
}

impl AppState {
    pub fn new(db: Db, config: Config, public_origin: Url) -> Result<AppState, WebauthnError> {
        let rp_id = public_origin.host_str().ok_or(WebauthnError::Configuration)?;
        let webauthn = WebauthnBuilder::new(rp_id, &public_origin)?
            .rp_name("jl1his-logger")
            .build()?;
        Ok(AppState {
            db,
            config,
            origin: public_origin.origin().ascii_serialization(),
            webauthn,
            ceremonies: Ceremonies::default(),
            references: References::default(),
            writes: watch::Sender::new(0),
            closing: watch::Sender::new(false),
        })
    }

    pub fn close_streams(&self) {
        self.closing.send_replace(true);
    }
}

pub fn router(state: Arc<AppState>, web_dir: Option<&Path>) -> Router {
    let api = Router::new()
        .route("/api/health", get(health))
        .route("/api/me", get(auth::me))
        .route("/api/auth/login/start", post(auth::login_start))
        .route("/api/auth/login/finish", post(auth::login_finish))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/register/start", post(auth::link_start))
        .route("/api/auth/register/finish", post(auth::link_finish))
        .route("/api/passkeys", get(auth::list_passkeys))
        .route("/api/passkeys/register/start", post(auth::add_start))
        .route("/api/passkeys/register/finish", post(auth::add_finish))
        .route("/api/passkeys/{id}", delete(auth::delete_passkey))
        .route("/api/reference", get(references::list))
        .route("/api/tokens", get(token::list).post(token::issue))
        .route("/api/tokens/{id}", delete(token::revoke))
        .route("/api/logbooks", get(logbook::list))
        .route("/api/logbooks/{logbook_id}", get(logbook::get))
        .route("/api/logbooks/{logbook_id}/members", get(logbook::members))
        .route(
            "/api/logbooks/{logbook_id}/members/{callsign}",
            put(logbook::set_member).delete(logbook::remove_member),
        )
        .route(
            "/api/logbooks/{logbook_id}/stations",
            get(station::list).post(station::create),
        )
        .route(
            "/api/logbooks/{logbook_id}/stations/{id}",
            get(station::get).put(station::update),
        )
        .route(
            "/api/logbooks/{logbook_id}/stations/{id}/archive",
            post(station::archive).delete(station::unarchive),
        )
        .route(
            "/api/logbooks/{logbook_id}/stations/{id}/rewrite",
            get(station::preview_rewrite).post(station::rewrite),
        )
        .route("/api/logbooks/{logbook_id}/qsl", post(export::update_qsl))
        .route(
            "/api/logbooks/{logbook_id}/imports",
            post(import::import).layer(DefaultBodyLimit::max(import::MAX_FILE)),
        )
        .route(
            "/api/logbooks/{logbook_id}/exports",
            get(export::list).post(export::create),
        )
        .route("/api/logbooks/{logbook_id}/exports/{id}", get(export::get))
        .route("/api/logbooks/{logbook_id}/exports/{id}/apply", post(export::apply))
        .route("/api/logbooks/{logbook_id}/changes", get(qso::changes))
        .route("/api/logbooks/{logbook_id}/events", get(events::stream))
        .route("/api/logbooks/{logbook_id}/duplicates", get(qso::duplicates))
        .route("/api/logbooks/{logbook_id}/lookup", get(lookup::get))
        .route("/api/logbooks/{logbook_id}/qsos", get(qso::search).post(qso::create))
        .route(
            "/api/logbooks/{logbook_id}/qsos/{id}",
            get(qso::get).put(qso::update).delete(qso::delete),
        )
        .route("/api", any(api_not_found))
        .route("/api/{*rest}", any(api_not_found))
        .merge(wl2::router())
        .layer(middleware::from_fn_with_state(state.clone(), announce_writes))
        .layer(middleware::from_fn_with_state(state.clone(), same_origin_only))
        .with_state(state);

    let app = match web_dir {
        Some(dir) => {
            let immutable = SetResponseHeader::overriding(
                ServeDir::new(dir.join("_app").join("immutable")),
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=31536000, immutable"),
            );
            let files = SetResponseHeader::overriding(
                ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html"))),
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-cache"),
            );
            api.nest_service("/_app/immutable", immutable).fallback_service(files)
        }
        None => api,
    };
    app.layer(TraceLayer::new_for_http())
}

async fn same_origin_only(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    if !matches!(*request.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        let headers = request.headers();
        let foreign_origin = headers
            .get(header::ORIGIN)
            .is_some_and(|o| o.as_bytes() != state.origin.as_bytes());
        let cross_site = headers
            .get("sec-fetch-site")
            .is_some_and(|s| s != "same-origin" && s != "none");
        if foreign_origin || cross_site {
            return ApiError::new(StatusCode::FORBIDDEN, "cross-origin request").into_response();
        }
    }
    next.run(request).await
}

async fn announce_writes(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    let write = !matches!(*request.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    let response = next.run(request).await;
    if write && response.status().is_success() {
        state.writes.send_modify(|n| *n = n.wrapping_add(1));
    }
    response
}

async fn health(State(state): State<Arc<AppState>>) -> Response {
    match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => Json(json!({ "status": "ok" })).into_response(),
        Err(e) => {
            tracing::error!("health check failed: {e}");
            (StatusCode::SERVICE_UNAVAILABLE, Json(json!({ "status": "error" }))).into_response()
        }
    }
}

async fn api_not_found() -> Response {
    ApiError::new(StatusCode::NOT_FOUND, "not found").into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use rstest::rstest;
    use tempfile::TempDir;
    use tower::ServiceExt;

    use super::*;
    use crate::{
        db,
        store::{
            WriteTx,
            reference::{self, ReferenceKind},
            token::TokenKind,
        },
        web::testing::{Auth, Harness},
    };

    async fn app(web: bool) -> (TempDir, Router) {
        let dir = tempfile::tempdir().unwrap();
        let db = db::open(&dir.path().join("test.db")).await.unwrap();
        let web_dir = dir.path().join("build");
        std::fs::create_dir_all(web_dir.join("_app").join("immutable")).unwrap();
        std::fs::write(web_dir.join("index.html"), "<!doctype html>").unwrap();
        std::fs::write(web_dir.join("_app").join("immutable").join("app.js"), "app").unwrap();
        let state =
            Arc::new(AppState::new(db, Config::default(), Url::parse("http://localhost:8080").unwrap()).unwrap());
        let router = router(state, web.then_some(web_dir.as_path()));
        (dir, router)
    }

    async fn get(router: Router, uri: &str) -> (StatusCode, Option<String>, String) {
        let response = router
            .oneshot(axum::http::Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let cache = response
            .headers()
            .get(header::CACHE_CONTROL)
            .map(|v| v.to_str().unwrap().to_owned());
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, cache, String::from_utf8(body.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn reports_health() {
        let (_dir, router) = app(false).await;
        let (status, _, body) = get(router, "/api/health").await;
        assert_eq!((status, body.as_str()), (StatusCode::OK, r#"{"status":"ok"}"#));
    }

    #[rstest]
    #[tokio::test]
    async fn unknown_api_paths_are_json_404s(
        #[values(false, true)] web: bool,
        #[values("/api", "/api/nothing", "/api/nested/nothing")] uri: &str,
    ) {
        let (_dir, router) = app(web).await;
        let (status, _, body) = get(router, uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.contains("not found"), "{body}");
    }

    #[rstest]
    #[case::foreign_origin(&[("origin", "https://evil.example.com")], StatusCode::FORBIDDEN)]
    #[case::null_origin(&[("origin", "null")], StatusCode::FORBIDDEN)]
    #[case::cross_site(&[("sec-fetch-site", "cross-site")], StatusCode::FORBIDDEN)]
    #[case::same_site(&[("origin", "http://localhost:8080"), ("sec-fetch-site", "same-site")], StatusCode::FORBIDDEN)]
    #[case::same_origin(&[("origin", "http://localhost:8080"), ("sec-fetch-site", "same-origin")], StatusCode::NO_CONTENT)]
    #[case::not_a_browser(&[], StatusCode::NO_CONTENT)]
    #[tokio::test]
    async fn accepts_writes_only_from_the_public_origin(#[case] headers: &[(&str, &str)], #[case] status: StatusCode) {
        let (_dir, router) = app(false).await;
        let mut request = axum::http::Request::post("/api/auth/logout");
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = router.oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), status);
    }

    #[tokio::test]
    async fn shows_reference_versions_to_signed_in_users() {
        let h = Harness::new().await;
        let admin = h.world.admin_access().await;
        let mut tx = WriteTx::begin(&h.world.db).await.unwrap();
        let cty = reference::set(&mut tx, &admin, ReferenceKind::Cty, "2026-10-15", Some(&[1]))
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let session = h.session(&h.world.outsider).await;
        let (status, body) = h.send("GET", "/api/reference", Auth::Session(&session), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            json!([{ "kind": "cty", "version": "2026-10-15", "updated_at": cty.updated_at }])
        );

        let token = h
            .token(&h.world.operator, TokenKind::Native, &[&h.world.logbook.id])
            .await;
        for auth in [Auth::Anonymous, Auth::Bearer(&token)] {
            let (status, _) = h.send("GET", "/api/reference", auth, None).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED);
        }
    }

    #[tokio::test]
    async fn reads_from_other_origins() {
        let (_dir, router) = app(false).await;
        let request = axum::http::Request::get("/api/health")
            .header(header::ORIGIN, "https://evil.example.com")
            .header("sec-fetch-site", "cross-site");
        let response = router.oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[rstest]
    #[case("/", "no-cache", "<!doctype html>")]
    #[case("/log/2026", "no-cache", "<!doctype html>")]
    #[case("/_app/immutable/app.js", "public, max-age=31536000, immutable", "app")]
    #[tokio::test]
    async fn serves_the_client(#[case] uri: &str, #[case] cache: &str, #[case] content: &str) {
        let (_dir, router) = app(true).await;
        let (status, cache_control, body) = get(router, uri).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(cache_control.as_deref(), Some(cache));
        assert_eq!(body, content);
    }
}
