use std::{collections::BTreeMap, sync::Arc};

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::{
    store::{
        WriteTx,
        instrument::{self, Instrument, InstrumentInput},
    },
    web::{
        AppState,
        error::ApiError,
        extract::Json,
        member::{Item, Member},
    },
};

#[derive(Debug, Serialize)]
pub struct InstrumentBody {
    id: String,
    name: String,
    my_rig: Option<String>,
    my_antenna: Option<String>,
    tx_pwr: Option<String>,
    fields: BTreeMap<String, String>,
    archived_at: Option<String>,
    created_at: String,
    updated_at: String,
}

impl From<Instrument> for InstrumentBody {
    fn from(instrument: Instrument) -> InstrumentBody {
        InstrumentBody {
            id: instrument.id,
            name: instrument.name,
            my_rig: instrument.my_rig,
            my_antenna: instrument.my_antenna,
            tx_pwr: instrument.tx_pwr,
            fields: instrument.fields,
            archived_at: instrument.archived_at,
            created_at: instrument.created_at,
            updated_at: instrument.updated_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct InstrumentRequest {
    name: String,
    my_rig: Option<String>,
    my_antenna: Option<String>,
    tx_pwr: Option<String>,
    #[serde(default)]
    fields: BTreeMap<String, String>,
}

impl From<InstrumentRequest> for InstrumentInput {
    fn from(req: InstrumentRequest) -> InstrumentInput {
        InstrumentInput {
            name: req.name,
            my_rig: req.my_rig,
            my_antenna: req.my_antenna,
            tx_pwr: req.tx_pwr,
            fields: req.fields,
        }
    }
}

pub async fn list(State(state): State<Arc<AppState>>, member: Member) -> Result<Json<Vec<InstrumentBody>>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let instruments = instrument::list(&mut conn, &member.access).await?;
    Ok(Json(instruments.into_iter().map(InstrumentBody::from).collect()))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(item): Path<Item>,
) -> Result<Json<InstrumentBody>, ApiError> {
    let mut conn = state.db.acquire().await?;
    Ok(Json(instrument::get(&mut conn, &member.access, &item.id).await?.into()))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    member: Member,
    Json(req): Json<InstrumentRequest>,
) -> Result<(StatusCode, Json<InstrumentBody>), ApiError> {
    let mut tx = WriteTx::begin(&state.db).await?;
    let created = instrument::create(&mut tx, &member.access, &req.into()).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(created.into())))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(item): Path<Item>,
    Json(req): Json<InstrumentRequest>,
) -> Result<Json<InstrumentBody>, ApiError> {
    let mut tx = WriteTx::begin(&state.db).await?;
    let updated = instrument::update(&mut tx, &member.access, &item.id, &req.into()).await?;
    tx.commit().await?;
    Ok(Json(updated.into()))
}

pub async fn archive(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(item): Path<Item>,
) -> Result<Json<InstrumentBody>, ApiError> {
    set_archived(&state, &member, &item.id, true).await
}

pub async fn unarchive(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(item): Path<Item>,
) -> Result<Json<InstrumentBody>, ApiError> {
    set_archived(&state, &member, &item.id, false).await
}

async fn set_archived(
    state: &AppState,
    member: &Member,
    id: &str,
    archived: bool,
) -> Result<Json<InstrumentBody>, ApiError> {
    let mut tx = WriteTx::begin(&state.db).await?;
    let instrument = instrument::set_archived(&mut tx, &member.access, id, archived).await?;
    tx.commit().await?;
    Ok(Json(instrument.into()))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::web::testing::{Auth, Harness};

    fn rig() -> Value {
        json!({
            "name": "IC-705",
            "my_rig": "IC-705",
            "my_antenna": "EFHW",
            "tx_pwr": "5",
            "fields": { "my_sig": "POTA" },
        })
    }

    #[tokio::test]
    async fn members_manage_instruments() {
        let h = Harness::new().await;
        let instruments = h.path("/instruments");
        let owner = h.session(&h.world.owner).await;
        let operator = h.session(&h.world.operator).await;
        let outsider = h.session(&h.world.outsider).await;

        let (status, _) = h
            .send("POST", &instruments, Auth::Session(&outsider), Some(rig()))
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, created) = h
            .send("POST", &instruments, Auth::Session(&operator), Some(rig()))
            .await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        assert_eq!(created["tx_pwr"], "5");
        assert_eq!(created["fields"], json!({ "MY_SIG": "POTA" }));
        let uri = format!("{instruments}/{}", created["id"].as_str().unwrap());

        let mut qrp = rig();
        qrp["tx_pwr"] = json!("0.5");
        qrp["my_antenna"] = Value::Null;
        let (status, updated) = h.send("PUT", &uri, Auth::Session(&owner), Some(qrp)).await;
        assert_eq!(status, StatusCode::OK, "{updated}");
        assert_eq!(
            (&updated["tx_pwr"], &updated["my_antenna"]),
            (&json!("0.5"), &Value::Null)
        );

        let mut bad = rig();
        bad["tx_pwr"] = json!("QRP");
        let (status, _) = h.send("PUT", &uri, Auth::Session(&operator), Some(bad)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, archived) = h
            .send("POST", &format!("{uri}/archive"), Auth::Session(&operator), None)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert!(archived["archived_at"].is_string());
        let (_, restored) = h
            .send("DELETE", &format!("{uri}/archive"), Auth::Session(&operator), None)
            .await;
        assert_eq!(restored["archived_at"], Value::Null);

        let (status, list) = h.send("GET", &instruments, Auth::Session(&owner), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(list, json!([restored]));
        let (status, fetched) = h.send("GET", &uri, Auth::Session(&operator), None).await;
        assert_eq!((status, fetched), (StatusCode::OK, restored));
    }
}
