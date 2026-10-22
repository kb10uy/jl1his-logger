use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};
use chrono::{Days, NaiveDate};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    domain::{adif::LengthUnit, qso::Qso},
    store::{
        qso::{self, CallMatch, Confirmation, Filter, InsertionQuery, TimeRange},
        station,
    },
    web::{
        AppState,
        export::adif_document,
        wl2::{Wl2Access, Wl2Error, Wl2Query, data},
    },
};

const DEFAULT_PER_PAGE: u32 = 100;
const MAX_PER_PAGE: u32 = 5000;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    format: Option<String>,
    station_id: Option<String>,
    since_id: Option<String>,
    page: Option<String>,
    per_page: Option<String>,
    callsign: Option<String>,
    callsign_match: Option<String>,
    band: Option<String>,
    mode: Option<String>,
    qsl_filter: Option<String>,
    qso_since: Option<String>,
    qso_until: Option<String>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    Wl2Access(access): Wl2Access,
    Wl2Query(query): Wl2Query<ListQuery>,
) -> Result<Json<Value>, Wl2Error> {
    if !query
        .format
        .as_deref()
        .is_some_and(|f| f.trim().eq_ignore_ascii_case("adif"))
    {
        return Err(Wl2Error::validation("only format=adif is supported"));
    }
    let since_id = integer("since_id", query.since_id.as_deref())?.unwrap_or(0);
    let page = integer("page", query.page.as_deref())?.unwrap_or(1);
    let per_page = integer("per_page", query.per_page.as_deref())?.map_or(DEFAULT_PER_PAGE, |n| {
        u32::try_from(n).unwrap_or(MAX_PER_PAGE).min(MAX_PER_PAGE)
    });
    if page == 0 || per_page == 0 {
        return Err(Wl2Error::validation("page and per_page start at 1"));
    }
    let call = match (present(&query.callsign), present(&query.callsign_match)) {
        (None, _) => None,
        (Some(call), None | Some("exact")) => Some(CallMatch::Exact(call.to_owned())),
        (Some(call), Some("plain")) => Some(CallMatch::Base(call.to_owned())),
        (Some(_), Some(other)) => {
            return Err(Wl2Error::validation(format!(
                "callsign_match {other:?} is not supported"
            )));
        }
    };
    let time = TimeRange {
        from: date("qso_since", present(&query.qso_since), 0)?,
        until: date("qso_until", present(&query.qso_until), 1)?,
    };

    let mut conn = state.db.acquire().await?;
    let station_ids = match present(&query.station_id) {
        None => None,
        Some(list) => {
            let stations = station::list(&mut conn, &access).await?;
            let mut ids = vec![];
            for wavelog_id in list.split(',').map(str::trim) {
                let station = stations
                    .iter()
                    .find(|s| s.wavelog_id.is_some_and(|id| id.to_string() == wavelog_id))
                    .ok_or_else(|| {
                        Wl2Error::new(
                            StatusCode::NOT_FOUND,
                            "not_found",
                            format!("Station {wavelog_id:?} not found"),
                        )
                    })?;
                ids.push(station.id.clone());
            }
            Some(ids)
        }
    };
    let confirmed_by = match present(&query.qsl_filter) {
        None => vec![],
        Some(list) => list
            .split(',')
            .map(|kind| match kind.trim().to_ascii_lowercase().as_str() {
                "qsl" => Ok(Confirmation::Card),
                "lotw" => Ok(Confirmation::Lotw),
                "eqsl" => Ok(Confirmation::Eqsl),
                other => Err(Wl2Error::validation(format!(
                    "qsl_filter {other:?} is not recorded here"
                ))),
            })
            .collect::<Result<_, _>>()?,
    };
    let insertion = InsertionQuery {
        filter: Filter {
            time,
            band: present(&query.band).map(str::to_owned),
            mode: present(&query.mode).map(str::to_owned),
            call,
            ..Filter::default()
        },
        station_ids,
        confirmed_by,
        after_seq: i64::try_from(since_id).map_err(|_| Wl2Error::validation("since_id is out of range"))?,
    };
    let offset = (page - 1).saturating_mul(u64::from(per_page));
    let found = qso::by_insertion(&mut conn, &access, &insertion, offset, per_page).await?;

    let last = found.rows.last().map_or(insertion.after_seq, |row| row.seq);
    let records: Vec<Qso> = found.rows.into_iter().map(|row| row.qso).collect();
    let total_pages = found.total.div_ceil(u64::from(per_page));
    Ok(data(
        json!({
            "exported": records.len(),
            "lastfetchedid": last,
            "adif": adif_document(&records, LengthUnit::Codepoints),
        }),
        json!({
            "page": page,
            "per_page": per_page,
            "count": records.len(),
            "total": found.total,
            "total_pages": total_pages,
            "has_more": page < total_pages,
        }),
    ))
}

