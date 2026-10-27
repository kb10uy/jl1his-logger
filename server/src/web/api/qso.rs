pub(super) mod events;
pub(super) mod export;
pub(super) mod import;
pub(in crate::web) mod lookup;

use std::{collections::BTreeMap, sync::Arc};

use axum::{
    extract::{FromRequestParts, Path, State},
    http::{HeaderName, StatusCode, header, request::Parts},
};
use serde::{Deserialize, Serialize};

use crate::{
    domain::qso::{Qso, QsoId, Warning},
    store::{
        StoreError, WriteTx,
        access::LogbookAccess,
        qso::{self, CallMatch, Created, Cursor, Filter, Order, QsoInput, StoredQso},
    },
    web::{
        AppState,
        api::logbook::member::{Item, Member},
        api::qso::export::TargetBody,
        error::ApiError,
        extract::{Json, Query},
        query::{range, text},
    },
};

const MAX_LIMIT: u32 = 1000;
const DEFAULT_SEARCH_LIMIT: u32 = 100;

#[derive(Debug, Serialize)]
pub struct QsoBody {
    id: String,
    station_id: String,
    instrument_id: Option<String>,
    seq: i64,
    updated_seq: i64,
    revision: i64,
    created_by: String,
    created_by_callsign: String,
    created_at: String,
    updated_by: String,
    updated_by_callsign: String,
    updated_at: String,
    deleted_at: Option<String>,
    fields: BTreeMap<String, String>,
}

