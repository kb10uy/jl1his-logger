use std::{collections::BTreeMap, sync::Arc};

use axum::{
    extract::{FromRequestParts, Path, State},
    http::{HeaderName, StatusCode, header, request::Parts},
};
use chrono::DateTime;
use serde::{Deserialize, Serialize};

use crate::{
    domain::qso::{Qso, QsoId, Warning},
    store::{
        StoreError, WriteTx,
        access::{self, LogbookAccess},
        qso::{self, CallMatch, Created, Cursor, Filter, QsoInput, StoredQso, TimeRange},
    },
    web::{
        AppState,
        auth::Caller,
        error::ApiError,
        extract::{Json, Query},
    },
};

const MAX_LIMIT: u32 = 1000;
const DEFAULT_SEARCH_LIMIT: u32 = 100;

#[derive(Debug, Serialize)]
pub struct QsoBody {
    id: String,
    station_id: String,
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
            id: stored.id,
            station_id: stored.station_id,
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
    fields: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct ChangedQso {
    station_id: String,
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
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "If-Match takes one strong entity tag, the revision in quotes",
                )
            })
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
    cursor: Option<String>,
    limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct PageBody {
    qsos: Vec<QsoBody>,
    next: Option<String>,
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
    caller: Caller,
    Path(logbook_id): Path<String>,
    Json(req): Json<NewQso>,
) -> Result<(StatusCode, Tagged<Saved>), ApiError> {
    let id: QsoId = req.id.parse().map_err(|e| StoreError::Invalid(format!("{e}")))?;
    let input = QsoInput {
        station_id: req.station_id,
        qso: Qso::from_fields(req.fields)?,
    };
    let mut tx = WriteTx::begin(&state.db).await?;
    let access = access::logbook(&mut tx, &caller.principal, &logbook_id).await?;
    let created = qso::create(&mut tx, &access, &id, input).await?;
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
    caller: Caller,
    Path((logbook_id, id)): Path<(String, String)>,
) -> Result<Tagged<QsoBody>, ApiError> {
    let id = path_id(&id)?;
    let mut conn = state.db.acquire().await?;
    let access = access::logbook(&mut conn, &caller.principal, &logbook_id).await?;
    let stored = qso::get(&mut conn, &access, &id).await?;
    Ok(tagged(stored.revision, stored.into()))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path((logbook_id, id)): Path<(String, String)>,
    IfMatch(revision): IfMatch,
    Json(req): Json<ChangedQso>,
) -> Result<Tagged<Saved>, ApiError> {
    let id = path_id(&id)?;
    let input = QsoInput {
        station_id: req.station_id,
        qso: Qso::from_fields(req.fields)?,
    };
    let mut tx = WriteTx::begin(&state.db).await?;
    let access = access::logbook(&mut tx, &caller.principal, &logbook_id).await?;
    match qso::update(&mut tx, &access, &id, revision, input).await {
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
            Err(with_current(&state, &access, &id, e).await)
        }
    }
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path((logbook_id, id)): Path<(String, String)>,
    IfMatch(revision): IfMatch,
) -> Result<Tagged<QsoBody>, ApiError> {
    let id = path_id(&id)?;
    let mut tx = WriteTx::begin(&state.db).await?;
    let access = access::logbook(&mut tx, &caller.principal, &logbook_id).await?;
    match qso::delete(&mut tx, &access, &id, revision).await {
        Ok(stored) => {
            tx.commit().await?;
            Ok(tagged(stored.revision, stored.into()))
        }
        Err(e) => {
            drop(tx);
            Err(with_current(&state, &access, &id, e).await)
        }
    }
}

pub async fn changes(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path(logbook_id): Path<String>,
    Query(query): Query<ChangesQuery>,
) -> Result<Json<ChangesBody>, ApiError> {
    let limit = limit(query.limit, MAX_LIMIT)?;
    let mut conn = state.db.acquire().await?;
    let access = access::logbook(&mut conn, &caller.principal, &logbook_id).await?;
    let changes = qso::changes(&mut conn, &access, query.after.unwrap_or(0), limit).await?;
    Ok(Json(ChangesBody {
        cursor_generation: changes.cursor_generation,
        last_updated_seq: changes.last_updated_seq,
        qsos: changes.rows.into_iter().map(QsoBody::from).collect(),
        has_more: changes.has_more,
    }))
}

