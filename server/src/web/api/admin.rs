use std::sync::Arc;

use axum::{extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};

use crate::{
    store::{
        StoreError, WriteTx,
        account::user,
        logbook::{self, Owned},
    },
    web::{AppState, api::auth::Admin, error::ApiError, extract::Json},
};

#[derive(Debug, Serialize)]
pub struct LogbookBody {
    id: String,
    name: String,
    last_seq: i64,
    owners: Vec<String>,
}

impl From<Owned> for LogbookBody {
    fn from(owned: Owned) -> LogbookBody {
        LogbookBody {
            id: owned.logbook.id,
            name: owned.logbook.name,
            last_seq: owned.logbook.last_seq,
            owners: owned.owners,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateLogbookRequest {
    name: String,
    owner: String,
}

pub async fn list_logbooks(
    State(state): State<Arc<AppState>>,
    Admin(admin): Admin,
) -> Result<Json<Vec<LogbookBody>>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let logbooks = logbook::list_all_with_owners(&mut conn, &admin).await?;
    Ok(Json(logbooks.into_iter().map(LogbookBody::from).collect()))
}

pub async fn create_logbook(
    State(state): State<Arc<AppState>>,
    Admin(admin): Admin,
    Json(req): Json<CreateLogbookRequest>,
) -> Result<(StatusCode, Json<LogbookBody>), ApiError> {
    let mut tx = WriteTx::begin(&state.db).await?;
    let owner = user::find_by_callsign(&mut tx, &req.owner)
        .await?
        .ok_or(StoreError::NotFound("user"))?;
    let created = logbook::create(&mut tx, &admin, &req.name, &owner.id).await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(LogbookBody {
            id: created.id,
            name: created.name,
            last_seq: created.last_seq,
            owners: vec![owner.callsign],
        }),
    ))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;

    use crate::web::testing::{Auth, Harness};

    #[tokio::test]
    async fn admins_list_and_create_logbooks() {
        let h = Harness::new().await;
        let admin = h.session(&h.world.admin).await;
        let owner = h.session(&h.world.owner).await;
        let outsider = h.session(&h.world.outsider).await;
        let club = json!({
            "id": h.world.logbook.id,
            "name": "Club",
            "last_seq": 0,
            "owners": ["JA1OWN"],
        });

        let (status, body) = h.send("GET", "/api/admin/logbooks", Auth::Session(&admin), None).await;
        assert_eq!((status, body), (StatusCode::OK, json!([club])));
        let (status, _) = h.send("GET", "/api/admin/logbooks", Auth::Session(&owner), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let token = h.token(&h.world.owner, &[&h.world.logbook.id]).await;
        let (status, _) = h.send("GET", "/api/admin/logbooks", Auth::Bearer(&token), None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let contest = Some(json!({ "name": " Contest ", "owner": "ja1out" }));
        let (status, _) = h
            .send("POST", "/api/admin/logbooks", Auth::Session(&owner), contest.clone())
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, body) = h
            .send("POST", "/api/admin/logbooks", Auth::Session(&admin), contest)
            .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body["name"], "Contest");
        assert_eq!(body["owners"], json!(["JA1OUT"]));
        let id = body["id"].as_str().unwrap().to_owned();

        let (status, body) = h.send("GET", "/api/admin/logbooks", Auth::Session(&admin), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body[0], club);
        assert_eq!(body[1]["id"], id);
        let (status, body) = h.send("GET", "/api/logbooks", Auth::Session(&outsider), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body[0]["id"], id);
        assert_eq!(body[0]["role"], "owner");

        let (status, body) = h
            .send(
                "POST",
                "/api/admin/logbooks",
                Auth::Session(&admin),
                Some(json!({ "name": "Nobody's", "owner": "JA1NONE" })),
            )
            .await;
        assert_eq!((status, &body["what"]), (StatusCode::NOT_FOUND, &json!("user")));
        let (status, body) = h
            .send(
                "POST",
                "/api/admin/logbooks",
                Auth::Session(&admin),
                Some(json!({ "name": " ", "owner": "JA1OUT" })),
            )
            .await;
        assert_eq!(
            (status, &body["code"]),
            (StatusCode::BAD_REQUEST, &json!("name_required"))
        );
    }
}
