use std::sync::Arc;

use axum::{Json, extract::State};
use serde_json::{Map, Number, Value, json};

use crate::{
    store::logbook::station::{self, Station},
    web::{
        AppState,
        wl2::{Wl2Access, Wl2Error, data},
    },
};

const TEXT_FIELDS: &[(&str, &str)] = &[
    ("gridsquare", "MY_GRIDSQUARE"),
    ("city", "MY_CITY"),
    ("country", "MY_COUNTRY"),
    ("state", "MY_STATE"),
    ("cnty", "MY_CNTY"),
    ("iota", "MY_IOTA"),
    ("sota", "MY_SOTA_REF"),
    ("wwff", "MY_WWFF_REF"),
    ("pota", "MY_POTA_REF"),
    ("sig", "MY_SIG"),
    ("sig_info", "MY_SIG_INFO"),
];

const NUMBER_FIELDS: &[(&str, &str)] = &[("dxcc", "MY_DXCC"), ("cq", "MY_CQ_ZONE"), ("itu", "MY_ITU_ZONE")];

pub async fn list(State(state): State<Arc<AppState>>, Wl2Access(access): Wl2Access) -> Result<Json<Value>, Wl2Error> {
    let mut conn = state.db.acquire().await?;
    let mut stations: Vec<(i64, Station)> = station::list(&mut conn, &access)
        .await?
        .into_iter()
        .filter(|station| station.archived_at.is_none())
        .filter_map(|station| Some((station.wavelog_id?, station)))
        .collect();
    stations.sort_by_key(|(wavelog_id, _)| *wavelog_id);
    let stations = stations
        .into_iter()
        .map(|(wavelog_id, station)| body(wavelog_id, station))
        .collect();
    Ok(data(
        Value::Array(stations),
        json!({ "resource": "station", "method": "GET" }),
    ))
}

fn body(wavelog_id: i64, station: Station) -> Value {
    let mut body = Map::new();
    body.insert("id".into(), wavelog_id.into());
    body.insert("uuid".into(), station.id.into());
    body.insert("name".into(), station.name.into());
    body.insert("callsign".into(), station.callsign.into());
    let field = |name: &str| station.fields.get(name).map(|v| v.trim()).filter(|v| !v.is_empty());
    for (key, name) in TEXT_FIELDS {
        body.insert((*key).into(), field(name).map_or(Value::Null, Value::from));
    }
    for (key, name) in NUMBER_FIELDS {
        body.insert((*key).into(), field(name).map_or(Value::Null, number));
    }
    body.insert("power".into(), Value::Null);
    body.insert("active".into(), false.into());
    Value::Object(body)
}

fn number(value: &str) -> Value {
    match value.parse::<i64>() {
        Ok(n) => n.into(),
        Err(_) => value
            .parse::<f64>()
            .ok()
            .and_then(Number::from_f64)
            .map_or(Value::Null, Value::Number),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use serde_json::json;

    use crate::{
        store::{
            WriteTx,
            account::token::TokenKind,
            logbook::station::{self, StationInput},
        },
        web::testing::{Auth, Harness},
    };

    #[tokio::test]
    async fn lists_the_mapped_stations_that_are_not_archived() {
        let h = Harness::new().await;
        let owner = h.world.access(&h.world.owner).await;
        let owner = owner.owner().unwrap();
        let mut tx = WriteTx::begin(&h.world.db).await.unwrap();
        let mut created = vec![];
        for (name, wavelog_id, fields) in [
            (
                "Home",
                Some(3),
                vec![
                    ("MY_GRIDSQUARE", "PM95"),
                    ("MY_CITY", "杉並区"),
                    ("MY_DXCC", "339"),
                    ("MY_CQ_ZONE", "25"),
                    ("MY_STATE", "13"),
                    ("MY_CNTY", "100110"),
                ],
            ),
            ("Portable", Some(9), vec![("MY_POTA_REF", "JP-0001,JP-0002")]),
            ("Archived", Some(4), vec![]),
            ("New", None, vec![]),
        ] {
            let input = StationInput {
                name: name.into(),
                callsign: "JL1HIS".into(),
                wavelog_id,
                fields: fields
                    .into_iter()
                    .map(|(name, value)| (name.to_owned(), value.to_owned()))
                    .collect::<BTreeMap<_, _>>(),
            };
            created.push(station::create(&mut tx, &owner, &input).await.unwrap());
        }
        station::set_archived(&mut tx, &owner, &created[2].id, true)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let token = h.token(&h.world.operator, TokenKind::Wl2, &[&h.world.logbook.id]).await;

        let (status, body) = h
            .send("GET", "/wl2/index.php/api/v2/station", Auth::Bearer(&token), None)
            .await;

        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body["data"],
            json!([
                {
                    "id": 3, "uuid": created[0].id, "name": "Home", "callsign": "JL1HIS",
                    "gridsquare": "PM95", "city": "杉並区", "country": null, "state": "13", "cnty": "100110",
                    "iota": null, "sota": null, "wwff": null, "pota": null, "sig": null, "sig_info": null,
                    "dxcc": 339, "cq": 25, "itu": null, "power": null, "active": false
                },
                {
                    "id": 9, "uuid": created[1].id, "name": "Portable", "callsign": "JL1HIS",
                    "gridsquare": null, "city": null, "country": null, "state": null, "cnty": null,
                    "iota": null, "sota": null, "wwff": null, "pota": "JP-0001,JP-0002", "sig": null,
                    "sig_info": null, "dxcc": null, "cq": null, "itu": null, "power": null, "active": false
                },
            ])
        );
    }
}
