use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use crate::{
    store::{
        WriteTx,
        account::session::{self, Session},
        hash_token,
    },
    web::{
        AppState,
        api::auth::{SESSION_COOKIE, SignedIn},
        error::ApiError,
        extract::{Json, Query},
    },
};

#[derive(Debug, Serialize)]
pub struct SessionBody {
    id: String,
    user_id: String,
    user_agent: Option<String>,
    created_at: String,
    last_used_at: String,
    expires_at: String,
    current: bool,
}

impl SessionBody {
    fn new(session: Session, current_hash: Option<&[u8]>) -> SessionBody {
        SessionBody {
            current: current_hash == Some(session.token_hash.as_slice()),
            id: session.id,
            user_id: session.user_id,
            user_agent: session.user_agent,
            created_at: session.created_at,
            last_used_at: session.last_used_at,
            expires_at: session.expires_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    user_id: Option<String>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    SignedIn(user): SignedIn,
    jar: CookieJar,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<SessionBody>>, ApiError> {
    let user_id = query.user_id.as_deref().unwrap_or(user.user_id());
    let current = jar.get(SESSION_COOKIE).map(|cookie| hash_token(cookie.value()));
    let mut conn = state.db.acquire().await?;
    let sessions = session::list(&mut conn, &user, user_id).await?;
    Ok(Json(
        sessions
            .into_iter()
            .map(|session| SessionBody::new(session, current.as_deref()))
            .collect(),
    ))
}

pub async fn revoke(
    State(state): State<Arc<AppState>>,
    SignedIn(user): SignedIn,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let mut tx = WriteTx::begin(&state.db).await?;
    session::revoke(&mut tx, &user, &id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::web::testing::{Auth, Harness};

    #[tokio::test]
    async fn lists_the_users_sessions_marking_the_current_one() {
        let h = Harness::new().await;
        let phone = h.session(&h.world.operator).await;
        let laptop = h.session(&h.world.operator).await;

        let (status, body) = h.send("GET", "/api/sessions", Auth::Session(&laptop), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let sessions = body.as_array().unwrap();
        assert_eq!(sessions.len(), 2);
        let current: Vec<_> = sessions.iter().filter(|s| s["current"] == json!(true)).collect();
        assert_eq!(current.len(), 1);
        assert_eq!(current[0]["user_id"], h.world.operator.id);
        assert!(sessions[0].get("token_hash").is_none());

        let (status, _) = h.send("GET", "/api/sessions", Auth::Session(&phone), None).await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = h.send("GET", "/api/sessions", Auth::Anonymous, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn revokes_a_session_of_the_user_or_for_an_admin() {
        let h = Harness::new().await;
        let phone = h.session(&h.world.operator).await;
        let laptop = h.session(&h.world.operator).await;
        let (_, body) = h.send("GET", "/api/sessions", Auth::Session(&laptop), None).await;
        let other = body
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["current"] == json!(false))
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let uri = format!("/api/sessions/{other}");

        let outsider = h.session(&h.world.outsider).await;
        let (status, _) = h.send("DELETE", &uri, Auth::Session(&outsider), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let others = format!("/api/sessions?user_id={}", h.world.operator.id);
        let (status, _) = h.send("GET", &others, Auth::Session(&outsider), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, _) = h.send("DELETE", &uri, Auth::Session(&laptop), None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _) = h.send("GET", "/api/me", Auth::Session(&phone), None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let admin = h.session(&h.world.admin).await;
        let (status, listed) = h.send("GET", &others, Auth::Session(&admin), None).await;
        assert_eq!((status, listed.as_array().unwrap().len()), (StatusCode::OK, 1));
        let remaining = format!("/api/sessions/{}", listed[0]["id"].as_str().unwrap());
        let (status, _) = h.send("DELETE", &remaining, Auth::Session(&admin), None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _) = h.send("GET", "/api/me", Auth::Session(&laptop), None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}
