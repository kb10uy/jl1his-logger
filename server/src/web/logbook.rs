use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::{
    store::{
        StoreError, WriteTx,
        access::{self, LogbookRole},
        logbook::{self, Logbook},
        membership::{self, Member},
        user,
    },
    web::{AppState, auth::Caller, error::ApiError, extract::Json},
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

#[derive(Debug, Serialize)]
pub struct MemberBody {
    user_id: String,
    callsign: String,
    role: &'static str,
    disabled: bool,
}

impl From<Member> for MemberBody {
    fn from(member: Member) -> MemberBody {
        MemberBody {
            user_id: member.user_id,
            callsign: member.callsign,
            role: member.role.as_str(),
            disabled: member.disabled,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoleRequest {
    Owner,
    Operator,
}

#[derive(Debug, Deserialize)]
pub struct MembershipRequest {
    role: RoleRequest,
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

pub async fn members(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path(logbook_id): Path<String>,
) -> Result<Json<Vec<MemberBody>>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let access = access::logbook(&mut conn, &caller.principal, &logbook_id).await?;
    let members = membership::list(&mut conn, &access).await?;
    Ok(Json(members.into_iter().map(MemberBody::from).collect()))
}

pub async fn set_member(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path((logbook_id, callsign)): Path<(String, String)>,
    Json(req): Json<MembershipRequest>,
) -> Result<Json<Vec<MemberBody>>, ApiError> {
    let role = match req.role {
        RoleRequest::Owner => LogbookRole::Owner,
        RoleRequest::Operator => LogbookRole::Operator,
    };
    let mut tx = WriteTx::begin(&state.db).await?;
    let access = access::logbook(&mut tx, &caller.principal, &logbook_id).await?;
    let owner = access.owner()?;
    let user = user::find_by_callsign(&mut tx, &callsign)
        .await?
        .ok_or(StoreError::NotFound("user"))?;
    membership::set(&mut tx, &owner, &user.id, role).await?;
    let members = membership::list(&mut tx, &access).await?;
    tx.commit().await?;
    Ok(Json(members.into_iter().map(MemberBody::from).collect()))
}

pub async fn remove_member(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path((logbook_id, callsign)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let mut tx = WriteTx::begin(&state.db).await?;
    let access = access::logbook(&mut tx, &caller.principal, &logbook_id).await?;
    let owner = access.owner()?;
    let user = user::find_by_callsign(&mut tx, &callsign)
        .await?
        .ok_or(StoreError::NotFound("Membership"))?;
    membership::remove(&mut tx, &owner, &user.id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
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
    async fn owners_manage_members_by_callsign() {
        let h = Harness::new().await;
        let members = format!("/api/logbooks/{}/members", h.world.logbook.id);
        let owner = h.session(&h.world.owner).await;
        let operator = h.session(&h.world.operator).await;
        let outsider = h.session(&h.world.outsider).await;

        let (status, list) = h.send("GET", &members, Auth::Session(&operator), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            list,
            json!([
                { "user_id": h.world.operator.id, "callsign": "JA1OPR", "role": "operator", "disabled": false },
                { "user_id": h.world.owner.id, "callsign": "JA1OWN", "role": "owner", "disabled": false },
            ])
        );

        let operator_role = Some(json!({ "role": "operator" }));
        let (status, _) = h
            .send(
                "PUT",
                &format!("{members}/JA1OUT"),
                Auth::Session(&operator),
                operator_role.clone(),
            )
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, list) = h
            .send(
                "PUT",
                &format!("{members}/ja1out"),
                Auth::Session(&owner),
                operator_role.clone(),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(list[1]["callsign"], "JA1OUT");
        let (status, _) = h.send("GET", &members, Auth::Session(&outsider), None).await;
        assert_eq!(status, StatusCode::OK);

        let (status, _) = h
            .send(
                "PUT",
                &format!("{members}/JA1NONE"),
                Auth::Session(&owner),
                operator_role,
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _) = h
            .send(
                "PUT",
                &format!("{members}/JA1OUT"),
                Auth::Session(&owner),
                Some(json!({ "role": "admin" })),
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

        let (status, _) = h
            .send("DELETE", &format!("{members}/JA1OWN"), Auth::Session(&owner), None)
            .await;
        assert_eq!(status, StatusCode::CONFLICT);
        let (status, _) = h
            .send("DELETE", &format!("{members}/JA1OUT"), Auth::Session(&owner), None)
            .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _) = h.send("GET", &members, Auth::Session(&outsider), None).await;
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