impl From<StoredQso> for QsoBody {
    fn from(stored: StoredQso) -> QsoBody {
        QsoBody {
            fields: stored
                .qso
                .iter()
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
                .collect(),
            id: stored.id.to_string(),
            station_id: stored.station_id,
            instrument_id: stored.instrument_id,
            seq: stored.seq,
            updated_seq: stored.updated_seq,
            revision: stored.revision,
            created_by: stored.created_by,
            created_by_callsign: stored.created_by_callsign,
            created_at: stored.created_at,
            updated_by: stored.updated_by,
            updated_by_callsign: stored.updated_by_callsign,
            updated_at: stored.updated_at,
            deleted_at: stored.deleted_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Saved {
    qso: QsoBody,
    warnings: Vec<Warning>,
}

#[derive(Debug, Deserialize)]
pub struct NewQso {
    id: String,
    station_id: String,
    instrument_id: Option<String>,
    fields: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct ChangedQso {
    station_id: String,
    instrument_id: Option<String>,
    fields: BTreeMap<String, String>,
}

pub struct IfMatch(i64);

impl<S: Send + Sync> FromRequestParts<S> for IfMatch {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<IfMatch, ApiError> {
        let value = parts.headers.get(header::IF_MATCH).ok_or_else(|| {
            ApiError::new(
                StatusCode::PRECONDITION_REQUIRED,
                "If-Match with the revision of the QSO is required",
            )
        })?;
        value
            .to_str()
            .ok()
            .and_then(|v| v.trim().strip_prefix('"')?.strip_suffix('"')?.parse().ok())
            .map(IfMatch)
            .ok_or_else(|| ApiError::bad_request("If-Match takes one strong entity tag, the revision in quotes"))
    }
}

type Tagged<T> = ([(HeaderName, String); 1], Json<T>);

fn tagged<T>(revision: i64, body: T) -> Tagged<T> {
    ([(header::ETAG, format!("\"{revision}\""))], Json(body))
}

#[derive(Debug, Deserialize)]
pub struct ChangesQuery {
    after: Option<i64>,
    limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ChangesBody {
    cursor_generation: String,
    last_updated_seq: i64,
    qsos: Vec<QsoBody>,
    has_more: bool,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    from: Option<String>,
    until: Option<String>,
    station_id: Option<String>,
    band: Option<String>,
    mode: Option<String>,
    call: Option<String>,
    call_base: Option<String>,
    qsl_sent: Option<String>,
    contest_id: Option<String>,
    q: Option<String>,
    sort: Option<String>,
    cursor: Option<String>,
    limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct PageBody {
    qsos: Vec<QsoBody>,
    next: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct TargetsBody {
    qsos: Vec<TargetBody>,
}

#[derive(Debug, Deserialize)]
pub struct DuplicatesQuery {
    call: Option<String>,
    band: Option<String>,
    mode: Option<String>,
    qso_date: Option<String>,
    time_on: Option<String>,
    except: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DuplicatesBody {
    qsos: Vec<QsoBody>,
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    member: Member,
    Json(req): Json<NewQso>,
) -> Result<(StatusCode, Tagged<Saved>), ApiError> {
    let id: QsoId = req.id.parse().map_err(|e| ApiError::bad_request(format!("{e}")))?;
    let mut qso = Qso::from_fields(req.fields)?;
    if let Some(entity) = state
        .references
        .entity_of(&state.db, &state.config.reference, &qso)
        .await
    {
        entity.fill(&mut qso)?;
    }
    let input = QsoInput {
        station_id: req.station_id,
        instrument_id: req.instrument_id,
        qso,
    };
    let mut tx = WriteTx::begin(&state.db).await?;
    let created = qso::create(&mut tx, &member.access, &id, input).await?;
    tx.commit().await?;
    let (status, stored, warnings) = match created {
        Created::New { stored, warnings } => (StatusCode::CREATED, stored, warnings),
        Created::Existing(stored) => (StatusCode::OK, stored, vec![]),
    };
    Ok((
        status,
        tagged(
            stored.revision,
            Saved {
                qso: stored.into(),
                warnings,
            },
        ),
    ))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(item): Path<Item>,
) -> Result<Tagged<QsoBody>, ApiError> {
    let id = path_id(&item.id)?;
    let mut conn = state.db.acquire().await?;
    let stored = qso::get(&mut conn, &member.access, &id).await?;
    Ok(tagged(stored.revision, stored.into()))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(item): Path<Item>,
    IfMatch(revision): IfMatch,
    Json(req): Json<ChangedQso>,
) -> Result<Tagged<Saved>, ApiError> {
    let id = path_id(&item.id)?;
    let input = QsoInput {
        station_id: req.station_id,
        instrument_id: req.instrument_id,
        qso: Qso::from_fields(req.fields)?,
    };
    let mut tx = WriteTx::begin(&state.db).await?;
    match qso::update(&mut tx, &member.access, &id, revision, input).await {
        Ok((stored, warnings)) => {
            tx.commit().await?;
            Ok(tagged(
                stored.revision,
                Saved {
                    qso: stored.into(),
                    warnings,
                },
            ))
        }
        Err(e) => {
            drop(tx);
            Err(with_current(&state, &member.access, &id, e).await)
        }
    }
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(item): Path<Item>,
    IfMatch(revision): IfMatch,
) -> Result<Tagged<QsoBody>, ApiError> {
    let id = path_id(&item.id)?;
    let mut tx = WriteTx::begin(&state.db).await?;
    match qso::delete(&mut tx, &member.access, &id, revision).await {
        Ok(stored) => {
            tx.commit().await?;
            Ok(tagged(stored.revision, stored.into()))
        }
        Err(e) => {
            drop(tx);
            Err(with_current(&state, &member.access, &id, e).await)
        }
    }
}

pub async fn changes(
    State(state): State<Arc<AppState>>,
    member: Member,
    Query(query): Query<ChangesQuery>,
) -> Result<Json<ChangesBody>, ApiError> {
    let limit = limit(query.limit, MAX_LIMIT)?;
    let mut conn = state.db.acquire().await?;
    let changes = qso::changes(&mut conn, &member.access, query.after.unwrap_or(0), limit).await?;
    Ok(Json(ChangesBody {
        cursor_generation: changes.cursor_generation,
        last_updated_seq: changes.last_updated_seq,
        qsos: changes.rows.into_iter().map(QsoBody::from).collect(),
        has_more: changes.has_more,
    }))
}

fn search_filter(query: SearchQuery) -> Result<Filter, ApiError> {
    let call = match (query.call, query.call_base) {
        (Some(_), Some(_)) => return Err(ApiError::bad_request("give call or call_base, not both")),
        (Some(call), None) => Some(CallMatch::Exact(call)),
        (None, Some(call)) => Some(CallMatch::Base(call)),
        (None, None) => None,
    };
    let filter = Filter {
        time: range(query.from.as_deref(), query.until.as_deref())?,
        station_ids: query.station_id.map(|id| vec![id]),
        band: query.band,
        mode: query.mode,
        call,
        qsl_sent: query.qsl_sent.map(|v| v.split(',').map(str::to_owned).collect()),
        contest_id: query.contest_id,
        query: text(query.q.as_deref())?,
    };
    Ok(filter)
}

pub async fn search(
    State(state): State<Arc<AppState>>,
    member: Member,
    Query(mut query): Query<SearchQuery>,
) -> Result<Json<PageBody>, ApiError> {
    let limit = limit(query.limit, DEFAULT_SEARCH_LIMIT)?;
    let order = order(query.sort.take().as_deref())?;
    let after = query
        .cursor
        .take()
        .as_deref()
        .map(|cursor| parse_cursor(cursor, order))
        .transpose()?;
    let filter = search_filter(query)?;
    let mut conn = state.db.acquire().await?;
    let page = qso::search(&mut conn, &member.access, &filter, order, after.as_ref(), limit).await?;
    let total = match after {
        Some(_) => None,
        None => Some(qso::count(&mut conn, &member.access, &filter).await?),
    };
    Ok(Json(PageBody {
        qsos: page.rows.into_iter().map(QsoBody::from).collect(),
        next: page.next.map(|c| match c.call_base {
            Some(call) => format!("{}_{}_{call}", c.qso_time, c.id),
            None => format!("{}_{}", c.qso_time, c.id),
        }),
        total,
    }))
}

pub async fn targets(
    State(state): State<Arc<AppState>>,
    member: Member,
    Query(mut query): Query<SearchQuery>,
) -> Result<Json<TargetsBody>, ApiError> {
    if query.cursor.take().is_some() || query.limit.take().is_some() || query.sort.take().is_some() {
        return Err(ApiError::bad_request("targets take no sort, cursor or limit"));
    }
    let filter = search_filter(query)?;
    let mut conn = state.db.acquire().await?;
    let targets = qso::targets(&mut conn, &member.access, &filter).await?;
    Ok(Json(TargetsBody {
        qsos: targets.into_iter().map(TargetBody::from).collect(),
    }))
}

pub async fn duplicates(
    State(state): State<Arc<AppState>>,
    member: Member,
    Query(query): Query<DuplicatesQuery>,
) -> Result<Json<DuplicatesBody>, ApiError> {
    let candidate = Qso::from_fields(
        [
            ("CALL", query.call),
            ("BAND", query.band),
            ("MODE", query.mode),
            ("QSO_DATE", query.qso_date),
            ("TIME_ON", query.time_on),
        ]
        .into_iter()
        .filter_map(|(name, value)| Some((name, value?))),
    )?;
    let except = query
        .except
        .as_deref()
        .map(|id| id.parse::<QsoId>().map_err(|e| ApiError::bad_request(e.to_string())))
        .transpose()?;
    let mut conn = state.db.acquire().await?;
    let rows = qso::possible_duplicates(&mut conn, &member.access, &candidate, except.as_ref()).await?;
    Ok(Json(DuplicatesBody {
        qsos: rows.into_iter().map(QsoBody::from).collect(),
    }))
}

async fn with_current(state: &AppState, access: &LogbookAccess, id: &QsoId, e: StoreError) -> ApiError {
    if !matches!(e, StoreError::Conflict(_)) {
        return e.into();
    }
    let current = match state.db.acquire().await {
        Ok(mut conn) => qso::get(&mut conn, access, id).await.ok(),
        Err(_) => None,
    };
    match current {
        Some(current) => {
            let status = match current.deleted_at {
                Some(_) => StatusCode::CONFLICT,
                None => StatusCode::PRECONDITION_FAILED,
            };
            ApiError::new(status, e.to_string()).with(
                "current",
                serde_json::to_value(QsoBody::from(current)).expect("a QSO serializes"),
            )
        }
        None => e.into(),
    }
}

fn path_id(id: &str) -> Result<QsoId, ApiError> {
    id.parse().map_err(|_| StoreError::NotFound("QSO").into())
}

fn limit(limit: Option<u32>, default: u32) -> Result<u32, ApiError> {
    match limit.unwrap_or(default) {
        0 => Err(ApiError::bad_request("limit must be at least 1")),
        limit => Ok(limit.min(MAX_LIMIT)),
    }
}

fn order(sort: Option<&str>) -> Result<Order, ApiError> {
    match sort.unwrap_or("-time") {
        "-time" => Ok(Order::Newest),
        "time" => Ok(Order::Oldest),
        "call" => Ok(Order::CallAscending),
        "-call" => Ok(Order::CallDescending),
        other => Err(ApiError::bad_request(format!(
            "{other:?} is not a sort order; give -time, time, call or -call"
        ))),
    }
}

fn parse_cursor(cursor: &str, order: Order) -> Result<Cursor, ApiError> {
    let mut parts = cursor.splitn(3, '_');
    let (time, id, call) = (parts.next(), parts.next(), parts.next());
    time.zip(id)
        .filter(|_| call.is_some() == order.by_call())
        .and_then(|(time, id)| {
            Some(Cursor {
                call_base: call.map(str::to_owned),
                qso_time: time.parse().ok()?,
                id: id.to_owned(),
            })
        })
        .ok_or_else(|| ApiError::bad_request(format!("{cursor:?} is not a cursor of this sort order")))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serde_json::{Value, json};

    use super::*;
    use crate::{
        domain::reference::tests::CTY,
        store::reference::ReferenceKind,
        web::testing::{Auth, Harness},
    };

    fn entry(h: &Harness, id: &QsoId, call: &str, band: &str, time_on: &str) -> Value {
        json!({
            "id": id.to_string(),
            "station_id": h.station.id,
            "fields": {
                "CALL": call,
                "QSO_DATE": "20261022",
                "TIME_ON": time_on,
                "BAND": band,
                "MODE": "CW",
                "freq": "7.041",
                "NAME": "太郎",
            },
        })
    }

    async fn create(h: &Harness, cookie: &str, call: &str, band: &str, time_on: &str) -> Value {
        h.create_qso(
            cookie,
            &h.station.id,
            call,
            time_on,
            &[("BAND", band), ("freq", "7.041"), ("NAME", "太郎")],
        )
        .await
    }

    #[tokio::test]
    async fn bakes_the_named_instrument_and_keeps_its_id() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let rig = h.world.instrument("IC-705").await;
        let id = QsoId::generate();
        let mut body = entry(&h, &id, "JA1XYZ", "40m", "040506");
        body["instrument_id"] = json!(rig.id);
        let (status, created) = h
            .send("POST", &h.path("/qsos"), Auth::Session(&cookie), Some(body))
            .await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        assert_eq!(created["qso"]["instrument_id"], json!(rig.id));
        assert_eq!(
            (&created["qso"]["fields"]["MY_RIG"], &created["qso"]["fields"]["TX_PWR"]),
            (&json!("IC-705"), &json!("0.5"))
        );

        let change = json!({ "station_id": h.station.id, "fields": created["qso"]["fields"] });
        let (status, _, updated) = h
            .send_with(
                "PUT",
                &h.path(&format!("/qsos/{id}")),
                Auth::Session(&cookie),
                &[("if-match", "\"1\"")],
                Some(change),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{updated}");
        assert_eq!(updated["qso"]["instrument_id"], Value::Null);
        assert_eq!(updated["qso"]["fields"]["MY_RIG"], "IC-705");

        let mut unknown = entry(&h, &QsoId::generate(), "JA1ZZZ", "40m", "050000");
        unknown["instrument_id"] = json!("nothing");
        let (status, _) = h
            .send("POST", &h.path("/qsos"), Auth::Session(&cookie), Some(unknown))
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn creates_a_qso_once_per_id() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let id = QsoId::generate();

        let (status, created) = h
            .send(
                "POST",
                &h.path("/qsos"),
                Auth::Session(&cookie),
                Some(entry(&h, &id, "JA1XYZ", "40m", "040506")),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        assert_eq!(created["warnings"], json!([]));
        let qso = &created["qso"];
        assert_eq!(qso["id"], id.to_string());
        assert_eq!((qso["seq"].as_i64(), qso["revision"].as_i64()), (Some(1), Some(1)));
        assert_eq!(qso["created_by"], h.world.operator.id);
        assert_eq!(qso["created_by_callsign"], "JA1OPR");
        assert_eq!(qso["updated_by_callsign"], "JA1OPR");
        assert_eq!(qso["deleted_at"], Value::Null);
        assert_eq!(qso["fields"]["STATION_CALLSIGN"], "JL1HIS/1");
        assert_eq!(qso["fields"]["OPERATOR"], "JA1OPR");
        assert_eq!(qso["fields"]["FREQ"], "7.041");
        assert_eq!(qso["fields"]["NAME"], "太郎");

        let (status, again) = h
            .send(
                "POST",
                &h.path("/qsos"),
                Auth::Session(&cookie),
                Some(entry(&h, &id, "JA1ZZZ", "40m", "050000")),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(again["qso"], created["qso"]);

        let (status, headers, fetched) = h
            .send_with(
                "GET",
                &h.path(&format!("/qsos/{id}")),
                Auth::Session(&cookie),
                &[],
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::ETAG], "\"1\"");
        assert_eq!(fetched, created["qso"]);
    }

    #[tokio::test]
    async fn fills_the_entity_fields_a_new_qso_lacks() {
        let (h, _dir) = Harness::with_references().await;
        let cookie = h.session(&h.world.operator).await;
        let entity = |body: &Value| ["DXCC", "CQZ", "ITUZ", "CONT"].map(|name| body["qso"]["fields"][name].clone());

        let bare = create(&h, &cookie, "JA1AAA", "40m", "010000").await;
        assert_eq!(entity(&bare), [Value::Null, Value::Null, Value::Null, Value::Null]);

        h.install_cty(CTY, "20260915").await;
        let filled = create(&h, &cookie, "JA1BBB", "40m", "020000").await;
        assert_eq!(entity(&filled), [json!("339"), json!("25"), json!("45"), json!("AS")]);

        let created = h
            .create_qso(
                &cookie,
                &h.station.id,
                "JA1CCC",
                "030000",
                &[("CQZ", "26"), ("cont", "OC")],
            )
            .await;
        assert_eq!(entity(&created), [json!("339"), json!("26"), json!("45"), json!("OC")]);

        let mut fields = created["qso"]["fields"].clone();
        fields.as_object_mut().unwrap().remove("DXCC");
        let uri = h.path(&format!("/qsos/{}", created["qso"]["id"].as_str().unwrap()));
        let (status, _, updated) = h
            .send_with(
                "PUT",
                &uri,
                Auth::Session(&cookie),
                &[("if-match", "\"1\"")],
                Some(json!({ "station_id": h.station.id, "fields": fields })),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{updated}");
        assert_eq!(updated["qso"]["fields"]["DXCC"], Value::Null);

        let unknown = create(&h, &cookie, "XX0XX", "40m", "040000").await;
        assert_eq!(entity(&unknown), [Value::Null, Value::Null, Value::Null, Value::Null]);
    }

    #[tokio::test]
    async fn records_a_qso_when_the_reference_data_cannot_be_read() {
        let (h, _dir) = Harness::with_references().await;
        h.install_cty(CTY, "20260915").await;
        std::fs::write(h.reference_path(ReferenceKind::Cty), "changed").unwrap();
        let cookie = h.session(&h.world.operator).await;
        let created = create(&h, &cookie, "JA1AAA", "40m", "010000").await;
        assert_eq!(created["qso"]["fields"]["CQZ"], Value::Null);
    }

    #[tokio::test]
    async fn refuses_malformed_ids() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let mut body = entry(&h, &QsoId::generate(), "JA1XYZ", "40m", "040506");
        body["id"] = json!("6f1c2a3e-4b5d-4e6f-8a7b-0123456789ab");
        let (status, _) = h
            .send("POST", &h.path("/qsos"), Auth::Session(&cookie), Some(body))
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, body) = h
            .send("GET", &h.path("/qsos/not-a-uuid"), Auth::Session(&cookie), None)
            .await;
        assert_eq!(
            (status, &body["error"], &body["code"], &body["what"]),
            (
                StatusCode::NOT_FOUND,
                &json!("QSO not found"),
                &json!("not_found"),
                &json!("QSO")
            )
        );
        let (status, _) = h
            .send(
                "GET",
                &h.path("/duplicates?call=JA1XYZ&band=40m&mode=CW&qso_date=20261022&time_on=0100&except=x"),
                Auth::Session(&cookie),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn reports_invalid_qsos() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let mut body = entry(&h, &QsoId::generate(), "JA1XYZ", "40m", "040506");
        body["fields"]["BAND"] = json!("41m");
        let (status, reply) = h
            .send("POST", &h.path("/qsos"), Auth::Session(&cookie), Some(body))
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(reply["status"], "error");
        assert!(reply["error"].as_str().unwrap().contains("BAND"), "{reply}");
        assert_eq!(
            (&reply["code"], &reply["field"], &reply["value"]),
            (&json!("qso_invalid"), &json!("BAND"), &json!("41m")),
            "{reply}"
        );

        let (status, reply) = h
            .send(
                "POST",
                &h.path("/qsos"),
                Auth::Session(&cookie),
                Some(json!({ "id": 1 })),
            )
            .await;
        assert!(status.is_client_error(), "{status}");
        assert_eq!(reply["status"], "error");
    }

    #[rstest]
    #[case::submode_of_its_mode("MFSK", "FT4", json!([]))]
    #[case::mode_from_import_only("DSTAR", "", json!([]))]
    #[case::unknown_mode("FT9", "", json!([{ "kind": "unknown_mode", "mode": "FT9" }]))]
    #[case::unknown_submode("SSB", "XSB", json!([{ "kind": "unknown_submode", "submode": "XSB" }]))]
    #[case::submode_of_another_mode(
        "FT8",
        "ft4",
        json!([{ "kind": "submode_of_another_mode", "mode": "FT8", "submode": "ft4", "expected": "MFSK" }])
    )]
    #[tokio::test]
    async fn warns_about_modes_outside_the_adif_table(
        #[case] mode: &str,
        #[case] submode: &str,
        #[case] expected: Value,
    ) {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let reply = h
            .create_qso(
                &cookie,
                &h.station.id,
                "JA1XYZ",
                "040506",
                &[("MODE", mode), ("SUBMODE", submode)],
            )
            .await;
        assert_eq!(reply["warnings"], expected);
    }

    #[tokio::test]
    async fn warns_about_possible_duplicates() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let first = create(&h, &cookie, "JA1XYZ", "40m", "0405").await;
        let first_id = first["qso"]["id"].clone();
        let second = create(&h, &cookie, "ja1xyz", "40M", "0410").await;
        assert_eq!(
            second["warnings"],
            json!([{ "kind": "possible_duplicate", "ids": [first_id] }])
        );

        let uri = h.path("/duplicates?call=JA1XYZ&band=40m&mode=cw&qso_date=20261022&time_on=0415");
        let (status, body) = h.send("GET", &uri, Auth::Session(&cookie), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, json!({ "qsos": [first["qso"], second["qso"]] }));

        let except = format!("{uri}&except={}", first_id.as_str().unwrap());
        let (_, body) = h.send("GET", &except, Auth::Session(&cookie), None).await;
        assert_eq!(body, json!({ "qsos": [second["qso"]] }));

        let (status, _) = h
            .send("GET", &h.path("/duplicates?call=JA1XYZ"), Auth::Session(&cookie), None)
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[rstest]
    #[case::missing(&[], StatusCode::PRECONDITION_REQUIRED)]
    #[case::unquoted(&[("if-match", "1")], StatusCode::BAD_REQUEST)]
    #[case::weak(&[("if-match", "W/\"1\"")], StatusCode::BAD_REQUEST)]
    #[case::any(&[("if-match", "*")], StatusCode::BAD_REQUEST)]
    #[tokio::test]
    async fn requires_a_strong_if_match(
        #[case] headers: &'static [(&'static str, &'static str)],
        #[case] expected: StatusCode,
    ) {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let created = create(&h, &cookie, "JA1XYZ", "40m", "040506").await;
        let uri = h.path(&format!("/qsos/{}", created["qso"]["id"].as_str().unwrap()));
        let change = json!({ "station_id": h.station.id, "fields": created["qso"]["fields"] });
        let (status, _, _) = h
            .send_with("PUT", &uri, Auth::Session(&cookie), headers, Some(change))
            .await;
        assert_eq!(status, expected);
        let (status, _, _) = h.send_with("DELETE", &uri, Auth::Session(&cookie), headers, None).await;
        assert_eq!(status, expected);
    }

    #[tokio::test]
    async fn updates_at_the_revision_in_if_match() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let created = create(&h, &cookie, "JA1XYZ", "40m", "040506").await;
        let uri = h.path(&format!("/qsos/{}", created["qso"]["id"].as_str().unwrap()));
        let mut fields = created["qso"]["fields"].clone();
        fields["NAME"] = json!("Hanako");
        fields["COMMENT"] = json!("tnx");
        let change = json!({ "station_id": h.station.id, "fields": fields });
        let put = |if_match: &'static [(&'static str, &'static str)]| {
            h.send_with("PUT", &uri, Auth::Session(&cookie), if_match, Some(change.clone()))
        };

        let (status, headers, updated) = put(&[("if-match", "\"1\"")]).await;
        assert_eq!(status, StatusCode::OK, "{updated}");
        assert_eq!(headers[header::ETAG], "\"2\"");
        assert_eq!(updated["qso"]["revision"], 2);
        assert_eq!(updated["qso"]["updated_seq"], 2);
        assert_eq!(updated["qso"]["fields"]["NAME"], "Hanako");
        assert_eq!(updated["qso"]["fields"]["COMMENT"], "tnx");

        let (status, _, stale) = put(&[("if-match", "\"1\"")]).await;
        assert_eq!(status, StatusCode::PRECONDITION_FAILED);
        assert_eq!(stale["status"], "error");
        assert_eq!(stale["current"], updated["qso"]);

        let deleted = h.delete_qso(&cookie, updated["qso"]["id"].as_str().unwrap(), 2).await;
        let (status, _, gone) = put(&[("if-match", "\"3\"")]).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(gone["current"], deleted);
    }

    #[tokio::test]
    async fn deletes_leave_tombstones_in_the_changes() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let gone = create(&h, &cookie, "JA1XYZ", "40m", "0100").await;
        let kept = create(&h, &cookie, "JA1ABC", "40m", "0200").await;
        let uri = h.path(&format!("/qsos/{}", gone["qso"]["id"].as_str().unwrap()));

        let (status, _, stale) = h
            .send_with("DELETE", &uri, Auth::Session(&cookie), &[("if-match", "\"2\"")], None)
            .await;
        assert_eq!(status, StatusCode::PRECONDITION_FAILED);
        assert_eq!(stale["current"], gone["qso"]);

        let (status, headers, deleted) = h
            .send_with("DELETE", &uri, Auth::Session(&cookie), &[("if-match", "\"1\"")], None)
            .await;
        assert_eq!(status, StatusCode::OK, "{deleted}");
        assert_eq!(headers[header::ETAG], "\"2\"");
        assert!(deleted["deleted_at"].is_string());
        assert_eq!(
            (deleted["revision"].as_i64(), deleted["updated_seq"].as_i64()),
            (Some(2), Some(3))
        );

        let changes = h.path("/changes");
        let (status, body) = h.send("GET", &changes, Auth::Session(&cookie), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["cursor_generation"], h.world.logbook.cursor_generation);
        assert_eq!(body["last_updated_seq"], 3);
        assert_eq!(body["has_more"], false);
        assert_eq!(body["qsos"], json!([kept["qso"], deleted]));

        let (_, first) = h
            .send(
                "GET",
                &format!("{changes}?after=0&limit=1"),
                Auth::Session(&cookie),
                None,
            )
            .await;
        assert_eq!(
            (first["qsos"][0]["updated_seq"].as_i64(), first["has_more"].as_bool()),
            (Some(2), Some(true))
        );
        let (_, rest) = h
            .send("GET", &format!("{changes}?after=2"), Auth::Session(&cookie), None)
            .await;
        assert_eq!(rest["qsos"], json!([deleted]));

        let (_, page) = h.send("GET", &h.path("/qsos"), Auth::Session(&cookie), None).await;
        assert_eq!(page["qsos"], json!([kept["qso"]]));
    }

    async fn three(h: &Harness) -> (String, [Value; 3]) {
        let cookie = h.session(&h.world.operator).await;
        let early = create(h, &cookie, "JA1XYZ", "40m", "0100").await;
        let middle = create(h, &cookie, "JA1XYZ/P", "20m", "0200").await;
        let late = create(h, &cookie, "JA1ABC", "40m", "0300").await;
        (cookie, [early, middle, late].map(|created| created["qso"].clone()))
    }

    #[tokio::test]
    async fn searches_newest_first_with_a_cursor() {
        let h = Harness::new().await;
        let (cookie, [early, middle, late]) = three(&h).await;

        let (status, first) = h
            .send("GET", &h.path("/qsos?limit=2"), Auth::Session(&cookie), None)
            .await;
        assert_eq!(status, StatusCode::OK, "{first}");
        assert_eq!(first["qsos"], json!([late, middle]));
        assert_eq!(first["total"], 3);
        let cursor = first["next"].as_str().unwrap();
        let (_, second) = h
            .send(
                "GET",
                &h.path(&format!("/qsos?limit=2&cursor={cursor}")),
                Auth::Session(&cookie),
                None,
            )
            .await;
        assert_eq!(second["qsos"], json!([early]));
        assert_eq!(second["next"], Value::Null);
        assert!(second.get("total").is_none(), "{second}");
    }

    #[rstest]
    #[case::newest("", [2, 1, 0])]
    #[case::oldest("&sort=time", [0, 1, 2])]
    #[case::call("&sort=call", [2, 1, 0])]
    #[case::call_descending("&sort=-call", [1, 0, 2])]
    #[tokio::test]
    async fn pages_through_each_sort_order(#[case] sort: &str, #[case] expected: [usize; 3]) {
        let h = Harness::new().await;
        let (cookie, qsos) = three(&h).await;
        let mut found = vec![];
        let mut uri = h.path(&format!("/qsos?limit=1{sort}"));
        loop {
            let (status, page) = h.send("GET", &uri, Auth::Session(&cookie), None).await;
            assert_eq!(status, StatusCode::OK, "{page}");
            found.extend(page["qsos"].as_array().unwrap().iter().cloned());
            match page["next"].as_str() {
                Some(next) => uri = h.path(&format!("/qsos?limit=1{sort}&cursor={next}")),
                None => break,
            }
        }
        assert_eq!(found, expected.map(|i| qsos[i].clone()));
    }

    #[tokio::test]
    async fn lists_the_targets_of_a_query() {
        let h = Harness::new().await;
        let (cookie, [early, middle, _]) = three(&h).await;
        let (status, body) = h
            .send("GET", &h.path("/targets?q=call:JA1XYZ*"), Auth::Session(&cookie), None)
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let targets = json!([
            { "id": middle["id"], "revision": 1 },
            { "id": early["id"], "revision": 1 },
        ]);
        assert_eq!(body["qsos"], targets);
        let (status, _) = h
            .send("GET", &h.path("/targets?limit=1"), Auth::Session(&cookie), None)
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn names_the_bad_term_of_a_query() {
        let h = Harness::new().await;
        let (cookie, _) = three(&h).await;
        let (status, body) = h
            .send(
                "GET",
                &h.path("/qsos?q=band:40m%20qrg:70"),
                Auth::Session(&cookie),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            (
                &body["code"],
                &body["term"],
                &body["index"],
                &body["start"],
                &body["end"],
                &body["value"]
            ),
            (
                &json!("unknown_field"),
                &json!("qrg:70"),
                &json!(1),
                &json!(9),
                &json!(15),
                &json!("qrg")
            )
        );
    }

    #[rstest]
    #[case::band("band=20M", &[1])]
    #[case::call("call=ja1xyz", &[0])]
    #[case::call_base("call_base=JA1XYZ", &[1, 0])]
    #[case::time("from=2026-10-22T01:30:00Z&until=2026-10-22T03:00:00Z", &[1])]
    #[case::qsl_sent_empty("qsl_sent=", &[2, 1, 0])]
    #[case::qsl_sent("qsl_sent=Y", &[])]
    #[case::text("q=-band:20m%20call:JA1*", &[2, 0])]
    #[case::text_and_filter("q=call:JA1XYZ*&band=40m", &[0])]
    #[tokio::test]
    async fn filters_searches(#[case] query: &str, #[case] expected: &[usize]) {
        let h = Harness::new().await;
        let (cookie, qsos) = three(&h).await;
        let (status, page) = h
            .send("GET", &h.path(&format!("/qsos?{query}")), Auth::Session(&cookie), None)
            .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        let expected: Vec<&Value> = expected.iter().map(|i| &qsos[*i]).collect();
        assert_eq!(page["qsos"], json!(expected));
    }

    #[rstest]
    #[case("call=A&call_base=A")]
    #[case("from=20261022")]
    #[case("cursor=nope")]
    #[case("sort=band")]
    #[case("sort=call&cursor=0_x")]
    #[case("cursor=0_x_JA1XYZ")]
    #[case("limit=0")]
    #[case("limit=many")]
    #[tokio::test]
    async fn refuses_searches_it_cannot_read(#[case] query: &str) {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let (status, body) = h
            .send("GET", &h.path(&format!("/qsos?{query}")), Auth::Session(&cookie), None)
            .await;
        assert_eq!((status, &body["status"]), (StatusCode::BAD_REQUEST, &json!("error")));
    }

    #[tokio::test]
    async fn refuses_a_filter_list_longer_than_a_hundred_values() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let query = format!("qsl_sent={}", vec!["Y"; 101].join(","));
        let (status, body) = h
            .send("GET", &h.path(&format!("/qsos?{query}")), Auth::Session(&cookie), None)
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(body["error"].as_str().unwrap().contains("at most 100"));
        assert_eq!(
            (&body["code"], &body["count"], &body["max"]),
            (&json!("too_many_values"), &json!(101), &json!(100))
        );
    }

    #[tokio::test]
    async fn accepts_native_tokens_scoped_to_the_logbook() {
        let h = Harness::new().await;
        let logbook_id = h.world.logbook.id.as_str();
        let native = h.token(&h.world.operator, &[logbook_id]).await;
        let outsider = h.session(&h.world.outsider).await;
        let admin = h.session(&h.world.admin).await;
        let uri = h.path("/qsos");

        let (status, created) = h
            .send(
                "POST",
                &uri,
                Auth::Bearer(&native),
                Some(entry(&h, &QsoId::generate(), "JA1XYZ", "40m", "0100")),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        assert_eq!(created["qso"]["created_by"], h.world.operator.id);

        for (auth, expected) in [
            (Auth::Bearer(&native), StatusCode::OK),
            (Auth::Bearer("nothing"), StatusCode::UNAUTHORIZED),
            (Auth::Anonymous, StatusCode::UNAUTHORIZED),
            (Auth::Session(&outsider), StatusCode::NOT_FOUND),
            (Auth::Session(&admin), StatusCode::NOT_FOUND),
        ] {
            let (status, body) = h.send("GET", &uri, auth, None).await;
            assert_eq!(status, expected, "{body}");
        }
    }
}
