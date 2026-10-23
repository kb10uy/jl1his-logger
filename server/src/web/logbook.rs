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
        membership::{self, Member as Membership},
        user,
    },
    web::{AppState, auth::Caller, error::ApiError, extract::Json, member::Member},
};

#[derive(Debug, Serialize)]
pub struct LogbookBody {
    id: String,
    name: String,
    role: LogbookRole,
    cursor_generation: String,
    last_seq: i64,
    last_updated_seq: i64,
}

impl LogbookBody {
    fn new(logbook: Logbook, role: LogbookRole) -> LogbookBody {
        LogbookBody {
            id: logbook.id,
            name: logbook.name,
            role,
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
    role: LogbookRole,
    disabled: bool,
}

impl From<Membership> for MemberBody {
    fn from(member: Membership) -> MemberBody {
        MemberBody {
            user_id: member.user_id,
            callsign: member.callsign,
            role: member.role,
            disabled: member.disabled,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct MemberPath {
    callsign: String,
}

#[derive(Debug, Deserialize)]
pub struct MembershipRequest {
    role: LogbookRole,
}

pub async fn list(State(state): State<Arc<AppState>>, caller: Caller) -> Result<Json<Vec<LogbookBody>>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let logbooks = match &caller.scopes {
        None => {
            let user = access::user(&mut conn, &caller.principal).await?;
            logbook::list(&mut conn, &user).await?
        }
        Some(scopes) => logbook::list_for_scopes(&mut conn, &caller.principal, scopes).await?,
    };
    Ok(Json(
        logbooks
            .into_iter()
            .map(|(logbook, role)| LogbookBody::new(logbook, role))
            .collect(),
    ))
}

pub async fn get(State(state): State<Arc<AppState>>, member: Member) -> Result<Json<LogbookBody>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let logbook = logbook::get(&mut conn, &member.access).await?;
    Ok(Json(LogbookBody::new(logbook, member.access.role())))
}

pub async fn members(State(state): State<Arc<AppState>>, member: Member) -> Result<Json<Vec<MemberBody>>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let members = membership::list(&mut conn, &member.access).await?;
    Ok(Json(members.into_iter().map(MemberBody::from).collect()))
}

pub async fn set_member(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(path): Path<MemberPath>,
    Json(req): Json<MembershipRequest>,
) -> Result<Json<Vec<MemberBody>>, ApiError> {
    let owner = member.access.owner()?;
    let mut tx = WriteTx::begin(&state.db).await?;
    let user_id = user_id(&mut tx, &path.callsign).await?;
    membership::set(&mut tx, &owner, &user_id, req.role).await?;
    let members = membership::list(&mut tx, &member.access).await?;
    tx.commit().await?;
    Ok(Json(members.into_iter().map(MemberBody::from).collect()))
}

pub async fn remove_member(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(path): Path<MemberPath>,
) -> Result<StatusCode, ApiError> {
    let owner = member.access.owner()?;
    let mut tx = WriteTx::begin(&state.db).await?;
    let user_id = user_id(&mut tx, &path.callsign).await?;
    membership::remove(&mut tx, &owner, &user_id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn user_id(tx: &mut WriteTx, callsign: &str) -> Result<String, StoreError> {
    Ok(user::find_by_callsign(tx, callsign)
        .await?
        .ok_or(StoreError::NotFound("user"))?
        .id)
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

        let (status, body) = h.send("GET", &h.path(""), Auth::Bearer(&token), None).await;
        assert_eq!((status, &body), (StatusCode::OK, &expected[0]));

        let outsider = h.session(&h.world.outsider).await;
        let (status, body) = h.send("GET", "/api/logbooks", Auth::Session(&outsider), None).await;
        assert_eq!((status, body), (StatusCode::OK, json!([])));
        let (status, _) = h.send("GET", &h.path(""), Auth::Session(&outsider), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn owners_manage_members_by_callsign() {
        let h = Harness::new().await;
        let members = h.path("/members");
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
            .send("DELETE", &format!("{members}/JA1NONE"), Auth::Session(&owner), None)
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
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
