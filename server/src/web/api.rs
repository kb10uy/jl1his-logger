pub(super) mod auth;
mod logbook;
pub(super) mod qso;

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{any, delete, get, post, put},
};
use serde_json::json;

use super::{AppState, error::ApiError, references};

pub(super) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/version", get(version))
        .route("/api/me", get(auth::me))
        .route("/api/auth/login/start", post(auth::passkey::login_start))
        .route("/api/auth/login/finish", post(auth::passkey::login_finish))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/register/start", post(auth::passkey::link_start))
        .route("/api/auth/register/finish", post(auth::passkey::link_finish))
        .route("/api/passkeys", get(auth::passkey::list_passkeys))
        .route("/api/passkeys/register/start", post(auth::passkey::add_start))
        .route("/api/passkeys/register/finish", post(auth::passkey::add_finish))
        .route("/api/passkeys/{id}", delete(auth::passkey::delete_passkey))
        .route("/api/sessions", get(auth::session::list))
        .route("/api/sessions/{id}", delete(auth::session::revoke))
        .route("/api/reference", get(references::list))
        .route("/api/reference/subdivisions/{dxcc}", get(references::subdivisions))
        .route("/api/tokens", get(auth::token::list).post(auth::token::issue))
        .route("/api/tokens/{id}", delete(auth::token::revoke))
        .route("/api/logbooks", get(logbook::list))
        .route("/api/logbooks/{logbook_id}", get(logbook::get))
        .route("/api/logbooks/{logbook_id}/settings", put(logbook::set_settings))
        .route("/api/logbooks/{logbook_id}/members", get(logbook::members))
        .route(
            "/api/logbooks/{logbook_id}/members/{callsign}",
            put(logbook::set_member).delete(logbook::remove_member),
        )
        .route(
            "/api/logbooks/{logbook_id}/stations",
            get(logbook::station::list).post(logbook::station::create),
        )
        .route(
            "/api/logbooks/{logbook_id}/stations/{id}",
            get(logbook::station::get).put(logbook::station::update),
        )
        .route(
            "/api/logbooks/{logbook_id}/stations/{id}/archive",
            post(logbook::station::archive).delete(logbook::station::unarchive),
        )
        .route(
            "/api/logbooks/{logbook_id}/stations/{id}/rewrite",
            get(logbook::station::preview_rewrite).post(logbook::station::rewrite),
        )
        .route(
            "/api/logbooks/{logbook_id}/instruments",
            get(logbook::instrument::list).post(logbook::instrument::create),
        )
        .route(
            "/api/logbooks/{logbook_id}/instruments/{id}",
            get(logbook::instrument::get).put(logbook::instrument::update),
        )
        .route(
            "/api/logbooks/{logbook_id}/instruments/{id}/archive",
            post(logbook::instrument::archive).delete(logbook::instrument::unarchive),
        )
        .route(
            "/api/logbooks/{logbook_id}/instruments/{id}/rewrite",
            get(logbook::instrument::preview_rewrite).post(logbook::instrument::rewrite),
        )
        .route("/api/logbooks/{logbook_id}/qsl", post(qso::export::update_qsl))
        .route(
            "/api/logbooks/{logbook_id}/imports",
            post(qso::import::import).layer(DefaultBodyLimit::max(qso::import::MAX_FILE)),
        )
        .route(
            "/api/logbooks/{logbook_id}/exports",
            get(qso::export::list).post(qso::export::create),
        )
        .route("/api/logbooks/{logbook_id}/exports/{id}", get(qso::export::get))
        .route(
            "/api/logbooks/{logbook_id}/exports/{id}/apply",
            post(qso::export::apply),
        )
        .route("/api/logbooks/{logbook_id}/changes", get(qso::changes))
        .route("/api/logbooks/{logbook_id}/events", get(qso::events::stream))
        .route("/api/logbooks/{logbook_id}/duplicates", get(qso::duplicates))
        .route("/api/logbooks/{logbook_id}/lookup", get(qso::lookup::get))
        .route("/api/logbooks/{logbook_id}/qsos", get(qso::search).post(qso::create))
        .route("/api/logbooks/{logbook_id}/targets", get(qso::targets))
        .route(
            "/api/logbooks/{logbook_id}/qsos/{id}",
            get(qso::get).put(qso::update).delete(qso::delete),
        )
        .route("/api", any(api_not_found))
        .route("/api/{*rest}", any(api_not_found))
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

async fn version(_: auth::SignedIn) -> Response {
    Json(json!({ "version": env!("CARGO_PKG_VERSION"), "commit": option_env!("JL1HIS_COMMIT") })).into_response()
}

async fn api_not_found() -> Response {
    ApiError::new(StatusCode::NOT_FOUND, "not found").into_response()
}
