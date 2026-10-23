use std::{collections::BTreeMap, sync::Arc};

use axum::extract::{Path, State};
use chrono::DateTime;
use serde::{Deserialize, Serialize};

use crate::{
    domain::reference::Entity,
    store::{StoreError, access, qso, timestamp},
    web::{
        AppState,
        auth::Caller,
        error::ApiError,
        extract::{Json, Query},
    },
};

#[derive(Debug, Deserialize)]
pub struct LookupQuery {
    call: Option<String>,
    station_ids: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LookupBody {
    call: String,
    entity: Option<Entity>,
    previous: PreviousBody,
}

#[derive(Debug, Serialize)]
pub struct PreviousBody {
    count: u64,
    last_qso_at: Option<String>,
    fields: BTreeMap<&'static str, String>,
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path(logbook_id): Path<String>,
    Query(query): Query<LookupQuery>,
) -> Result<Json<LookupBody>, ApiError> {
    let call = query
        .call
        .as_deref()
        .map(|c| c.trim().to_ascii_uppercase())
        .filter(|c| !c.is_empty())
        .ok_or_else(|| StoreError::Invalid("call is required".into()))?;
    let station_ids = query
        .station_ids
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|list| list.split(',').map(|id| id.trim().to_owned()).collect());

    let mut conn = state.db.acquire().await?;
    let access = access::logbook(&mut conn, &caller.principal, &logbook_id).await?;
    let previous = qso::previous(&mut conn, &access, &call, station_ids).await?;
    drop(conn);
    let entity = state
        .references
        .entity(&state.db, &state.config.reference, &call)
        .await?;

    Ok(Json(LookupBody {
        entity,
        previous: PreviousBody {
            count: previous.count,
            last_qso_at: previous
                .last_qso_time
                .and_then(|time| DateTime::from_timestamp(time, 0))
                .map(timestamp),
            fields: previous.fields,
        },
        call,
    }))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use serde_json::{Value, json};

    use crate::{
        domain::reference::tests::CTY,
        store::{
            WriteTx,
            reference::ReferenceKind,
            station::{self, StationInput},
            token::TokenKind,
        },
        web::testing::{Auth, Harness},
    };

    async fn log(h: &Harness, session: &str, station_id: &str, call: &str, time_on: &str, extra: &[(&str, &str)]) {
        let mut fields: BTreeMap<&str, &str> = BTreeMap::from([
            ("CALL", call),
            ("QSO_DATE", "20261022"),
            ("TIME_ON", time_on),
            ("BAND", "40m"),
            ("MODE", "CW"),
        ]);
        fields.extend(extra.iter().copied());
        let body = json!({ "id": uuid::Uuid::now_v7().to_string(), "station_id": station_id, "fields": fields });
        let uri = format!("/api/logbooks/{}/qsos", h.world.logbook.id);
        let (status, body) = h.send("POST", &uri, Auth::Session(session), Some(body)).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }

    async fn portable(h: &Harness) -> String {
        let owner = h.world.access(&h.world.owner).await;
        let mut tx = WriteTx::begin(&h.world.db).await.unwrap();
        let station = station::create(
            &mut tx,
            &owner.owner().unwrap(),
            &StationInput {
                name: "Portable".into(),
                callsign: "JL1HIS/1".into(),
                wavelog_id: None,
                fields: BTreeMap::new(),
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        station.id
    }

    fn uri(h: &Harness, query: &str) -> String {
        format!("/api/logbooks/{}/lookup?{query}", h.world.logbook.id)
    }

    #[tokio::test]
    async fn answers_the_entity_and_the_previous_contacts() {
        let (h, _dir) = Harness::with_references().await;
        h.install_reference(ReferenceKind::Cty, CTY, "20260915").await;
        h.install_reference(ReferenceKind::Dxcc, "JA = 339\n", "1").await;
        let session = h.session(&h.world.operator).await;
        let portable = portable(&h).await;
        log(
            &h,
            &session,
            &h.station.id,
            "JA1XYZ/P",
            "030000",
            &[("NAME", "Hanako"), ("QTH", "Chiba")],
        )
        .await;
        log(
            &h,
            &session,
            &portable,
            "ja1xyz",
            "010000",
            &[("NAME", "Taro"), ("GRIDSQUARE", "PM95")],
        )
        .await;

        let (status, body) = h
            .send("GET", &uri(&h, "call=ja1xyz"), Auth::Session(&session), None)
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body,
            json!({
                "call": "JA1XYZ",
                "entity": {
                    "name": "Japan", "dxcc": 339, "continent": "AS", "cq_zone": 25, "itu_zone": 45,
                    "latitude": 36.4, "longitude": 138.38
                },
                "previous": {
                    "count": 2,
                    "last_qso_at": "2026-10-22T03:00:00Z",
                    "fields": { "NAME": "Hanako", "QTH": "Chiba", "GRIDSQUARE": "PM95" }
                }
            })
        );

        let narrowed = uri(&h, &format!("call=JA1XYZ&station_ids={portable}"));
        let (_, body) = h.send("GET", &narrowed, Auth::Session(&session), None).await;
        assert_eq!(
            body["previous"],
            json!({
                "count": 1,
                "last_qso_at": "2026-10-22T01:00:00Z",
                "fields": { "NAME": "Taro", "GRIDSQUARE": "PM95" }
            })
        );

        let (_, unknown) = h
            .send("GET", &uri(&h, "call=XX0XX"), Auth::Session(&session), None)
            .await;
        assert_eq!(
            (&unknown["entity"], &unknown["previous"]),
            (&Value::Null, &json!({ "count": 0, "last_qso_at": null, "fields": {} }))
        );
    }

    #[tokio::test]
    async fn answers_without_an_entity_until_cty_is_installed() {
        let (h, _dir) = Harness::with_references().await;
        h.install_reference(ReferenceKind::Dxcc, "JA = 339\n", "1").await;
        let token = h
            .token(&h.world.operator, TokenKind::Native, &[&h.world.logbook.id])
            .await;
        let (status, body) = h.send("GET", &uri(&h, "call=JA1XYZ"), Auth::Bearer(&token), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["entity"], Value::Null);

        h.install_reference(ReferenceKind::Cty, CTY, "20260915").await;
        let (_, body) = h.send("GET", &uri(&h, "call=JD1ABC"), Auth::Bearer(&token), None).await;
        assert_eq!(
            (&body["entity"]["name"], &body["entity"]["dxcc"]),
            (&json!("Ogasawara"), &Value::Null)
        );
    }

    #[tokio::test]
    async fn refuses_a_missing_call_and_outsiders() {
        let h = Harness::new().await;
        let session = h.session(&h.world.operator).await;
        for query in ["", "call=%20"] {
            let (status, body) = h.send("GET", &uri(&h, query), Auth::Session(&session), None).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {body}");
        }
        let outsider = h.session(&h.world.outsider).await;
        let (status, _) = h
            .send("GET", &uri(&h, "call=JA1XYZ"), Auth::Session(&outsider), None)
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _) = h.send("GET", &uri(&h, "call=JA1XYZ"), Auth::Anonymous, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}
