mod api;
mod error;
mod extract;
mod query;
mod references;
mod serve;
#[cfg(test)]
mod testing;
mod wl2;

use std::{path::Path, sync::Arc, time::Duration};

use axum::{
    Router,
    extract::{Request, State},
    http::{HeaderName, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use tokio::sync::{Semaphore, watch};
use tower_http::{
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeader,
    timeout::RequestBodyTimeoutLayer,
    trace::TraceLayer,
};
use url::Url;
use webauthn_rs::{Webauthn, WebauthnBuilder, prelude::WebauthnError};

use self::{api::auth::Ceremonies, error::ApiError, references::References};
use crate::{config::Config, db::Db};

pub use self::serve::serve;

pub const MAX_STREAMS: usize = 64;
const BODY_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

const SECURITY_HEADERS: [(HeaderName, &str); 5] = [
    (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    (header::X_FRAME_OPTIONS, "DENY"),
    (header::REFERRER_POLICY, "same-origin"),
    (
        header::CONTENT_SECURITY_POLICY,
        "frame-ancestors 'none'; base-uri 'self'; form-action 'self'; object-src 'none'",
    ),
    (
        HeaderName::from_static("permissions-policy"),
        "camera=(), geolocation=(), payment=()",
    ),
];

pub struct AppState {
    pub db: Db,
    pub config: Config,
    origin: String,
    webauthn: Webauthn,
    ceremonies: Ceremonies,
    references: References,
    streams: Arc<Semaphore>,
    writes: watch::Sender<u64>,
    closing: watch::Sender<bool>,
}

impl AppState {
    pub fn new(db: Db, config: Config, public_origin: Url) -> Result<AppState, WebauthnError> {
        let rp_id = public_origin.host_str().ok_or(WebauthnError::Configuration)?;
        let webauthn = WebauthnBuilder::new(rp_id, &public_origin)?
            .rp_name("JL1HIS Logger")
            .build()?;
        Ok(AppState {
            db,
            config,
            origin: public_origin.origin().ascii_serialization(),
            webauthn,
            ceremonies: Ceremonies::default(),
            references: References::default(),
            streams: Arc::new(Semaphore::new(MAX_STREAMS)),
            writes: watch::Sender::new(0),
            closing: watch::Sender::new(false),
        })
    }

    pub fn close_streams(&self) {
        self.closing.send_replace(true);
    }
}

pub fn router(state: Arc<AppState>, web_dir: Option<&Path>) -> Router {
    let api = api::router()
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
    app.layer(middleware::from_fn(security_headers))
        .layer(RequestBodyTimeoutLayer::new(BODY_IDLE_TIMEOUT))
        .layer(TraceLayer::new_for_http())
}

fn under(path: &str, prefix: &str) -> bool {
    path == prefix || path.strip_prefix(prefix).is_some_and(|rest| rest.starts_with('/'))
}

fn is_api(path: &str) -> bool {
    under(path, "/api") || under(path, "/wl2")
}

async fn security_headers(request: Request, next: Next) -> Response {
    let api = is_api(request.uri().path());
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    for (name, value) in SECURITY_HEADERS {
        headers.insert(name, HeaderValue::from_static(value));
    }
    if api {
        headers
            .entry(header::CACHE_CONTROL)
            .or_insert(HeaderValue::from_static("no-store"));
    }
    response
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
    let path = request.uri().path();
    let write = !matches!(*request.method(), Method::GET | Method::HEAD | Method::OPTIONS) && !under(path, "/api/auth");
    let response = next.run(request).await;
    if write && response.status().is_success() {
        state.writes.send_modify(|n| *n = n.wrapping_add(1));
    }
    response
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::HeaderMap};
    use http_body_util::BodyExt;
    use rstest::rstest;
    use serde_json::json;
    use tempfile::TempDir;
    use tower::ServiceExt;

    use super::*;
    use crate::{
        db,
        store::{
            WriteTx,
            account::token::TokenKind,
            reference::{self, ReferenceKind},
        },
        web::testing::{Auth, Harness, authorize},
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

    async fn get(router: Router, uri: &str) -> (StatusCode, HeaderMap, String) {
        let response = router
            .oneshot(axum::http::Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, headers, String::from_utf8(body.to_vec()).unwrap())
    }

    fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
        headers.get(name).map(|v| v.to_str().unwrap())
    }

    #[tokio::test]
    async fn reports_health() {
        let (_dir, router) = app(false).await;
        let (status, _, body) = get(router, "/api/health").await;
        assert_eq!((status, body.as_str()), (StatusCode::OK, r#"{"status":"ok"}"#));
    }

    #[tokio::test]
    async fn reports_the_version_to_signed_in_users() {
        let h = Harness::new().await;
        let session = h.session(&h.world.operator).await;
        let (status, body) = h.send("GET", "/api/version", Auth::Session(&session), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(body["commit"], json!(option_env!("JL1HIS_COMMIT")));
        assert_eq!(body["adif"], "3.1.8");
        let (status, _) = h.send("GET", "/api/version", Auth::Anonymous, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[rstest]
    #[case::api("/api/health", Some("no-store"))]
    #[case::api_not_found("/api/nothing", Some("no-store"))]
    #[case::wl2("/wl2/index.php/api/v2/station", Some("no-store"))]
    #[case::page("/", Some("no-cache"))]
    #[case::page_under_api_like_path("/apiary", Some("no-cache"))]
    #[case::immutable("/_app/immutable/app.js", Some("public, max-age=31536000, immutable"))]
    #[tokio::test]
    async fn sets_security_headers_on_every_response(#[case] uri: &str, #[case] cache: Option<&str>) {
        let (_dir, router) = app(true).await;
        let (_, headers, _) = get(router, uri).await;
        assert_eq!(header(&headers, "x-content-type-options"), Some("nosniff"));
        assert_eq!(header(&headers, "x-frame-options"), Some("DENY"));
        assert_eq!(header(&headers, "referrer-policy"), Some("same-origin"));
        assert_eq!(
            header(&headers, "content-security-policy"),
            Some("frame-ancestors 'none'; base-uri 'self'; form-action 'self'; object-src 'none'")
        );
        assert_eq!(
            header(&headers, "permissions-policy"),
            Some("camera=(), geolocation=(), payment=()")
        );
        assert_eq!(header(&headers, "cache-control"), cache);
    }

    #[tokio::test]
    async fn keeps_the_cache_control_of_the_event_stream() {
        let h = Harness::new().await;
        let session = h.session(&h.world.operator).await;
        let request = authorize(axum::http::Request::get(h.path("/events")), Auth::Session(&session));
        let response = h
            .app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(header(response.headers(), "cache-control"), Some("no-cache"));
    }

    #[tokio::test]
    async fn wakes_streams_for_writes_outside_sign_in() {
        let h = Harness::new().await;
        let writes = h.state.writes.subscribe();
        let session = h.session(&h.world.operator).await;

        let (status, _) = h.send("POST", "/api/auth/logout", Auth::Session(&session), None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let session = h.session(&h.world.operator).await;
        let (status, _) = h.send("POST", &h.path("/qsos"), Auth::Anonymous, Some(json!({}))).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(!writes.has_changed().unwrap());

        h.create_qso(&session, &h.station.id, "JA1XYZ", "1200", &[]).await;
        assert!(writes.has_changed().unwrap());
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
        let (status, headers, body) = get(router, uri).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(header(&headers, "cache-control"), Some(cache));
        assert_eq!(body, content);
    }
}
