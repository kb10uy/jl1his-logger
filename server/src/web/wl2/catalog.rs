use std::sync::Arc;

use axum::{Json, extract::State};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    store::qso,
    web::{
        AppState,
        wl2::{Wl2Access, Wl2Error, Wl2Query, data, present},
    },
};

const TOPICS: &[&str] = &["contest", "subdivisions"];

#[derive(Debug, Deserialize)]
pub struct CatalogQuery {
    topic: Option<String>,
    dxcc: Option<String>,
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Wl2Access(access): Wl2Access,
    Wl2Query(query): Wl2Query<CatalogQuery>,
) -> Result<Json<Value>, Wl2Error> {
    match present(&query.topic) {
        None => Ok(data(json!({ "topics": TOPICS }), json!({}))),
        Some("contest") => {
            let mut conn = state.db.acquire().await?;
            let contests = qso::contest_ids(&mut conn, &access)
                .await?
                .into_iter()
                .map(|contest| json!({ "contest": contest, "name": null }))
                .collect();
            Ok(data(Value::Array(contests), json!({ "topic": "contest" })))
        }
        Some("subdivisions") => {
            let dxcc: u32 = query
                .dxcc
                .as_deref()
                .and_then(|d| d.trim().parse().ok())
                .ok_or_else(|| Wl2Error::validation("dxcc is required as a DXCC entity code"))?;
            let subdivisions = state
                .references
                .subdivisions(&state.db, &state.config.reference)
                .await?
                .ok_or_else(|| Wl2Error::not_found("no subdivisions are installed"))?;
            let names = subdivisions
                .get(&dxcc)
                .into_iter()
                .flatten()
                .map(|(state, name)| json!({ "dxcc": dxcc, "state": state, "name": name }))
                .collect();
            Ok(data(
                Value::Array(names),
                json!({ "topic": "subdivisions", "dxcc": dxcc }),
            ))
        }
        Some(topic) => Err(
            Wl2Error::validation(format!("the catalog topic {topic:?} is not available"))
                .with_details(json!({ "allowed": TOPICS })),
        ),
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;

    use crate::{
        store::{reference::ReferenceKind, token::TokenKind},
        web::{
            testing::{Auth, Harness},
            wl2::tests::Fixture,
        },
    };

    #[tokio::test]
    async fn lists_the_contest_ids_of_live_qsos() {
        let h = Harness::new().await;
        let session = h.session(&h.world.operator).await;
        let station_id = h.station.id.clone();
        for (time_on, extra) in [
            ("010000", &[("CONTEST_ID", "JA-ALL")][..]),
            ("020000", &[("CONTEST_ID", " ja-all")]),
            ("030000", &[("CONTEST_ID", "ACAG")]),
            ("040000", &[]),
        ] {
            h.create_qso(&session, &station_id, "JA1XYZ", time_on, extra).await;
        }
        let deleted = h
            .create_qso(&session, &station_id, "JA1XYZ", "050000", &[("CONTEST_ID", "JIDX-CW")])
            .await;
        h.delete_qso(&session, deleted["qso"]["id"].as_str().unwrap(), 1).await;
        let token = h.token(&h.world.operator, TokenKind::Wl2, &[&h.world.logbook.id]).await;

        let (status, body) = h
            .send(
                "GET",
                "/wl2/index.php/api/v2/catalog?topic=contest",
                Auth::Bearer(&token),
                None,
            )
            .await;

        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body,
            json!({
                "data": [{ "contest": "ACAG", "name": null }, { "contest": "JA-ALL", "name": null }],
                "meta": { "topic": "contest" },
            })
        );

        let (status, body) = h
            .send("GET", "/wl2/index.php/api/v2/catalog", Auth::Bearer(&token), None)
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["data"], json!({ "topics": ["contest", "subdivisions"] }));

        let (status, body) = h
            .send(
                "GET",
                "/wl2/index.php/api/v2/catalog?topic=mode",
                Auth::Bearer(&token),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["error"]["code"], "validation_error");
        assert_eq!(
            body["error"]["details"],
            json!({ "allowed": ["contest", "subdivisions"] })
        );

        for uri in [
            "/wl2/index.php/api/v2/catalog?topic=subdivisions",
            "/wl2/index.php/api/v2/catalog?topic=subdivisions&dxcc=Japan",
        ] {
            let (status, body) = h.send("GET", uri, Auth::Bearer(&token), None).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
            assert_eq!(body["error"]["code"], "validation_error", "{uri}");
        }
    }

    #[tokio::test]
    async fn lists_the_installed_subdivisions_of_an_entity() {
        let f = Fixture::new().await;
        let uri = "/wl2/index.php/api/v2/catalog?topic=subdivisions&dxcc=339";
        let (status, body) = f.get(uri).await;
        assert_eq!(
            (status, &body["error"]["code"]),
            (StatusCode::NOT_FOUND, &json!("not_found"))
        );

        f.install(
            ReferenceKind::Subdivisions,
            "[339]\n13 = \"Tokyo\"\n01 = \"Hokkaido\"\n[291]\nCA = \"California\"\n",
            "2026-10-23",
        )
        .await;
        let (status, body) = f.get(uri).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body,
            json!({
                "data": [
                    { "dxcc": 339, "state": "01", "name": "Hokkaido" },
                    { "dxcc": 339, "state": "13", "name": "Tokyo" },
                ],
                "meta": { "topic": "subdivisions", "dxcc": 339 },
            })
        );
        let (_, none) = f.get("/wl2/index.php/api/v2/catalog?topic=subdivisions&dxcc=1").await;
        assert_eq!(none["data"], json!([]));
    }
}
