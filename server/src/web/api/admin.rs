use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::{
    db,
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

pub async fn delete_logbook(
    State(state): State<Arc<AppState>>,
    Admin(admin): Admin,
    Path(logbook_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let mut conn = state.db.acquire().await?;
    let known = logbook::list_all(&mut conn, &admin).await?;
    drop(conn);
    if !known.iter().any(|l| l.id == logbook_id) {
        return Err(StoreError::NotFound("Logbook").into());
    }
    let dir = db::snapshot_dir(&state.config.database.path);
    db::snapshot(&state.db, &dir, "before-delete")
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let mut tx = WriteTx::begin(&state.db).await?;
    logbook::delete(&mut tx, &admin, &logbook_id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use axum::http::StatusCode;
    use serde_json::json;

    use crate::{
        config::{Config, DatabaseConfig},
        web::testing::{Auth, Harness},
    };

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

    #[tokio::test]
    async fn admins_delete_a_logbook_after_a_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            database: DatabaseConfig {
                path: dir.path().join("live.db"),
            },
            ..Config::default()
        };
        let h = Harness::with_config(config).await;
        let admin = h.session(&h.world.admin).await;
        let owner = h.session(&h.world.owner).await;
        let path = format!("/api/admin/logbooks/{}", h.world.logbook.id);
        let snapshots = dir.path().join("snapshots");

        let (status, _) = h.send("DELETE", &path, Auth::Session(&owner), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _) = h
            .send("DELETE", "/api/admin/logbooks/nope", Auth::Session(&admin), None)
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(!snapshots.exists());

        let (status, _) = h.send("DELETE", &path, Auth::Session(&admin), None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let written: Vec<_> = fs::read_dir(&snapshots)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(written.len(), 1);
        assert!(written[0].ends_with("-before-delete.db"));

        let (status, body) = h.send("GET", "/api/admin/logbooks", Auth::Session(&admin), None).await;
        assert_eq!((status, body), (StatusCode::OK, json!([])));
        let (status, body) = h.send("GET", "/api/logbooks", Auth::Session(&owner), None).await;
        assert_eq!((status, body), (StatusCode::OK, json!([])));
        let (status, _) = h.send("DELETE", &path, Auth::Session(&admin), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
