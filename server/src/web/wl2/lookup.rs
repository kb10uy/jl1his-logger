use std::sync::Arc;

use axum::{Json, extract::State};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    store::{qso, station},
    web::{
        AppState,
        wl2::{Wl2Access, Wl2Error, Wl2Query, data, mapped},
    },
};

#[derive(Debug, Deserialize)]
pub struct LookupQuery {
    callsign: Option<String>,
    detail: Option<String>,
    station_ids: Option<String>,
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Wl2Access(access): Wl2Access,
    Wl2Query(query): Wl2Query<LookupQuery>,
) -> Result<Json<Value>, Wl2Error> {
    let callsign = query
        .callsign
        .as_deref()
        .map(|c| c.trim().to_ascii_uppercase())
        .filter(|c| !c.is_empty())
        .ok_or_else(|| Wl2Error::validation("callsign is required"))?;
    if query
        .detail
        .as_deref()
        .is_some_and(|d| !d.trim().is_empty() && d.trim() != "basic")
    {
        return Err(Wl2Error::validation("only detail=basic is supported"));
    }

    let mut conn = state.db.acquire().await?;
    let station_ids = match query.station_ids.as_deref().filter(|s| !s.trim().is_empty()) {
        None => None,
        Some(list) => {
            let stations = station::list(&mut conn, &access).await?;
            let mut ids = vec![];
            for wavelog_id in list.split(',') {
                ids.push(mapped(&stations, wavelog_id)?.id.clone());
            }
            Some(ids)
        }
    };
    let previous = qso::previous(&mut conn, &access, &callsign, station_ids).await?;
    drop(conn);
    let entity = state
        .references
        .entity(&state.db, &state.config.reference, &callsign)
        .await?;
    let entity = entity.as_ref();
    let latest = |name: &str| previous.fields.get(name);

    Ok(data(
        json!({
            "callsign": callsign,
            "dxcc": entity.map(|e| &e.name),
            "dxcc_id": entity.and_then(|e| e.dxcc),
            "dxcc_lat": entity.map(|e| e.latitude),
            "dxcc_long": entity.map(|e| e.longitude),
            "dxcc_cqz": entity.map(|e| e.cq_zone),
            "dxcc_ituz": entity.map(|e| e.itu_zone),
            "cont": entity.map(|e| e.continent),
            "name": latest("NAME"),
            "gridsquare": latest("GRIDSQUARE"),
            "location": latest("QTH"),
            "state": latest("STATE"),
            "us_county": latest("CNTY"),
            "iota_ref": latest("IOTA"),
            "qsl_manager": latest("QSL_VIA"),
            "workedBefore": previous.count > 0,
        }),
        json!({ "resource": "lookup", "detail": "basic" }),
    ))
}

#[cfg(test)]
pub(in crate::web::wl2) mod tests {
    use std::{collections::BTreeMap, fs};

    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use tempfile::TempDir;

    use crate::{
        domain::{qso::Qso, reference::tests::CTY},
        store::{
            WriteTx,
            qso::{self, QsoInput},
            reference::ReferenceKind,
            station::{self, StationInput},
            token::TokenKind,
        },
        web::{
            testing::{Auth, Harness},
            wl2::qso::tests::create_qso,
        },
    };

    pub(in crate::web::wl2) struct Fixture {
        pub(in crate::web::wl2) h: Harness,
        token: String,
        _dir: TempDir,
    }

    impl Fixture {
        pub(in crate::web::wl2) async fn new() -> Fixture {
            let (h, _dir) = Harness::with_references().await;
            let token = h.token(&h.world.operator, TokenKind::Wl2, &[&h.world.logbook.id]).await;
            Fixture { h, token, _dir }
        }

        pub(in crate::web::wl2) async fn install(&self, kind: ReferenceKind, text: &str, version: &str) {
            self.h.install_reference(kind, text, version).await;
        }

        pub(in crate::web::wl2) async fn get(&self, uri: &str) -> (StatusCode, Value) {
            self.h.send("GET", uri, Auth::Bearer(&self.token), None).await
        }
    }

