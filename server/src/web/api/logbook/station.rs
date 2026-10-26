use std::{collections::BTreeMap, sync::Arc};

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::{
    store::{
        WriteTx,
        logbook::station::{self, Station, StationInput},
    },
    web::{
        AppState,
        api::logbook::member::{Item, Member},
        error::ApiError,
        extract::{Json, Query},
        query::{Count, RangeQuery, RewriteRequest, range},
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
    favorite: bool,
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
            favorite: station.favorite,
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

pub async fn list(State(state): State<Arc<AppState>>, member: Member) -> Result<Json<Vec<StationBody>>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let stations = station::list(&mut conn, &member.access).await?;
    Ok(Json(stations.into_iter().map(StationBody::from).collect()))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(item): Path<Item>,
) -> Result<Json<StationBody>, ApiError> {
    let mut conn = state.db.acquire().await?;
    Ok(Json(station::get(&mut conn, &member.access, &item.id).await?.into()))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    member: Member,
    Json(req): Json<StationRequest>,
) -> Result<(StatusCode, Json<StationBody>), ApiError> {
    let owner = member.access.owner()?;
    let mut tx = WriteTx::begin(&state.db).await?;
    let created = station::create(&mut tx, &owner, &req.into()).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(created.into())))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(item): Path<Item>,
    Json(req): Json<StationRequest>,
) -> Result<Json<StationBody>, ApiError> {
    let owner = member.access.owner()?;
    let mut tx = WriteTx::begin(&state.db).await?;
    let updated = station::update(&mut tx, &owner, &item.id, &req.into()).await?;
    tx.commit().await?;
    Ok(Json(updated.into()))
}

pub async fn archive(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(item): Path<Item>,
) -> Result<Json<StationBody>, ApiError> {
    set_archived(&state, &member, &item.id, true).await
}

pub async fn unarchive(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(item): Path<Item>,
) -> Result<Json<StationBody>, ApiError> {
    set_archived(&state, &member, &item.id, false).await
}

async fn set_archived(
    state: &AppState,
    member: &Member,
    id: &str,
    archived: bool,
) -> Result<Json<StationBody>, ApiError> {
    let owner = member.access.owner()?;
    let mut tx = WriteTx::begin(&state.db).await?;
    let station = station::set_archived(&mut tx, &owner, id, archived).await?;
    tx.commit().await?;
    Ok(Json(station.into()))
}

pub async fn favorite(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(item): Path<Item>,
) -> Result<Json<StationBody>, ApiError> {
    set_favorite(&state, &member, &item.id, true).await
}

pub async fn unfavorite(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(item): Path<Item>,
) -> Result<Json<StationBody>, ApiError> {
    set_favorite(&state, &member, &item.id, false).await
}

async fn set_favorite(
    state: &AppState,
    member: &Member,
    id: &str,
    favorite: bool,
) -> Result<Json<StationBody>, ApiError> {
    let mut tx = WriteTx::begin(&state.db).await?;
    let station = station::set_favorite(&mut tx, &member.access, id, favorite).await?;
    tx.commit().await?;
    Ok(Json(station.into()))
}

pub async fn preview_rewrite(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(item): Path<Item>,
    Query(query): Query<RangeQuery>,
) -> Result<Json<Count>, ApiError> {
    let range = range(query.from.as_deref(), query.until.as_deref())?;
    let owner = member.access.owner()?;
    let mut conn = state.db.acquire().await?;
    let count = station::preview_rewrite(&mut conn, &owner, &item.id, range).await?;
    Ok(Json(Count { count }))
}

pub async fn rewrite(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(item): Path<Item>,
    Json(req): Json<RewriteRequest>,
) -> Result<Json<Count>, ApiError> {
    let range = range(req.from.as_deref(), req.until.as_deref())?;
    let owner = member.access.owner()?;
    let mut tx = WriteTx::begin(&state.db).await?;
    let count = station::rewrite_qsos(&mut tx, &owner, &item.id, range, req.count).await?;
    tx.commit().await?;
    Ok(Json(Count { count }))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::{
        store::account::token::TokenKind,
        web::testing::{Auth, Harness},
    };

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
        let stations = h.path("/stations");
        let owner = h.session(&h.world.owner).await;
        let operator = h.session(&h.world.operator).await;

        let (status, _) = h
            .send("POST", &stations, Auth::Session(&operator), Some(portable()))
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let (status, created) = h.send("POST", &stations, Auth::Session(&owner), Some(portable())).await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        assert_eq!(created["callsign"], "JL1HIS/P");
        assert_eq!(created["wavelog_id"], 7);
        assert_eq!(created["fields"], json!({ "MY_GRIDSQUARE": "PM95" }));
        let uri = format!("{stations}/{}", created["id"].as_str().unwrap());

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

        let (status, list) = h.send("GET", &stations, Auth::Session(&operator), None).await;
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
        let uri = h.path(&format!("/stations/{}", h.station.id));
        let created = h.create_qso(&owner, &h.station.id, "JA1XYZ", "0100", &[]).await;
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
            .send("GET", &format!("{rewrite}?from=yesterday"), Auth::Session(&owner), None)
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) = h
            .send("POST", &rewrite, Auth::Session(&owner), Some(json!({ "count": 2 })))
            .await;
        assert_eq!(status, StatusCode::CONFLICT);
        let (status, done) = h
            .send("POST", &rewrite, Auth::Session(&owner), Some(json!({ "count": 1 })))
            .await;
        assert_eq!((status, done), (StatusCode::OK, json!({ "count": 1 })));

        let qso_uri = h.path(&format!("/qsos/{}", created["qso"]["id"].as_str().unwrap()));
        let (_, rewritten) = h.send("GET", &qso_uri, Auth::Session(&owner), None).await;
        assert_eq!(rewritten["fields"]["MY_CITY"], "Shiba");
        assert_eq!(rewritten["revision"], 2);
    }

    #[tokio::test]
    async fn members_mark_their_own_favorite_stations() {
        let h = Harness::new().await;
        let stations = h.path("/stations");
        let first = h.world.station("Alpha").await;
        let owner = h.session(&h.world.owner).await;
        let operator = h.session(&h.world.operator).await;
        let outsider = h.session(&h.world.outsider).await;
        let token = h
            .token(&h.world.operator, TokenKind::Native, &[&h.world.logbook.id])
            .await;
        let favorite = format!("{stations}/{}/favorite", first.id);

        let (status, _) = h.send("POST", &favorite, Auth::Session(&outsider), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, marked) = h.send("POST", &favorite, Auth::Session(&operator), None).await;
        assert_eq!((status, &marked["favorite"]), (StatusCode::OK, &json!(true)));

        let (_, listed) = h.send("GET", &stations, Auth::Bearer(&token), None).await;
        assert_eq!(listed[0], marked);
        let (_, listed) = h.send("GET", &stations, Auth::Session(&owner), None).await;
        assert_eq!(listed[0]["favorite"], false);

        let (status, unmarked) = h.send("DELETE", &favorite, Auth::Bearer(&token), None).await;
        assert_eq!((status, &unmarked["favorite"]), (StatusCode::OK, &json!(false)));
    }
}
