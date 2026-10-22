use std::{collections::BTreeSet, sync::Arc};

use axum::{Json, extract::State};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    domain::reference::dxcc_code,
    store::{
        qso::{self, CallMatch, Filter},
        station,
    },
    web::{
        AppState,
        wl2::{Wl2Access, Wl2Error, Wl2Query, data, mapped},
    },
};

const MAX_PREVIOUS: u32 = 1000;

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

    let paths = &state.config.reference;
    let cty = state.references.cty(&state.db, paths).await?;
    let codes = state.references.dxcc(&state.db, paths).await?;
    let found = cty.as_deref().and_then(|cty| cty.database.lookup(&callsign));

    let mut conn = state.db.acquire().await?;
    let stations: Option<BTreeSet<String>> = match query.station_ids.as_deref().filter(|s| !s.trim().is_empty()) {
        None => None,
        Some(list) => {
            let stations = station::list(&mut conn, &access).await?;
            let mut ids = BTreeSet::new();
            for wavelog_id in list.split(',') {
                ids.insert(mapped(&stations, wavelog_id)?.id.clone());
            }
            Some(ids)
        }
    };
    let filter = Filter {
        call: Some(CallMatch::Base(callsign.clone())),
        ..Filter::default()
    };
    let previous: Vec<_> = qso::search(&mut conn, &access, &filter, None, MAX_PREVIOUS)
        .await?
        .rows
        .into_iter()
        .filter(|row| stations.as_ref().is_none_or(|ids| ids.contains(&row.station_id)))
        .collect();
    let latest = |name: &str| {
        previous
            .iter()
            .find_map(|row| row.qso.get(name).map(str::trim).filter(|v| !v.is_empty()))
    };

    Ok(data(
        json!({
            "callsign": callsign,
            "dxcc": found.map(|f| f.entity.name.clone()),
            "dxcc_id": found.and_then(|f| codes.as_deref().and_then(|codes| dxcc_code(codes, &f))),
            "dxcc_lat": found.map(|f| f.location.latitude),
            "dxcc_long": found.map(|f| f.location.longitude),
            "dxcc_cqz": found.map(|f| f.location.cq_zone),
            "dxcc_ituz": found.map(|f| f.location.itu_zone),
            "cont": found.map(|f| f.location.continent.abbreviation()),
            "name": latest("NAME"),
            "gridsquare": latest("GRIDSQUARE"),
            "location": latest("QTH"),
            "state": latest("STATE"),
            "us_county": latest("CNTY"),
            "iota_ref": latest("IOTA"),
            "qsl_manager": latest("QSL_VIA"),
            "workedBefore": !previous.is_empty(),
        }),
        json!({ "resource": "lookup", "detail": "basic" }),
    ))
}

#[cfg(test)]
pub(in crate::web::wl2) mod tests {
    use std::{collections::BTreeMap, fs};

    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use crate::{
        config::{Config, ReferenceConfig},
        domain::reference::tests::CTY,
        store::{
            WriteTx,
            reference::{self, ReferenceKind},
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
        dir: TempDir,
    }

    impl Fixture {
        pub(in crate::web::wl2) async fn new() -> Fixture {
            let dir = tempfile::tempdir().unwrap();
            let config = Config {
                reference: ReferenceConfig {
                    cty: dir.path().join("cty.dat"),
                    dxcc: dir.path().join("dxcc.toml"),
                    subdivisions: dir.path().join("subdivisions.toml"),
                    parks: dir.path().join("parks.toml"),
                },
                ..Config::default()
            };
            let h = Harness::with_config(config).await;
            let token = h.token(&h.world.operator, TokenKind::Wl2, &[&h.world.logbook.id]).await;
            Fixture { h, token, dir }
        }

        pub(in crate::web::wl2) async fn install(&self, kind: ReferenceKind, file: &str, text: &str, version: &str) {
            fs::write(self.dir.path().join(file), text).unwrap();
            self.record(kind, version, text).await;
        }

        async fn record(&self, kind: ReferenceKind, version: &str, text: &str) {
            let admin = self.h.world.admin_access().await;
            let mut tx = WriteTx::begin(&self.h.world.db).await.unwrap();
            let checksum = Sha256::digest(text);
            reference::set(&mut tx, &admin, kind, version, Some(checksum.as_slice()))
                .await
                .unwrap();
            tx.commit().await.unwrap();
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
        f.install(ReferenceKind::Cty, "cty.dat", CTY, "20260915").await;
        f.install(ReferenceKind::Dxcc, "dxcc.toml", "JA = 339\n", "1").await;
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
        assert_eq!(status, StatusCode::NOT_FOUND);
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

        f.install(ReferenceKind::Cty, "cty.dat", CTY, "20260915").await;
        let (_, first) = f.get("/wl2/api/v2/lookup?callsign=JA1XYZ").await;
        assert_eq!(first["data"]["dxcc"], "Japan");

        let renamed = CTY.replace("Japan:", "Nippon:");
        f.install(ReferenceKind::Cty, "cty.dat", &renamed, "20261001").await;
        let (_, second) = f.get("/wl2/api/v2/lookup?callsign=JA1XYZ").await;
        assert_eq!(second["data"]["dxcc"], "Nippon");

        fs::write(f.dir.path().join("cty.dat"), CTY).unwrap();
        let (_, cached) = f.get("/wl2/api/v2/lookup?callsign=JA1XYZ").await;
        assert_eq!(cached["data"]["dxcc"], "Nippon");
        f.record(ReferenceKind::Cty, "20261015", &renamed).await;
        let (status, body) = f.get("/wl2/api/v2/lookup?callsign=JA1XYZ").await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
        assert_eq!(body["error"]["code"], "internal_error");
    }
}
