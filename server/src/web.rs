use std::{path::Path, sync::Arc};

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{any, get},
};
use serde_json::json;
use tower_http::{
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeader,
    trace::TraceLayer,
};
use url::Url;

use crate::{config::Config, db::Db};

pub struct AppState {
    pub db: Db,
    pub config: Config,
    pub public_origin: Url,
}

pub fn router(state: Arc<AppState>, web_dir: Option<&Path>) -> Router {
    let api = Router::new()
        .route("/api/health", get(health))
        .route("/api", any(api_not_found))
        .route("/api/{*rest}", any(api_not_found))
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
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "status": "error", "error": "not found" })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use rstest::rstest;
    use tempfile::TempDir;
    use tower::ServiceExt;

    use super::*;
    use crate::db;

    async fn app(web: bool) -> (TempDir, Router) {
        let dir = tempfile::tempdir().unwrap();
        let db = db::open(&dir.path().join("test.db")).await.unwrap();
        let web_dir = dir.path().join("build");
        std::fs::create_dir_all(web_dir.join("_app").join("immutable")).unwrap();
        std::fs::write(web_dir.join("index.html"), "<!doctype html>").unwrap();
        std::fs::write(web_dir.join("_app").join("immutable").join("app.js"), "app").unwrap();
        let state = Arc::new(AppState {
            db,
            config: Config::default(),
            public_origin: Url::parse("http://localhost:8080").unwrap(),
        });
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
    #[case::without_web_dir(false)]
    #[case::with_web_dir(true)]
    #[tokio::test]
    async fn unknown_api_paths_are_json_404s(#[case] web: bool) {
        for uri in ["/api", "/api/nothing", "/api/nested/nothing"] {
            let (_dir, router) = app(web).await;
            let (status, _, body) = get(router, uri).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
            assert!(body.contains("not found"), "{uri}: {body}");
        }
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
