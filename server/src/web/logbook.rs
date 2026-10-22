use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use serde::Serialize;

use crate::{
    store::{
        StoreError,
        access::{self, LogbookRole},
        logbook::{self, Logbook},
    },
    web::{AppState, auth::Caller, error::ApiError},
};

#[derive(Debug, Serialize)]
pub struct LogbookBody {
    id: String,
    name: String,
    role: &'static str,
    cursor_generation: String,
    last_seq: i64,
    last_updated_seq: i64,
}

impl LogbookBody {
    fn new(logbook: Logbook, role: LogbookRole) -> LogbookBody {
        LogbookBody {
            id: logbook.id,
            name: logbook.name,
            role: role.as_str(),
            cursor_generation: logbook.cursor_generation,
            last_seq: logbook.last_seq,
            last_updated_seq: logbook.last_updated_seq,
        }
    }
}

pub async fn list(State(state): State<Arc<AppState>>, caller: Caller) -> Result<Json<Vec<LogbookBody>>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let mut logbooks = match &caller.scopes {
        None => {
            let user = access::user(&mut conn, &caller.principal).await?;
            logbook::list(&mut conn, &user).await?
        }
        Some(scopes) => {
            let mut logbooks = vec![];
            for logbook_id in scopes {
                match access::logbook(&mut conn, &caller.principal, logbook_id).await {
                    Ok(access) => logbooks.push((logbook::get(&mut conn, &access).await?, access.role())),
                    Err(StoreError::NotFound(_)) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            logbooks
        }
    };
    logbooks.sort_by(|(a, _), (b, _)| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
    Ok(Json(
        logbooks
            .into_iter()
            .map(|(logbook, role)| LogbookBody::new(logbook, role))
            .collect(),
    ))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path(logbook_id): Path<String>,
) -> Result<Json<LogbookBody>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let access = access::logbook(&mut conn, &caller.principal, &logbook_id).await?;
    let logbook = logbook::get(&mut conn, &access).await?;
    Ok(Json(LogbookBody::new(logbook, access.role())))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;

    use crate::{
        store::token::TokenKind,
        web::testing::{Auth, Harness},
    };

    #[tokio::test]
    async fn lists_the_logbooks_of_the_caller() {
        let h = Harness::new().await;
        let logbook = &h.world.logbook;
        let expected = json!([{
            "id": logbook.id,
            "name": "Club",
            "role": "operator",
            "cursor_generation": logbook.cursor_generation,
            "last_seq": 0,
            "last_updated_seq": 0,
        }]);

        let operator = h.session(&h.world.operator).await;
        let (status, body) = h.send("GET", "/api/logbooks", Auth::Session(&operator), None).await;
        assert_eq!((status, &body), (StatusCode::OK, &expected));

        let token = h.token(&h.world.operator, TokenKind::Native, &[&logbook.id]).await;
        let (status, body) = h.send("GET", "/api/logbooks", Auth::Bearer(&token), None).await;
        assert_eq!((status, &body), (StatusCode::OK, &expected));

        let (status, body) = h
            .send(
                "GET",
                &format!("/api/logbooks/{}", logbook.id),
                Auth::Bearer(&token),
                None,
            )
            .await;
        assert_eq!((status, &body), (StatusCode::OK, &expected[0]));

        let outsider = h.session(&h.world.outsider).await;
        let (status, body) = h.send("GET", "/api/logbooks", Auth::Session(&outsider), None).await;
        assert_eq!((status, body), (StatusCode::OK, json!([])));
        let (status, _) = h
            .send(
                "GET",
                &format!("/api/logbooks/{}", logbook.id),
                Auth::Session(&outsider),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn drops_scopes_the_token_user_lost() {
        let h = Harness::new().await;
        let token = h
            .token(&h.world.operator, TokenKind::Native, &[&h.world.logbook.id])
            .await;
        sqlx::query("DELETE FROM memberships WHERE user_id = ?")
            .bind(&h.world.operator.id)
            .execute(&h.world.db)
            .await
            .unwrap();
        let (status, body) = h.send("GET", "/api/logbooks", Auth::Bearer(&token), None).await;
        assert_eq!((status, body), (StatusCode::OK, json!([])));
    }
}
