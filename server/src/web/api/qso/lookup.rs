use std::{collections::BTreeMap, sync::Arc};

use axum::extract::State;
use chrono::DateTime;
use serde::{Deserialize, Serialize};

use crate::{
    domain::reference::Entity,
    store::{qso, timestamp},
    web::{
        AppState,
        api::logbook::member::Member,
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

pub(in crate::web) fn call(value: Option<&str>) -> Option<String> {
    value.map(|c| c.trim().to_ascii_uppercase()).filter(|c| !c.is_empty())
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    member: Member,
    Query(query): Query<LookupQuery>,
) -> Result<Json<LookupBody>, ApiError> {
    let call = call(query.call.as_deref()).ok_or_else(|| ApiError::bad_request("call is required"))?;
    let station_ids = query
        .station_ids
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|list| list.split(',').map(|id| id.trim().to_owned()).collect());

    let mut conn = state.db.acquire().await?;
    let previous = qso::previous(&mut conn, &member.access, &call, station_ids).await?;
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
    use rstest::rstest;
    use serde_json::{Value, json};

    use super::*;
    use crate::{
        domain::reference::tests::CTY,
        store::{account::token::TokenKind, logbook::station::StationInput, reference::ReferenceKind},
        web::testing::{Auth, Harness},
    };

    fn uri(h: &Harness, query: &str) -> String {
        h.path(&format!("/lookup?{query}"))
    }

    #[rstest]
    #[case(Some(" ja1xyz/p "), Some("JA1XYZ/P"))]
    #[case(Some(" "), None)]
    #[case(None, None)]
    fn reads_the_call_to_look_up(#[case] value: Option<&str>, #[case] expected: Option<&str>) {
        assert_eq!(call(value).as_deref(), expected);
    }

    #[tokio::test]
    async fn answers_the_entity_and_the_previous_contacts() {
        let (h, _dir) = Harness::with_references().await;
        h.install_reference(ReferenceKind::Cty, CTY, "20260915").await;
        h.install_reference(ReferenceKind::Dxcc, "JA = 339\n", "1").await;
        let session = h.session(&h.world.operator).await;
        let portable = h
            .create_station(&StationInput {
                name: "Portable".into(),
                callsign: "JL1HIS/1".into(),
                wavelog_id: None,
                fields: BTreeMap::new(),
            })
            .await;
        h.create_qso(
            &session,
            &h.station.id,
            "JA1XYZ/P",
            "030000",
            &[("NAME", "Hanako"), ("QTH", "Chiba")],
        )
        .await;
        h.create_qso(
            &session,
            &portable.id,
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

        let narrowed = uri(&h, &format!("call=JA1XYZ&station_ids={}", portable.id));
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

    #[rstest]
    #[case::missing_call("operator", "", StatusCode::BAD_REQUEST)]
    #[case::blank_call("operator", "call=%20", StatusCode::BAD_REQUEST)]
    #[case::outsider("outsider", "call=JA1XYZ", StatusCode::NOT_FOUND)]
    #[case::anonymous("anonymous", "call=JA1XYZ", StatusCode::UNAUTHORIZED)]
    #[tokio::test]
    async fn refuses_a_missing_call_and_outsiders(
        #[case] who: &str,
        #[case] query: &str,
        #[case] expected: StatusCode,
    ) {
        let h = Harness::new().await;
        let session = match who {
            "operator" => Some(h.session(&h.world.operator).await),
            "outsider" => Some(h.session(&h.world.outsider).await),
            _ => None,
        };
        let auth = session.as_deref().map_or(Auth::Anonymous, Auth::Session);
        let (status, body) = h.send("GET", &uri(&h, query), auth, None).await;
        assert_eq!(status, expected, "{body}");
    }
}
