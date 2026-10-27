use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::{
    domain::{
        adif::LengthUnit,
        import::RecordIds,
        qso::Qso,
        wavelog::{STATION_PROFILE_ID, qso_from_body},
    },
    store::{
        WriteTx, export,
        logbook::station,
        qso::{self, CallMatch, Confirmation, Created, Filter, InsertionQuery, QsoInput, TimeRange},
    },
    web::{
        AppState,
        wl2::{Wl2Access, Wl2Error, Wl2Json, Wl2Query, data, mapped, present, station_ids},
    },
};

const DEFAULT_PER_PAGE: u32 = 1000;
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

#[derive(Debug, PartialEq, Eq)]
struct Listing {
    insertion: InsertionQuery,
    page: u64,
    per_page: u32,
}

fn listing(query: &ListQuery) -> Result<Listing, Wl2Error> {
    if !present(&query.format).is_some_and(|f| f.eq_ignore_ascii_case("adif")) {
        return Err(Wl2Error::validation("only format=adif is supported"));
    }
    let since_id = integer("since_id", present(&query.since_id))?.unwrap_or(0);
    let page = integer("page", present(&query.page))?.unwrap_or(1);
    let per_page = integer("per_page", present(&query.per_page))?.map_or(DEFAULT_PER_PAGE, |n| {
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
    Ok(Listing {
        insertion: InsertionQuery {
            filter: Filter {
                time: TimeRange::days(
                    date("qso_since", present(&query.qso_since))?,
                    date("qso_until", present(&query.qso_until))?,
                ),
                band: present(&query.band).map(str::to_owned),
                call,
                ..Filter::default()
            },
            mode_or_submode: present(&query.mode).map(str::to_owned),
            confirmed_by,
            after_seq: i64::try_from(since_id).map_err(|_| Wl2Error::validation("since_id is out of range"))?,
        },
        page,
        per_page,
    })
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    Wl2Access(access): Wl2Access,
    Wl2Query(query): Wl2Query<ListQuery>,
) -> Result<Json<Value>, Wl2Error> {
    let Listing {
        mut insertion,
        page,
        per_page,
    } = listing(&query)?;
    let mut conn = state.db.acquire().await?;
    insertion.filter.station_ids = station_ids(&mut conn, &access, present(&query.station_id)).await?;
    let offset = (page - 1).saturating_mul(u64::from(per_page));
    let found = qso::by_insertion(&mut conn, &access, &insertion, offset, per_page).await?;

    let last = found.rows.last().map_or(insertion.after_seq, |row| row.seq);
    let records: Vec<Qso> = found.rows.into_iter().map(|row| row.qso).collect();
    let total_pages = found.total.div_ceil(u64::from(per_page));
    Ok(data(
        json!({
            "exported": records.len(),
            "lastfetchedid": last,
            "adif": (!records.is_empty()).then(|| export::document(&records, LengthUnit::Codepoints)),
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

pub async fn create(
    State(state): State<Arc<AppState>>,
    Wl2Access(access): Wl2Access,
    Wl2Json(body): Wl2Json<Map<String, Value>>,
) -> Result<(StatusCode, Json<Value>), Wl2Error> {
    let wavelog_id = match body
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(STATION_PROFILE_ID))
        .map(|(_, value)| value)
    {
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(s)) => s.clone(),
        _ => return Err(Wl2Error::validation("station_profile_id is required")),
    };
    let mut qso = qso_from_body(&body)?;
    let entity = state
        .references
        .entity_of(&state.db, &state.config.reference, &qso)
        .await;

    let mut tx = WriteTx::begin(&state.db).await?;
    let station_id = mapped(&station::list(&mut tx, &access).await?, &wavelog_id)?.id.clone();
    let id = RecordIds::for_station(access.logbook_id(), &station_id).next(&qso)?;
    if let Some(entity) = entity {
        entity.fill(&mut qso)?;
    }
    match qso::create(
        &mut tx,
        &access,
        &id,
        QsoInput {
            station_id,
            instrument_id: None,
            qso,
        },
    )
    .await?
    {
        Created::New { stored, .. } => {
            tx.commit().await?;
            Ok((
                StatusCode::CREATED,
                data(
                    json!({ "id": stored.seq, "uuid": stored.id.to_string() }),
                    json!({ "resource": "qso", "method": "POST" }),
                ),
            ))
        }
        Created::Existing(stored) => {
            let summary = ["CALL", "QSO_DATE", "TIME_ON", "BAND", "MODE"]
                .map(|name| stored.qso.get(name).unwrap_or_default())
                .join(" ");
            Err(Wl2Error::validation("Duplicate QSO").with_details(json!({
                "critical": [],
                "validation": [],
                "duplicate": [summary],
            })))
        }
    }
}

fn integer(name: &str, value: Option<&str>) -> Result<Option<u64>, Wl2Error> {
    value
        .map(|v| {
            v.parse()
                .map_err(|_| Wl2Error::validation(format!("{name} {v:?} is not a non-negative integer")))
        })
        .transpose()
}

fn date(name: &str, value: Option<&str>) -> Result<Option<NaiveDate>, Wl2Error> {
    value
        .map(|value| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| Wl2Error::validation(format!("{name} {value:?} is not a YYYY-MM-DD date")))
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use rstest::rstest;
    use serde_json::{Value, json};

    use super::*;
    use crate::{
        domain::{
            adif::{Encoding, LengthUnit, Lengths, read_adi},
            reference::tests::CTY,
        },
        store::logbook::station::StationInput,
        web::{testing::Auth, wl2::tests::Fixture},
    };

    const QSO: &str = "/wl2/index.php/api/v2/qso";

    fn query(pairs: &[(&str, &str)]) -> ListQuery {
        let map: BTreeMap<&str, &str> = pairs.iter().copied().collect();
        serde_json::from_value(json!(map)).unwrap()
    }

    fn day(y: i32, m: u32, d: u32) -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(y, m, d)
    }

    #[rstest]
    #[case::defaults(&[("format", "ADIF ")], InsertionQuery::default(), 1, DEFAULT_PER_PAGE)]
    #[case::paging(
        &[("format", "adif"), ("since_id", "2"), ("page", "3"), ("per_page", "9000")],
        InsertionQuery { after_seq: 2, ..InsertionQuery::default() },
        3,
        MAX_PER_PAGE
    )]
    #[case::plain_call(
        &[("format", "adif"), ("callsign", "JA1AAA"), ("callsign_match", "plain")],
        InsertionQuery {
            filter: Filter { call: Some(CallMatch::Base("JA1AAA".into())), ..Filter::default() },
            ..InsertionQuery::default()
        },
        1,
        DEFAULT_PER_PAGE
    )]
    #[case::confirmations_and_days(
        &[("format", "adif"), ("qsl_filter", "lotw, QSL"), ("qso_since", "2026-10-22"), ("qso_until", "2026-10-22")],
        InsertionQuery {
            filter: Filter { time: TimeRange::days(day(2026, 10, 22), day(2026, 10, 22)), ..Filter::default() },
            confirmed_by: vec![Confirmation::Lotw, Confirmation::Card],
            ..InsertionQuery::default()
        },
        1,
        DEFAULT_PER_PAGE
    )]
    fn reads_the_list_query(
        #[case] pairs: &[(&str, &str)],
        #[case] insertion: InsertionQuery,
        #[case] page: u64,
        #[case] per_page: u32,
    ) {
        assert_eq!(
            listing(&query(pairs)).unwrap(),
            Listing {
                insertion,
                page,
                per_page
            }
        );
    }

    #[rstest]
    #[case::no_format(&[])]
    #[case::other_format(&[("format", "json")])]
    #[case::negative_since_id(&[("format", "adif"), ("since_id", "-1")])]
    #[case::page_zero(&[("format", "adif"), ("page", "0")])]
    #[case::per_page_zero(&[("format", "adif"), ("per_page", "0")])]
    #[case::unknown_confirmation(&[("format", "adif"), ("qsl_filter", "qrz")])]
    #[case::unknown_match(&[("format", "adif"), ("callsign", "JA1AAA"), ("callsign_match", "fuzzy")])]
    #[case::compact_date(&[("format", "adif"), ("qso_since", "20261022")])]
    fn refuses_list_queries_it_does_not_support(#[case] pairs: &[(&str, &str)]) {
        let refused = listing(&query(pairs)).unwrap_err();
        assert_eq!(
            (refused.status, refused.code),
            (StatusCode::BAD_REQUEST, "validation_error")
        );
    }

    async fn fixture() -> Fixture {
        let f = Fixture::new().await;
        let mapped =
            f.h.create_station(&StationInput {
                name: "Portable".into(),
                callsign: "JL1HIS/1".into(),
                wavelog_id: Some(3),
                fields: BTreeMap::new(),
            })
            .await;
        let session = f.h.session(&f.h.world.operator).await;
        for (station_id, call, time_on, extra) in [
            (&mapped.id, "JA1AAA", "010000", vec![("QSL_RCVD", "Y")]),
            (&mapped.id, "JA1AAA/P", "020000", vec![("BAND", "20m"), ("MODE", "FT8")]),
            (&f.h.station.id, "JA1BBB", "030000", vec![("LOTW_QSL_RCVD", "Y")]),
            (
                &mapped.id,
                "JA1CCC",
                "000000",
                vec![("QSO_DATE", "20261023"), ("NAME", "太郎")],
            ),
            (
                &mapped.id,
                "JA1DDD",
                "040000",
                vec![("MODE", "SSB"), ("SUBMODE", "USB")],
            ),
        ] {
            f.h.create_qso(&session, station_id, call, time_on, &extra).await;
        }
        let deleted = f.h.create_qso(&session, &mapped.id, "JA1EEE", "050000", &[]).await;
        f.h.delete_qso(&session, deleted["qso"]["id"].as_str().unwrap(), 1)
            .await;
        f
    }

    impl Fixture {
        async fn list(&self, query: &str) -> (StatusCode, Value) {
            self.get(&format!("{QSO}?{query}")).await
        }

        async fn calls(&self, query: &str) -> Vec<String> {
            let (status, body) = self.list(&format!("format=adif&{query}")).await;
            assert_eq!(status, StatusCode::OK, "{query}: {body}");
            calls(&body)
        }

        async fn create(&self, body: Value) -> (StatusCode, Value) {
            self.post(QSO, body).await
        }

        async fn stored(&self, uuid: &Value) -> Value {
            let session = self.h.session(&self.h.world.operator).await;
            let uri = self.h.path(&format!("/qsos/{}", uuid.as_str().unwrap()));
            let (status, stored) = self.h.send("GET", &uri, Auth::Session(&session), None).await;
            assert_eq!(status, StatusCode::OK, "{stored}");
            stored
        }
    }

    fn calls(body: &Value) -> Vec<String> {
        let Some(adif) = body["data"]["adif"].as_str() else {
            return vec![];
        };
        read_adi(adif, Encoding::Utf8, Lengths::Fixed(LengthUnit::Codepoints))
            .unwrap()
            .records
            .into_iter()
            .map(|qso| qso.unwrap().get("CALL").unwrap().to_owned())
            .collect()
    }

    #[tokio::test]
    async fn pages_through_the_insertion_order_after_an_id() {
        let f = fixture().await;

        let (status, first) = f.list("format=adif&station_id=3&since_id=0&per_page=2").await;
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

        let (_, next) = f.list("format=adif&station_id=3&since_id=2&per_page=2").await;
        assert_eq!(calls(&next), ["JA1CCC", "JA1DDD"]);
        assert!(next["data"]["adif"].as_str().unwrap().contains("<NAME:2>太郎"));
        assert_eq!(next["data"]["lastfetchedid"], 5);
        assert_eq!(next["meta"]["has_more"], false);

        let (_, last_page) = f.list("format=adif&station_id=3&page=2&per_page=2").await;
        assert_eq!(calls(&last_page), ["JA1CCC", "JA1DDD"]);
        assert_eq!(last_page["meta"]["total_pages"], 2);

        let (_, empty) = f.list("format=adif&station_id=3&since_id=5").await;
        assert_eq!(
            empty["data"],
            json!({ "exported": 0, "lastfetchedid": 5, "adif": null })
        );
        assert_eq!(
            empty["meta"],
            json!({ "page": 1, "per_page": 1000, "count": 0, "total": 0, "total_pages": 0, "has_more": false })
        );
    }

    fn hislog_body() -> Value {
        json!({
            "station_profile_id": 3,
            "call": "JA1XYZ",
            "qso_date": "2026-10-20",
            "time_on": "094127",
            "band": "40m",
            "freq": "7041500",
            "mode": "CW",
            "rst_sent": "599",
            "rst_rcvd": "579"
        })
    }

    #[tokio::test]
    async fn creates_a_qso_and_answers_a_resend_as_a_duplicate() {
        let f = fixture().await;

        let (status, created) = f.create(hislog_body()).await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        assert_eq!(created["data"]["id"], 7);
        let stored = f.stored(&created["data"]["uuid"]).await;
        assert_eq!(stored["fields"]["FREQ"], "7.0415");
        assert_eq!(stored["fields"]["QSO_DATE"], "20261020");
        assert_eq!(stored["fields"]["STATION_CALLSIGN"], "JL1HIS/1");
        assert_eq!(stored["fields"]["OPERATOR"], "JA1OPR");

        let (status, again) = f.create(hislog_body()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{again}");
        assert_eq!(again["error"]["code"], "validation_error");
        assert_eq!(
            again["error"]["details"]["duplicate"],
            json!(["JA1XYZ 20261020 094127 40m CW"])
        );

        let session = f.h.session(&f.h.world.operator).await;
        f.h.delete_qso(&session, created["data"]["uuid"].as_str().unwrap(), 1)
            .await;
        let (status, _) = f.create(hislog_body()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let mut nearby = hislog_body();
        nearby["time_on"] = json!("094130");
        let (status, body) = f.create(nearby).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(f.calls("since_id=6").await, ["JA1XYZ"]);
    }

    #[tokio::test]
    async fn reads_the_station_profile_id_in_any_case() {
        let f = fixture().await;
        let mut body = hislog_body();
        let id = body.as_object_mut().unwrap().remove("station_profile_id").unwrap();
        body["Station_Profile_Id"] = id;
        let (status, created) = f.create(body).await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        let stored = f.stored(&created["data"]["uuid"]).await;
        assert_eq!(stored["fields"]["STATION_CALLSIGN"], "JL1HIS/1");
        assert!(stored["fields"].get("STATION_PROFILE_ID").is_none());
    }

    #[tokio::test]
    async fn fills_entity_fields_without_changing_the_id_of_a_resend() {
        let f = fixture().await;
        let (status, created) = f.create(hislog_body()).await;
        assert_eq!(status, StatusCode::CREATED, "{created}");

        f.h.install_cty(CTY, "20260915").await;
        let (status, again) = f.create(hislog_body()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{again}");
        assert_eq!(
            again["error"]["details"]["duplicate"],
            json!(["JA1XYZ 20261020 094127 40m CW"])
        );

        let mut later = hislog_body();
        later["time_on"] = json!("101500");
        later["ituz"] = json!("44");
        let (status, created) = f.create(later).await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        let stored = f.stored(&created["data"]["uuid"]).await;
        assert_eq!(
            ["DXCC", "CQZ", "ITUZ", "CONT"].map(|name| stored["fields"][name].clone()),
            [json!("339"), json!("25"), json!("44"), json!("AS")]
        );
    }

    #[tokio::test]
    async fn keeps_identical_contacts_at_different_stations_and_deduplicates_resends() {
        let f = fixture().await;
        let mut input = StationInput {
            name: "Other".into(),
            callsign: "JL1HIS/1".into(),
            wavelog_id: Some(4),
            fields: BTreeMap::new(),
        };
        let other = f.h.create_station(&input).await;

        let mut second_body = hislog_body();
        second_body["station_profile_id"] = json!(4);
        let (first_status, first) = f.create(hislog_body()).await;
        let (second_status, second) = f.create(second_body.clone()).await;
        assert_eq!(first_status, StatusCode::CREATED, "{first}");
        assert_eq!(second_status, StatusCode::CREATED, "{second}");
        assert_ne!(first["data"]["uuid"], second["data"]["uuid"]);
        assert_eq!(f.calls("station_id=3&since_id=6").await, ["JA1XYZ"]);
        assert_eq!(f.calls("station_id=4&since_id=6").await, ["JA1XYZ"]);

        input.name = "Renamed".into();
        input.callsign = "JL1HIS/2".into();
        input.fields.insert("MY_GRIDSQUARE".into(), "PM96".into());
        let owner = f.h.world.access(&f.h.world.owner).await;
        let mut tx = WriteTx::begin(&f.h.world.db).await.unwrap();
        station::update(&mut tx, &owner.owner().unwrap(), &other.id, &input)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        for body in [hislog_body(), second_body.clone()] {
            let (status, duplicate) = f.create(body).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{duplicate}");
            assert!(
                !duplicate["error"]["details"]["duplicate"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
        }

        let session = f.h.session(&f.h.world.operator).await;
        f.h.delete_qso(&session, second["data"]["uuid"].as_str().unwrap(), 1)
            .await;
        let (status, duplicate) = f.create(second_body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{duplicate}");
        assert!(
            !duplicate["error"]["details"]["duplicate"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[rstest]
    #[case(json!({ "call": "JA1XYZ" }), StatusCode::BAD_REQUEST, "validation_error")]
    #[case(json!({ "station_profile_id": 7, "call": "JA1XYZ", "qso_date": "2026-10-20", "time_on": "0941" }), StatusCode::FORBIDDEN, "forbidden")]
    #[case(json!({ "station_profile_id": "3", "call": "JA1XYZ", "band": "40m", "mode": "CW" }), StatusCode::BAD_REQUEST, "validation_error")]
    #[case(json!({ "station_profile_id": 3, "call": "JA1XYZ", "qso_date": "2026-10-20", "time_on": "0941", "mode": "CW" }), StatusCode::BAD_REQUEST, "validation_error")]
    #[case(json!([]), StatusCode::BAD_REQUEST, "validation_error")]
    #[tokio::test]
    async fn refuses_a_qso_it_cannot_record(#[case] body: Value, #[case] status: StatusCode, #[case] code: &str) {
        let f = fixture().await;
        let (actual, answer) = f.create(body).await;
        assert_eq!(
            (actual, answer["error"]["code"].as_str()),
            (status, Some(code)),
            "{answer}"
        );
        assert!(answer["error"].get("details").is_none());
    }

    #[rstest]
    #[case("", &["JA1AAA", "JA1AAA/P", "JA1BBB", "JA1CCC", "JA1DDD"])]
    #[case("station_id=3", &["JA1AAA", "JA1AAA/P", "JA1CCC", "JA1DDD"])]
    #[case("callsign=ja1aaa", &["JA1AAA"])]
    #[case("callsign=JA1AAA&callsign_match=plain", &["JA1AAA", "JA1AAA/P"])]
    #[case("band=20M&mode=ft8", &["JA1AAA/P"])]
    #[case("mode=usb", &["JA1DDD"])]
    #[case("mode=SSB", &["JA1DDD"])]
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
    #[case("format=adif&station_id=7", StatusCode::FORBIDDEN, "forbidden")]
    #[case("format=adif&page=x", StatusCode::BAD_REQUEST, "validation_error")]
    #[tokio::test]
    async fn refuses_what_it_does_not_support(#[case] query: &str, #[case] status: StatusCode, #[case] code: &str) {
        let f = Fixture::new().await;
        let (actual, body) = f.list(query).await;
        assert_eq!((actual, body["error"]["code"].as_str()), (status, Some(code)), "{body}");
    }
}
