use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::{
    store::{
        WriteTx,
        token::{self, ApiToken, TokenInput, TokenKind},
    },
    web::{
        AppState,
        auth::SignedIn,
        error::ApiError,
        extract::{Json, Query},
    },
};

#[derive(Debug, Serialize)]
pub struct TokenBody {
    id: String,
    user_id: String,
    kind: &'static str,
    name: String,
    logbook_ids: Vec<String>,
    created_at: String,
    last_used_at: Option<String>,
}

impl From<ApiToken> for TokenBody {
    fn from(token: ApiToken) -> TokenBody {
        TokenBody {
            id: token.id,
            user_id: token.user_id,
            kind: token.kind.as_str(),
            name: token.name,
            logbook_ids: token.logbook_ids,
            created_at: token.created_at,
            last_used_at: token.last_used_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct IssuedBody {
    token: TokenBody,
    plaintext: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KindRequest {
    Native,
    Wl2,
}

#[derive(Debug, Deserialize)]
pub struct TokenRequest {
    kind: KindRequest,
    name: String,
    logbook_ids: Vec<String>,
    #[serde(default)]
    wl2_prefix: bool,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    user_id: Option<String>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    SignedIn(user): SignedIn,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<TokenBody>>, ApiError> {
    let user_id = query.user_id.as_deref().unwrap_or(user.user_id());
    let mut conn = state.db.acquire().await?;
    let tokens = token::list(&mut conn, &user, user_id).await?;
    Ok(Json(tokens.into_iter().map(TokenBody::from).collect()))
}

pub async fn issue(
    State(state): State<Arc<AppState>>,
    SignedIn(user): SignedIn,
    Json(req): Json<TokenRequest>,
) -> Result<(StatusCode, Json<IssuedBody>), ApiError> {
    let input = TokenInput {
        kind: match req.kind {
            KindRequest::Native => TokenKind::Native,
            KindRequest::Wl2 => TokenKind::Wl2,
        },
        name: req.name,
        logbook_ids: req.logbook_ids,
        wl2_prefix: req.wl2_prefix,
    };
    let mut tx = WriteTx::begin(&state.db).await?;
    let issued = token::issue(&mut tx, &user, &input).await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(IssuedBody {
            token: issued.token.into(),
            plaintext: issued.plaintext,
        }),
    ))
}

pub async fn revoke(
    State(state): State<Arc<AppState>>,
    SignedIn(user): SignedIn,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let mut tx = WriteTx::begin(&state.db).await?;
    token::revoke(&mut tx, &user, &id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::web::testing::{Auth, Harness};

    #[tokio::test]
    async fn issues_lists_and_revokes_tokens() {
        let h = Harness::new().await;
        let operator = h.session(&h.world.operator).await;
        let logbook_id = h.world.logbook.id.as_str();

        let request = json!({ "kind": "native", "name": "hislog", "logbook_ids": [logbook_id] });
        let (status, issued) = h
            .send("POST", "/api/tokens", Auth::Session(&operator), Some(request))
            .await;
        assert_eq!(status, StatusCode::CREATED, "{issued}");
        assert_eq!(issued["token"]["kind"], "native");
        assert_eq!(issued["token"]["logbook_ids"], json!([logbook_id]));
        let plaintext = issued["plaintext"].as_str().unwrap();

        let (status, _) = h
            .send(
                "GET",
                &format!("/api/logbooks/{logbook_id}"),
                Auth::Bearer(plaintext),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = h.send("GET", "/api/tokens", Auth::Bearer(plaintext), None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let grayline = json!({ "kind": "wl2", "name": "grayline", "logbook_ids": [logbook_id], "wl2_prefix": true });
        let (status, wl2) = h
            .send("POST", "/api/tokens", Auth::Session(&operator), Some(grayline))
            .await;
        assert_eq!(status, StatusCode::CREATED);
        assert!(wl2["plaintext"].as_str().unwrap().starts_with("wl2_"));

        let (status, list) = h.send("GET", "/api/tokens", Auth::Session(&operator), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(list[0]["id"], issued["token"]["id"]);
        assert!(list[0]["last_used_at"].is_string());
        assert!(list[0].get("plaintext").is_none());
        assert_eq!(list[1], wl2["token"]);

        let outsider = h.session(&h.world.outsider).await;
        let elsewhere = json!({ "kind": "native", "name": "x", "logbook_ids": [logbook_id] });
        let (status, _) = h
            .send("POST", "/api/tokens", Auth::Session(&outsider), Some(elsewhere))
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let others = format!("/api/tokens?user_id={}", h.world.operator.id);
        let (status, _) = h.send("GET", &others, Auth::Session(&outsider), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let revoke = format!("/api/tokens/{}", issued["token"]["id"].as_str().unwrap());
        let (status, _) = h.send("DELETE", &revoke, Auth::Session(&outsider), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let admin = h.session(&h.world.admin).await;
        let (status, list) = h.send("GET", &others, Auth::Session(&admin), None).await;
        assert_eq!((status, list.as_array().unwrap().len()), (StatusCode::OK, 2));
        let (status, _) = h.send("DELETE", &revoke, Auth::Session(&operator), None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _) = h
            .send(
                "GET",
                &format!("/api/logbooks/{logbook_id}"),
                Auth::Bearer(plaintext),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}
