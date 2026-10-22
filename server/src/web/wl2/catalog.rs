use std::sync::Arc;

use axum::{Json, extract::State};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    store::qso,
    web::{
        AppState,
        wl2::{Wl2Access, Wl2Error, Wl2Query, data},
    },
};

#[derive(Debug, Deserialize)]
pub struct CatalogQuery {
    topic: Option<String>,
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Wl2Access(access): Wl2Access,
    Wl2Query(query): Wl2Query<CatalogQuery>,
) -> Result<Json<Value>, Wl2Error> {
    match query.topic.as_deref().map(str::trim) {
        Some("contest") => {
            let mut conn = state.db.acquire().await?;
            let contests = qso::contest_ids(&mut conn, &access)
                .await?
                .into_iter()
                .map(|contest| json!({ "contest": contest, "name": null }))
                .collect();
            Ok(data(Value::Array(contests), json!({ "topic": "contest" })))
        }
        Some(topic) => Err(Wl2Error::validation(format!(
            "the catalog topic {topic:?} is not available"
        ))),
        None => Err(Wl2Error::validation("topic is required")),
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;

    use crate::{
        store::token::TokenKind,
        web::{
            testing::{Auth, Harness},
            wl2::qso::tests::create_qso,
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
            create_qso(&h, &session, &station_id, "JA1XYZ", time_on, extra).await;
        }
        let deleted = create_qso(
            &h,
            &session,
            &station_id,
            "JA1XYZ",
            "050000",
            &[("CONTEST_ID", "JIDX-CW")],
        )
        .await;
        let uri = format!(
            "/api/logbooks/{}/qsos/{}",
            h.world.logbook.id,
            deleted["qso"]["id"].as_str().unwrap()
        );
        let (status, _, _) = h
            .send_with("DELETE", &uri, Auth::Session(&session), &[("if-match", "\"1\"")], None)
            .await;
        assert_eq!(status, StatusCode::OK);
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

        for uri in [
            "/wl2/index.php/api/v2/catalog",
            "/wl2/index.php/api/v2/catalog?topic=mode",
        ] {
            let (status, body) = h.send("GET", uri, Auth::Bearer(&token), None).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
            assert_eq!(body["error"]["code"], "validation_error", "{uri}");
        }
    }
}