    async fn worked(f: &Fixture) {
        let owner = f.h.world.access(&f.h.world.owner).await;
        let mut tx = WriteTx::begin(&f.h.world.db).await.unwrap();
        let mapped = station::create(
            &mut tx,
            &owner.owner().unwrap(),
            &StationInput {
                name: "Portable".into(),
                callsign: "JL1HIS/1".into(),
                wavelog_id: Some(3),
                fields: BTreeMap::new(),
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let session = f.h.session(&f.h.world.operator).await;
        let home = f.h.station.id.clone();
        create_qso(
            &f.h,
            &session,
            &home,
            "JA1XYZ/P",
            "030000",
            &[("NAME", "Hanako"), ("QTH", "Chiba"), ("QSL_VIA", "JA1BURO")],
        )
        .await;
        create_qso(
            &f.h,
            &session,
            &mapped.id,
            "ja1xyz",
            "010000",
            &[("NAME", "Taro"), ("GRIDSQUARE", "PM95"), ("CNTY", "100110")],
        )
        .await;
    }

    #[tokio::test]
    async fn answers_the_entity_and_the_latest_own_contacts() {
        let f = Fixture::new().await;
        f.install(ReferenceKind::Cty, CTY, "20260915").await;
        f.install(ReferenceKind::Dxcc, "JA = 339\n", "1").await;
        worked(&f).await;

        let (status, body) = f.get("/wl2/index.php/api/v2/lookup?callsign=ja1xyz&detail=basic").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body,
            json!({
                "data": {
                    "callsign": "JA1XYZ", "dxcc": "Japan", "dxcc_id": 339, "dxcc_lat": 36.4, "dxcc_long": 138.38,
                    "dxcc_cqz": 25, "dxcc_ituz": 45, "cont": "AS", "name": "Hanako", "gridsquare": "PM95",
                    "location": "Chiba", "state": null, "us_county": "100110", "iota_ref": null,
                    "qsl_manager": "JA1BURO", "workedBefore": true
                },
                "meta": { "resource": "lookup", "detail": "basic" }
            })
        );

        let (_, alias) = f.get("/wl2/api/v2/lookup?callsign=JA1XYZ&station_ids=3").await;
        assert_eq!(
            (&alias["data"]["name"], &alias["data"]["location"]),
            (&json!("Taro"), &Value::Null)
        );

        let (_, unknown) = f.get("/wl2/api/v2/lookup?callsign=XX0XX").await;
        assert_eq!(
            (
                &unknown["data"]["dxcc"],
                &unknown["data"]["dxcc_id"],
                &unknown["data"]["workedBefore"]
            ),
            (&Value::Null, &Value::Null, &json!(false))
        );

        for uri in ["/wl2/api/v2/lookup", "/wl2/api/v2/lookup?callsign=JA1XYZ&detail=full"] {
            let (status, body) = f.get(uri).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
        }
        let (status, _) = f.get("/wl2/api/v2/lookup?callsign=JA1XYZ&station_ids=7").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn finds_older_fields_and_scopes_history_before_paging() {
        let f = Fixture::new().await;
        worked(&f).await;
        let owner = f.h.world.access(&f.h.world.owner).await;
        let mut tx = WriteTx::begin(&f.h.world.db).await.unwrap();
        station::update(
            &mut tx,
            &owner.owner().unwrap(),
            &f.h.station.id,
            &StationInput {
                name: f.h.station.name.clone(),
                callsign: f.h.station.callsign.clone(),
                wavelog_id: Some(4),
                fields: f.h.station.fields.clone(),
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let access = f.h.world.access(&f.h.world.operator).await;
        let mut tx = WriteTx::begin(&f.h.world.db).await.unwrap();
        let start = chrono::NaiveDate::from_ymd_opt(2026, 10, 23)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        for i in 0..1001 {
            let time = start + chrono::TimeDelta::minutes(i * 16);
            let qso = Qso::from_fields([
                ("CALL", "JA1XYZ".to_owned()),
                ("QSO_DATE", time.format("%Y%m%d").to_string()),
                ("TIME_ON", time.format("%H%M%S").to_string()),
                ("BAND", "40m".to_owned()),
                ("MODE", "CW".to_owned()),
                ("NAME", " ".to_owned()),
            ])
            .unwrap();
            let id = uuid::Uuid::now_v7().to_string().parse().unwrap();
            qso::create(
                &mut tx,
                &access,
                &id,
                QsoInput {
                    station_id: f.h.station.id.clone(),
                    qso,
                },
            )
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();

        for stations in ["", "&station_ids=3,4"] {
            let (status, body) = f.get(&format!("/wl2/api/v2/lookup?callsign=JA1XYZ{stations}")).await;
            assert_eq!(status, StatusCode::OK, "{body}");
            assert_eq!(body["data"]["workedBefore"], true);
            assert_eq!(body["data"]["name"], "Hanako");
            assert_eq!(body["data"]["gridsquare"], "PM95");
            assert_eq!(body["data"]["location"], "Chiba");
        }
        let (status, body) = f.get("/wl2/api/v2/lookup?callsign=JA1XYZ&station_ids=3").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["data"]["workedBefore"], true);
        assert_eq!(body["data"]["name"], "Taro");
        assert_eq!(body["data"]["location"], Value::Null);
        let (_, home) = f.get("/wl2/api/v2/lookup?callsign=JA1XYZ&station_ids=4").await;
        assert_eq!(home["data"]["name"], "Hanako");
        assert_eq!(home["data"]["gridsquare"], Value::Null);
        let (_, unknown) = f.get("/wl2/api/v2/lookup?callsign=XX0XX&station_ids=3,4").await;
        assert_eq!(unknown["data"]["workedBefore"], false);
    }

    #[tokio::test]
    async fn follows_the_installed_version_of_the_reference_data() {
        let f = Fixture::new().await;
        worked(&f).await;
        let (status, bare) = f.get("/wl2/api/v2/lookup?callsign=JA1XYZ").await;
        assert_eq!(status, StatusCode::OK, "{bare}");
        assert_eq!(
            (&bare["data"]["dxcc"], &bare["data"]["name"]),
            (&Value::Null, &json!("Hanako"))
        );

        f.install(ReferenceKind::Cty, CTY, "20260915").await;
        let (_, first) = f.get("/wl2/api/v2/lookup?callsign=JA1XYZ").await;
        assert_eq!(first["data"]["dxcc"], "Japan");

        let renamed = CTY.replace("Japan:", "Nippon:");
        f.install(ReferenceKind::Cty, &renamed, "20261001").await;
        let (_, second) = f.get("/wl2/api/v2/lookup?callsign=JA1XYZ").await;
        assert_eq!(second["data"]["dxcc"], "Nippon");

        fs::write(f.h.reference_path(ReferenceKind::Cty), CTY).unwrap();
        let (_, cached) = f.get("/wl2/api/v2/lookup?callsign=JA1XYZ").await;
        assert_eq!(cached["data"]["dxcc"], "Nippon");
        f.h.record_reference(ReferenceKind::Cty, "20261015", &renamed).await;
        let (status, body) = f.get("/wl2/api/v2/lookup?callsign=JA1XYZ").await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
        assert_eq!(body["error"]["code"], "internal_error");
    }
}