fn present(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

fn integer(name: &str, value: Option<&str>) -> Result<Option<u64>, Wl2Error> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(v) => v
            .parse()
            .map(Some)
            .map_err(|_| Wl2Error::validation(format!("{name} {v:?} is not a non-negative integer"))),
    }
}

fn date(name: &str, value: Option<&str>, add_days: u64) -> Result<Option<i64>, Wl2Error> {
    let Some(value) = value else {
        return Ok(None);
    };
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.checked_add_days(Days::new(add_days)))
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|t| Some(t.and_utc().timestamp()))
        .ok_or_else(|| Wl2Error::validation(format!("{name} {value:?} is not a YYYY-MM-DD date")))
}

#[cfg(test)]
pub(super) mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use rstest::rstest;
    use serde_json::{Value, json};

    use crate::{
        domain::adif::{LengthUnit, read_adi},
        store::{
            WriteTx,
            station::{self, StationInput},
            token::TokenKind,
        },
        web::testing::{Auth, Harness},
    };

    pub(in crate::web::wl2) async fn create_qso(
        h: &Harness,
        session: &str,
        station_id: &str,
        call: &str,
        time_on: &str,
        extra: &[(&str, &str)],
    ) -> Value {
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
        body
    }

    struct Fixture {
        h: Harness,
        token: String,
    }

    async fn fixture() -> Fixture {
        let h = Harness::new().await;
        let owner = h.world.access(&h.world.owner).await;
        let mut tx = WriteTx::begin(&h.world.db).await.unwrap();
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
        let session = h.session(&h.world.operator).await;
        for (station_id, call, time_on, extra) in [
            (&mapped.id, "JA1AAA", "010000", vec![("QSL_RCVD", "Y")]),
            (&mapped.id, "JA1AAA/P", "020000", vec![("BAND", "20m"), ("MODE", "FT8")]),
            (&h.station.id, "JA1BBB", "030000", vec![("LOTW_QSL_RCVD", "Y")]),
            (
                &mapped.id,
                "JA1CCC",
                "000000",
                vec![("QSO_DATE", "20261023"), ("NAME", "太郎")],
            ),
            (&mapped.id, "JA1DDD", "040000", vec![]),
        ] {
            create_qso(&h, &session, station_id, call, time_on, &extra).await;
        }
        let deleted = create_qso(&h, &session, &mapped.id, "JA1EEE", "050000", &[]).await;
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
        Fixture { h, token }
    }

    impl Fixture {
        async fn get(&self, query: &str) -> (StatusCode, Value) {
            let uri = format!("/wl2/index.php/api/v2/qso?{query}");
            self.h.send("GET", &uri, Auth::Bearer(&self.token), None).await
        }

        async fn calls(&self, query: &str) -> Vec<String> {
            let (status, body) = self.get(&format!("format=adif&{query}")).await;
            assert_eq!(status, StatusCode::OK, "{query}: {body}");
            calls(&body)
        }
    }

    fn calls(body: &Value) -> Vec<String> {
        read_adi(body["data"]["adif"].as_str().unwrap(), LengthUnit::Codepoints)
            .unwrap()
            .into_iter()
            .map(|qso| qso.unwrap().get("CALL").unwrap().to_owned())
            .collect()
    }

    #[tokio::test]
    async fn pages_through_the_insertion_order_after_an_id() {
        let f = fixture().await;

        let (status, first) = f.get("format=adif&station_id=3&since_id=0&per_page=2").await;
        assert_eq!(status, StatusCode::OK, "{first}");
        assert_eq!(calls(&first), ["JA1AAA", "JA1AAA/P"]);
        assert_eq!(
            (&first["data"]["exported"], &first["data"]["lastfetchedid"]),
            (&json!(2), &json!(2))
        );
        assert_eq!(
            first["meta"],
            json!({ "page": 1, "per_page": 2, "count": 2, "total": 4, "total_pages": 2, "has_more": true })
        );

        let (_, next) = f.get("format=adif&station_id=3&since_id=2&per_page=2").await;
        assert_eq!(calls(&next), ["JA1CCC", "JA1DDD"]);
        assert!(next["data"]["adif"].as_str().unwrap().contains("<NAME:2>太郎"));
        assert_eq!(next["data"]["lastfetchedid"], 5);
        assert_eq!(next["meta"]["has_more"], false);

        let (_, last_page) = f.get("format=adif&station_id=3&page=2&per_page=2").await;
        assert_eq!(calls(&last_page), ["JA1CCC", "JA1DDD"]);
        assert_eq!(last_page["meta"]["total_pages"], 2);

        let (_, empty) = f.get("format=adif&station_id=3&since_id=5").await;
        assert_eq!(calls(&empty), Vec::<String>::new());
        assert_eq!(
            (&empty["data"]["exported"], &empty["data"]["lastfetchedid"]),
            (&json!(0), &json!(5))
        );
        assert_eq!(
            empty["meta"],
            json!({ "page": 1, "per_page": 100, "count": 0, "total": 0, "total_pages": 0, "has_more": false })
        );
    }

    #[rstest]
    #[case("", &["JA1AAA", "JA1AAA/P", "JA1BBB", "JA1CCC", "JA1DDD"])]
    #[case("station_id=3", &["JA1AAA", "JA1AAA/P", "JA1CCC", "JA1DDD"])]
    #[case("callsign=ja1aaa", &["JA1AAA"])]
    #[case("callsign=JA1AAA&callsign_match=plain", &["JA1AAA", "JA1AAA/P"])]
    #[case("band=20M&mode=ft8", &["JA1AAA/P"])]
    #[case("qsl_filter=lotw,qsl", &["JA1AAA", "JA1BBB"])]
    #[case("qso_since=2026-10-23", &["JA1CCC"])]
    #[case("qso_until=2026-10-22", &["JA1AAA", "JA1AAA/P", "JA1BBB", "JA1DDD"])]
    #[case("per_page=9000", &["JA1AAA", "JA1AAA/P", "JA1BBB", "JA1CCC", "JA1DDD"])]
    #[tokio::test]
    async fn filters_the_qsos(#[case] query: &str, #[case] expected: &[&str]) {
        let f = fixture().await;
        assert_eq!(f.calls(query).await, expected);
    }

    #[rstest]
    #[case("", StatusCode::BAD_REQUEST, "validation_error")]
    #[case("format=json", StatusCode::BAD_REQUEST, "validation_error")]
    #[case("format=adif&station_id=7", StatusCode::NOT_FOUND, "not_found")]
    #[case("format=adif&since_id=-1", StatusCode::BAD_REQUEST, "validation_error")]
    #[case("format=adif&page=0", StatusCode::BAD_REQUEST, "validation_error")]
    #[case("format=adif&qsl_filter=qrz", StatusCode::BAD_REQUEST, "validation_error")]
    #[case(
        "format=adif&callsign=JA1AAA&callsign_match=fuzzy",
        StatusCode::BAD_REQUEST,
        "validation_error"
    )]
    #[case("format=adif&qso_since=20261022", StatusCode::BAD_REQUEST, "validation_error")]
    #[tokio::test]
    async fn refuses_what_it_does_not_support(#[case] query: &str, #[case] status: StatusCode, #[case] code: &str) {
        let f = fixture().await;
        let (actual, body) = f.get(query).await;
        assert_eq!((actual, body["error"]["code"].as_str()), (status, Some(code)), "{body}");
    }
}
