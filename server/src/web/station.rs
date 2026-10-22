use std::{collections::BTreeMap, sync::Arc};

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::{
    store::{
        WriteTx, access,
        qso::TimeRange,
        station::{self, Station, StationInput},
    },
    web::{
        AppState,
        auth::Caller,
        error::ApiError,
        extract::{Json, Query},
        qso::time,
    },
};

#[derive(Debug, Serialize)]
pub struct StationBody {
    id: String,
    wavelog_id: Option<i64>,
    name: String,
    callsign: String,
    fields: BTreeMap<String, String>,
    archived_at: Option<String>,
    created_at: String,
    updated_at: String,
}

impl From<Station> for StationBody {
    fn from(station: Station) -> StationBody {
        StationBody {
            id: station.id,
            wavelog_id: station.wavelog_id,
            name: station.name,
            callsign: station.callsign,
            fields: station.fields,
            archived_at: station.archived_at,
            created_at: station.created_at,
            updated_at: station.updated_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct StationRequest {
    name: String,
    callsign: String,
    wavelog_id: Option<i64>,
    #[serde(default)]
    fields: BTreeMap<String, String>,
}

impl From<StationRequest> for StationInput {
    fn from(req: StationRequest) -> StationInput {
        StationInput {
            name: req.name,
            callsign: req.callsign,
            wavelog_id: req.wavelog_id,
            fields: req.fields,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RangeQuery {
    from: Option<String>,
    until: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RewriteRequest {
    from: Option<String>,
    until: Option<String>,
    count: usize,
}

#[derive(Debug, Serialize)]
pub struct Count {
    count: usize,
}

fn range(from: Option<&str>, until: Option<&str>) -> Result<TimeRange, ApiError> {
    Ok(TimeRange {
        from: from.map(|t| time("from", t)).transpose()?,
        until: until.map(|t| time("until", t)).transpose()?,
    })
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path(logbook_id): Path<String>,
) -> Result<Json<Vec<StationBody>>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let access = access::logbook(&mut conn, &caller.principal, &logbook_id).await?;
    let stations = station::list(&mut conn, &access).await?;
    Ok(Json(stations.into_iter().map(StationBody::from).collect()))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path((logbook_id, id)): Path<(String, String)>,
) -> Result<Json<StationBody>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let access = access::logbook(&mut conn, &caller.principal, &logbook_id).await?;
    Ok(Json(station::get(&mut conn, &access, &id).await?.into()))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path(logbook_id): Path<String>,
    Json(req): Json<StationRequest>,
) -> Result<(StatusCode, Json<StationBody>), ApiError> {
    let mut tx = WriteTx::begin(&state.db).await?;
    let access = access::logbook(&mut tx, &caller.principal, &logbook_id).await?;
    let created = station::create(&mut tx, &access.owner()?, &req.into()).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(created.into())))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path((logbook_id, id)): Path<(String, String)>,
    Json(req): Json<StationRequest>,
) -> Result<Json<StationBody>, ApiError> {
    let mut tx = WriteTx::begin(&state.db).await?;
    let access = access::logbook(&mut tx, &caller.principal, &logbook_id).await?;
    let updated = station::update(&mut tx, &access.owner()?, &id, &req.into()).await?;
    tx.commit().await?;
    Ok(Json(updated.into()))
}

pub async fn archive(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path((logbook_id, id)): Path<(String, String)>,
) -> Result<Json<StationBody>, ApiError> {
    set_archived(&state, &caller, &logbook_id, &id, true).await
}

pub async fn unarchive(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path((logbook_id, id)): Path<(String, String)>,
) -> Result<Json<StationBody>, ApiError> {
    set_archived(&state, &caller, &logbook_id, &id, false).await
}

async fn set_archived(
    state: &AppState,
    caller: &Caller,
    logbook_id: &str,
    id: &str,
    archived: bool,
) -> Result<Json<StationBody>, ApiError> {
    let mut tx = WriteTx::begin(&state.db).await?;
    let access = access::logbook(&mut tx, &caller.principal, logbook_id).await?;
    let station = station::set_archived(&mut tx, &access.owner()?, id, archived).await?;
    tx.commit().await?;
    Ok(Json(station.into()))
}

pub async fn preview_rewrite(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path((logbook_id, id)): Path<(String, String)>,
    Query(query): Query<RangeQuery>,
) -> Result<Json<Count>, ApiError> {
    let range = range(query.from.as_deref(), query.until.as_deref())?;
    let mut conn = state.db.acquire().await?;
    let access = access::logbook(&mut conn, &caller.principal, &logbook_id).await?;
    let count = station::preview_rewrite(&mut conn, &access.owner()?, &id, range).await?;
    Ok(Json(Count { count }))
}

pub async fn rewrite(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path((logbook_id, id)): Path<(String, String)>,
    Json(req): Json<RewriteRequest>,
) -> Result<Json<Count>, ApiError> {
    let range = range(req.from.as_deref(), req.until.as_deref())?;
    let mut tx = WriteTx::begin(&state.db).await?;
    let access = access::logbook(&mut tx, &caller.principal, &logbook_id).await?;
    let count = station::rewrite_qsos(&mut tx, &access.owner()?, &id, range, req.count).await?;
    tx.commit().await?;
    Ok(Json(Count { count }))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::{
        domain::qso::QsoId,
        web::testing::{Auth, Harness},
    };

    fn stations(h: &Harness) -> String {
        format!("/api/logbooks/{}/stations", h.world.logbook.id)
    }

    fn portable() -> Value {
        json!({
            "name": "Portable",
            "callsign": "jl1his/p",
            "wavelog_id": 7,
            "fields": { "my_gridsquare": "PM95" },
        })
    }

    #[tokio::test]
    async fn owners_manage_stations_and_members_read_them() {
        let h = Harness::new().await;
        let owner = h.session(&h.world.owner).await;
        let operator = h.session(&h.world.operator).await;

        let (status, _) = h
            .send("POST", &stations(&h), Auth::Session(&operator), Some(portable()))
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let (status, created) = h
            .send("POST", &stations(&h), Auth::Session(&owner), Some(portable()))
            .await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        assert_eq!(created["callsign"], "JL1HIS/P");
        assert_eq!(created["wavelog_id"], 7);
        assert_eq!(created["fields"], json!({ "MY_GRIDSQUARE": "PM95" }));
        let uri = format!("{}/{}", stations(&h), created["id"].as_str().unwrap());

        let mut renamed = portable();
        renamed["name"] = json!("Mobile");
        let (status, updated) = h.send("PUT", &uri, Auth::Session(&owner), Some(renamed.clone())).await;
        assert_eq!((status, &updated["name"]), (StatusCode::OK, &json!("Mobile")));
        let (status, _) = h.send("PUT", &uri, Auth::Session(&operator), Some(renamed)).await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let (status, archived) = h
            .send("POST", &format!("{uri}/archive"), Auth::Session(&owner), None)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert!(archived["archived_at"].is_string());

        let (status, list) = h.send("GET", &stations(&h), Auth::Session(&operator), None).await;
        assert_eq!(status, StatusCode::OK);
        let names: Vec<&str> = list
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Home", "Mobile"]);

        let (_, restored) = h
            .send("DELETE", &format!("{uri}/archive"), Auth::Session(&owner), None)
            .await;
        assert_eq!(restored["archived_at"], Value::Null);
        let (status, fetched) = h.send("GET", &uri, Auth::Session(&operator), None).await;
        assert_eq!((status, fetched), (StatusCode::OK, restored));
    }

    #[tokio::test]
    async fn rewrites_past_qsos_by_the_previewed_count() {
        let h = Harness::new().await;
        let owner = h.session(&h.world.owner).await;
        let uri = format!("{}/{}", stations(&h), h.station.id);
        let qso = json!({
            "id": QsoId::generate().to_string(),
            "station_id": h.station.id,
            "fields": { "CALL": "JA1XYZ", "QSO_DATE": "20261022", "TIME_ON": "0100", "BAND": "40m", "MODE": "CW" },
        });
        let (status, created) = h
            .send(
                "POST",
                &format!("/api/logbooks/{}/qsos", h.world.logbook.id),
                Auth::Session(&owner),
                Some(qso),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(created["qso"]["fields"]["MY_CITY"], "Minato");

        let moved = json!({
            "name": "Home",
            "callsign": "JL1HIS/1",
            "fields": { "MY_CITY": "Shiba", "MY_CNTY": "100103" },
        });
        let (status, _) = h.send("PUT", &uri, Auth::Session(&owner), Some(moved)).await;
        assert_eq!(status, StatusCode::OK);

        let rewrite = format!("{uri}/rewrite");
        let (status, preview) = h.send("GET", &rewrite, Auth::Session(&owner), None).await;
        assert_eq!((status, preview), (StatusCode::OK, json!({ "count": 1 })));
        let (_, outside) = h
            .send(
                "GET",
                &format!("{rewrite}?from=2026-10-23T00:00:00Z"),
                Auth::Session(&owner),
                None,
            )
            .await;
        assert_eq!(outside, json!({ "count": 0 }));

        let (status, _) = h
            .send("POST", &rewrite, Auth::Session(&owner), Some(json!({ "count": 2 })))
            .await;
        assert_eq!(status, StatusCode::CONFLICT);
        let (status, done) = h
            .send("POST", &rewrite, Auth::Session(&owner), Some(json!({ "count": 1 })))
            .await;
        assert_eq!((status, done), (StatusCode::OK, json!({ "count": 1 })));

        let qso_uri = format!(
            "/api/logbooks/{}/qsos/{}",
            h.world.logbook.id,
            created["qso"]["id"].as_str().unwrap()
        );
        let (_, rewritten) = h.send("GET", &qso_uri, Auth::Session(&owner), None).await;
        assert_eq!(rewritten["fields"]["MY_CITY"], "Shiba");
        assert_eq!(rewritten["revision"], 2);
    }
}