pub async fn search(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path(logbook_id): Path<String>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<PageBody>, ApiError> {
    let limit = limit(query.limit, DEFAULT_SEARCH_LIMIT)?;
    let call = match (query.call, query.call_base) {
        (Some(_), Some(_)) => return Err(StoreError::Invalid("give call or call_base, not both".into()).into()),
        (Some(call), None) => Some(CallMatch::Exact(call)),
        (None, Some(call)) => Some(CallMatch::Base(call)),
        (None, None) => None,
    };
    let filter = Filter {
        time: TimeRange {
            from: query.from.as_deref().map(|t| time("from", t)).transpose()?,
            until: query.until.as_deref().map(|t| time("until", t)).transpose()?,
        },
        station_id: query.station_id,
        station_ids: None,
        band: query.band,
        mode: query.mode,
        call,
        qsl_sent: query.qsl_sent.map(|v| v.split(',').map(str::to_owned).collect()),
        contest_id: query.contest_id,
    };
    let after = query.cursor.as_deref().map(parse_cursor).transpose()?;
    let mut conn = state.db.acquire().await?;
    let access = access::logbook(&mut conn, &caller.principal, &logbook_id).await?;
    let page = qso::search(&mut conn, &access, &filter, after.as_ref(), limit).await?;
    Ok(Json(PageBody {
        qsos: page.rows.into_iter().map(QsoBody::from).collect(),
        next: page.next.map(|c| format!("{}_{}", c.qso_time, c.id)),
    }))
}

pub async fn duplicates(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path(logbook_id): Path<String>,
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
    let mut conn = state.db.acquire().await?;
    let access = access::logbook(&mut conn, &caller.principal, &logbook_id).await?;
    let rows = qso::possible_duplicates(&mut conn, &access, &candidate, query.except.as_deref()).await?;
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

fn path_id(id: &str) -> Result<QsoId, StoreError> {
    id.parse().map_err(|_| StoreError::NotFound("QSO"))
}

fn limit(limit: Option<u32>, default: u32) -> Result<u32, StoreError> {
    match limit.unwrap_or(default) {
        0 => Err(StoreError::Invalid("limit must be at least 1".into())),
        limit => Ok(limit.min(MAX_LIMIT)),
    }
}

pub(super) fn time(name: &str, value: &str) -> Result<i64, StoreError> {
    DateTime::parse_from_rfc3339(value)
        .map(|t| t.timestamp())
        .map_err(|_| StoreError::Invalid(format!("{name} {value:?} is not an RFC 3339 time")))
}

fn parse_cursor(cursor: &str) -> Result<Cursor, StoreError> {
    cursor
        .split_once('_')
        .and_then(|(time, id)| {
            Some(Cursor {
                qso_time: time.parse().ok()?,
                id: id.to_owned(),
            })
        })
        .ok_or_else(|| StoreError::Invalid(format!("{cursor:?} is not a cursor")))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::{
        store::token::TokenKind,
        web::testing::{Auth, Harness},
    };

    fn qsos(h: &Harness) -> String {
        format!("/api/logbooks/{}/qsos", h.world.logbook.id)
    }

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
        let id = QsoId::generate();
        let (status, body) = h
            .send(
                "POST",
                &qsos(h),
                Auth::Session(cookie),
                Some(entry(h, &id, call, band, time_on)),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body
    }

    #[tokio::test]
    async fn creates_a_qso_once_per_id() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let id = QsoId::generate();

        let (status, created) = h
            .send(
                "POST",
                &qsos(&h),
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
                &qsos(&h),
                Auth::Session(&cookie),
                Some(entry(&h, &id, "JA1ZZZ", "40m", "050000")),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(again["qso"], created["qso"]);

        let (status, headers, fetched) = h
            .send_with("GET", &format!("{}/{id}", qsos(&h)), Auth::Session(&cookie), &[], None)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::ETAG], "\"1\"");
        assert_eq!(fetched, created["qso"]);
    }

    #[tokio::test]
    async fn refuses_malformed_ids() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let mut body = entry(&h, &QsoId::generate(), "JA1XYZ", "40m", "040506");
        body["id"] = json!("6f1c2a3e-4b5d-4e6f-8a7b-0123456789ab");
        let (status, _) = h.send("POST", &qsos(&h), Auth::Session(&cookie), Some(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) = h
            .send("GET", &format!("{}/not-a-uuid", qsos(&h)), Auth::Session(&cookie), None)
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn reports_invalid_qsos() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let mut body = entry(&h, &QsoId::generate(), "JA1XYZ", "40m", "040506");
        body["fields"]["BAND"] = json!("41m");
        let (status, reply) = h.send("POST", &qsos(&h), Auth::Session(&cookie), Some(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(reply["status"], "error");
        assert!(reply["error"].as_str().unwrap().contains("BAND"), "{reply}");

        let (status, reply) = h
            .send("POST", &qsos(&h), Auth::Session(&cookie), Some(json!({ "id": 1 })))
            .await;
        assert!(status.is_client_error(), "{status}");
        assert_eq!(reply["status"], "error");

        let (status, reply) = h
            .send("GET", &format!("{}?limit=many", qsos(&h)), Auth::Session(&cookie), None)
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(reply["status"], "error");
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

        let uri = format!(
            "/api/logbooks/{}/duplicates?call=JA1XYZ&band=40m&mode=cw&qso_date=20261022&time_on=0415",
            h.world.logbook.id
        );
        let (status, body) = h.send("GET", &uri, Auth::Session(&cookie), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, json!({ "qsos": [first["qso"], second["qso"]] }));

        let except = format!("{uri}&except={}", first_id.as_str().unwrap());
        let (_, body) = h.send("GET", &except, Auth::Session(&cookie), None).await;
        assert_eq!(body, json!({ "qsos": [second["qso"]] }));

        let incomplete = format!("/api/logbooks/{}/duplicates?call=JA1XYZ", h.world.logbook.id);
        let (status, _) = h.send("GET", &incomplete, Auth::Session(&cookie), None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn updates_at_the_revision_in_if_match() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let created = create(&h, &cookie, "JA1XYZ", "40m", "040506").await;
        let uri = format!("{}/{}", qsos(&h), created["qso"]["id"].as_str().unwrap());
        let mut fields = created["qso"]["fields"].clone();
        fields["NAME"] = json!("Hanako");
        fields["COMMENT"] = json!("tnx");
        let change = json!({ "station_id": h.station.id, "fields": fields });
        let put = |if_match: &'static [(&'static str, &'static str)]| {
            h.send_with("PUT", &uri, Auth::Session(&cookie), if_match, Some(change.clone()))
        };

        let (status, _, _) = put(&[]).await;
        assert_eq!(status, StatusCode::PRECONDITION_REQUIRED);
        for malformed in [&[("if-match", "1")], &[("if-match", "W/\"1\"")], &[("if-match", "*")]] {
            let (status, _, _) = put(malformed).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{malformed:?}");
        }

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

        let (status, _, deleted) = h
            .send_with("DELETE", &uri, Auth::Session(&cookie), &[("if-match", "\"2\"")], None)
            .await;
        assert_eq!(status, StatusCode::OK);
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
        let uri = format!("{}/{}", qsos(&h), gone["qso"]["id"].as_str().unwrap());

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

        let changes = format!("/api/logbooks/{}/changes", h.world.logbook.id);
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

        let (_, page) = h.send("GET", &qsos(&h), Auth::Session(&cookie), None).await;
        assert_eq!(page["qsos"], json!([kept["qso"]]));
    }

    #[tokio::test]
    async fn searches_newest_first_with_a_cursor() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let early = create(&h, &cookie, "JA1XYZ", "40m", "0100").await;
        let middle = create(&h, &cookie, "JA1XYZ/P", "20m", "0200").await;
        let late = create(&h, &cookie, "JA1ABC", "40m", "0300").await;

        let (status, first) = h
            .send("GET", &format!("{}?limit=2", qsos(&h)), Auth::Session(&cookie), None)
            .await;
        assert_eq!(status, StatusCode::OK, "{first}");
        assert_eq!(first["qsos"], json!([late["qso"], middle["qso"]]));
        let cursor = first["next"].as_str().unwrap();
        let (_, second) = h
            .send(
                "GET",
                &format!("{}?limit=2&cursor={cursor}", qsos(&h)),
                Auth::Session(&cookie),
                None,
            )
            .await;
        assert_eq!(second["qsos"], json!([early["qso"]]));
        assert_eq!(second["next"], Value::Null);

        for (query, expected) in [
            ("band=20M", vec![&middle]),
            ("call=ja1xyz", vec![&early]),
            ("call_base=JA1XYZ", vec![&middle, &early]),
            ("from=2026-10-22T01:30:00Z&until=2026-10-22T03:00:00Z", vec![&middle]),
            ("qsl_sent=", vec![&late, &middle, &early]),
            ("qsl_sent=Y", vec![]),
        ] {
            let (status, page) = h
                .send("GET", &format!("{}?{query}", qsos(&h)), Auth::Session(&cookie), None)
                .await;
            assert_eq!(status, StatusCode::OK, "{query}: {page}");
            let expected: Vec<&Value> = expected.iter().map(|q| &q["qso"]).collect();
            assert_eq!(page["qsos"], json!(expected), "{query}");
        }

        for query in ["call=A&call_base=A", "from=20261022", "cursor=nope", "limit=0"] {
            let (status, _) = h
                .send("GET", &format!("{}?{query}", qsos(&h)), Auth::Session(&cookie), None)
                .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
        }
    }

    #[tokio::test]
    async fn accepts_native_tokens_scoped_to_the_logbook() {
        let h = Harness::new().await;
        let logbook_id = h.world.logbook.id.as_str();
        let native = h.token(&h.world.operator, TokenKind::Native, &[logbook_id]).await;
        let wl2 = h.token(&h.world.operator, TokenKind::Wl2, &[logbook_id]).await;
        let outsider = h.session(&h.world.outsider).await;
        let admin = h.session(&h.world.admin).await;
        let uri = qsos(&h);

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
            (Auth::Bearer(&wl2), StatusCode::UNAUTHORIZED),
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
