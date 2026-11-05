use std::{collections::BTreeMap, sync::Arc};

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use base64::{Engine, prelude::BASE64_STANDARD};
use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    domain::{
        adif::{Encoding, LengthUnit, Output, Unencodable, UnencodableError},
        qso::QsoId,
        split,
    },
    store::{
        WriteTx,
        bulk::{self, Change, Outcome, SkipReason, Skipped},
        export::{self, ExportBatch, ExportKind, ExportState},
        new_id,
        qso::{Filter, Target},
    },
    web::{
        AppState,
        api::logbook::member::{Item, Member},
        api::qso::{QsoBody, limit},
        error::ApiError,
        extract::{Json, Query},
        query::text,
    },
};

const DEFAULT_LIST_LIMIT: u32 = 10;

#[derive(Debug, Deserialize)]
pub struct TargetRequest {
    id: String,
    revision: i64,
}

#[derive(Debug, Serialize)]
pub struct TargetBody {
    id: String,
    revision: i64,
}

impl From<Target> for TargetBody {
    fn from(target: Target) -> TargetBody {
        TargetBody {
            id: target.id.to_string(),
            revision: target.revision,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct BulkRequest {
    qsos: Vec<TargetRequest>,
    station_id: Option<String>,
    #[serde(default, deserialize_with = "present")]
    instrument_id: Option<Option<String>>,
    #[serde(default)]
    fields: BTreeMap<String, String>,
}

fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Serialize)]
pub struct SkippedBody {
    id: String,
    reason: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_revision: Option<i64>,
}

impl From<Skipped> for SkippedBody {
    fn from(skipped: Skipped) -> SkippedBody {
        let (reason, current_revision) = match skipped.reason {
            SkipReason::Missing => ("missing", None),
            SkipReason::Deleted => ("deleted", None),
            SkipReason::Revision { current } => ("revision", Some(current)),
        };
        SkippedBody {
            id: skipped.id.to_string(),
            reason,
            current_revision,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct OutcomeBody {
    updated: Vec<QsoBody>,
    skipped: Vec<SkippedBody>,
}

impl From<Outcome> for OutcomeBody {
    fn from(outcome: Outcome) -> OutcomeBody {
        OutcomeBody {
            updated: outcome.updated.into_iter().map(QsoBody::from).collect(),
            skipped: outcome.skipped.into_iter().map(SkippedBody::from).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BatchBody {
    id: String,
    export_id: String,
    kind: ExportKind,
    created_by: String,
    created_at: String,
    applied_at: Option<String>,
    dismissed_at: Option<String>,
    qso_count: i64,
    label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    qsos: Option<Vec<TargetBody>>,
}

impl From<ExportBatch> for BatchBody {
    fn from(batch: ExportBatch) -> BatchBody {
        BatchBody {
            id: batch.id,
            export_id: batch.export_id,
            kind: batch.kind,
            created_by: batch.created_by,
            created_at: batch.created_at,
            applied_at: batch.applied_at,
            dismissed_at: batch.dismissed_at,
            qso_count: batch.qso_count,
            label: batch.label,
            qsos: None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ExportRequest {
    kind: ExportKind,
    q: Option<String>,
    park: Option<String>,
    length: Option<LengthUnit>,
    #[serde(default)]
    encoding: Encoding,
    #[serde(default)]
    unencodable: Unencodable,
}

#[derive(Debug, Serialize)]
pub struct ExportBody {
    batch: Option<BatchBody>,
    records: usize,
    adif: Option<String>,
    file: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SplitRequest {
    split: Vec<String>,
    #[serde(flatten)]
    export: ExportRequest,
}

#[derive(Debug, Serialize)]
pub struct FileBody {
    name: String,
    label: String,
    missing: Vec<String>,
    records: usize,
    batch: BatchBody,
}

#[derive(Debug, Serialize)]
pub struct SplitBody {
    records: usize,
    format: Option<&'static str>,
    file: Option<String>,
    files: Vec<FileBody>,
}

impl ExportRequest {
    fn output(&self, state: &AppState) -> Result<Output, ApiError> {
        let unit = self
            .encoding
            .export_unit(self.length, state.config.adif.length)
            .ok_or_else(|| {
                ApiError::bad_request(format!("lengths in {} are counted in bytes", self.encoding.name()))
            })?;
        Ok(Output {
            unit,
            encoding: self.encoding,
            unencodable: self.unencodable,
        })
    }

    async fn select(&self, state: &AppState, member: &Member) -> Result<export::Selection, ApiError> {
        let filter = Filter {
            query: text(self.q.as_deref())?,
            ..Filter::default()
        };
        let mut conn = state.db.acquire().await?;
        Ok(export::select(&mut conn, &member.access, self.kind, &filter, self.park.as_deref()).await?)
    }
}

fn unencodable(e: UnencodableError) -> ApiError {
    ApiError::bad_request(e.to_string())
        .with("code", "unencodable".into())
        .with("record", e.record.into())
        .with("field", e.field.clone().into())
        .with("character", e.character.to_string().into())
}

fn targets(requests: Vec<TargetRequest>) -> Result<Vec<Target>, ApiError> {
    requests
        .into_iter()
        .map(|t| {
            Ok(Target {
                id: t
                    .id
                    .parse::<QsoId>()
                    .map_err(|e| ApiError::bad_request(e.to_string()))?,
                revision: t.revision,
            })
        })
        .collect()
}

pub async fn bulk(
    State(state): State<Arc<AppState>>,
    member: Member,
    Json(req): Json<BulkRequest>,
) -> Result<Json<OutcomeBody>, ApiError> {
    let targets = targets(req.qsos)?;
    let change = Change {
        station_id: req.station_id,
        instrument_id: req.instrument_id,
        fields: req.fields,
    };
    let mut tx = WriteTx::begin(&state.db).await?;
    let outcome = bulk::update(&mut tx, &member.access, &targets, &change).await?;
    tx.commit().await?;
    Ok(Json(outcome.into()))
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    state: Option<ExportState>,
    after: Option<String>,
    limit: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ListBody {
    Every(Vec<BatchBody>),
    Page {
        batches: Vec<BatchBody>,
        next: Option<String>,
        total: i64,
    },
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    member: Member,
    Query(query): Query<ListQuery>,
) -> Result<Json<ListBody>, ApiError> {
    let Some(listed) = query.state else {
        if query.after.is_some() || query.limit.is_some() {
            return Err(ApiError::bad_request("after and limit need a state"));
        }
        let mut conn = state.db.acquire().await?;
        let batches = export::list(&mut conn, &member.access).await?;
        return Ok(Json(ListBody::Every(
            batches.into_iter().map(BatchBody::from).collect(),
        )));
    };
    let limit = limit(query.limit, DEFAULT_LIST_LIMIT)?;
    let mut conn = state.db.acquire().await?;
    let page = export::page(&mut conn, &member.access, listed, query.after.as_deref(), limit).await?;
    Ok(Json(ListBody::Page {
        batches: page.batches.into_iter().map(BatchBody::from).collect(),
        next: page.next,
        total: page.total,
    }))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(item): Path<Item>,
) -> Result<Json<BatchBody>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let (batch, targets) = export::get(&mut conn, &member.access, &item.id).await?;
    Ok(Json(BatchBody {
        qsos: Some(targets.into_iter().map(TargetBody::from).collect()),
        ..batch.into()
    }))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    member: Member,
    Json(req): Json<ExportRequest>,
) -> Result<(StatusCode, Json<ExportBody>), ApiError> {
    let output = req.output(&state)?;
    let selection = req.select(&state, &member).await?;
    if selection.targets.is_empty() {
        return Ok((
            StatusCode::OK,
            Json(ExportBody {
                batch: None,
                records: 0,
                adif: None,
                file: None,
            }),
        ));
    }
    let adif = export::encoded(&selection.records, output).map_err(unencodable)?;
    let file = matches!(req.encoding, Encoding::ShiftJis | Encoding::Windows1252)
        .then(|| BASE64_STANDARD.encode(req.encoding.encode(&adif)));
    let mut tx = WriteTx::begin(&state.db).await?;
    let batch = export::create(&mut tx, &member.access, req.kind, &selection.targets, None, &new_id()).await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(ExportBody {
            batch: Some(batch.into()),
            records: selection.records.len(),
            adif: Some(adif),
            file,
        }),
    ))
}

pub async fn create_split(
    State(state): State<Arc<AppState>>,
    member: Member,
    Json(req): Json<SplitRequest>,
) -> Result<(StatusCode, Json<SplitBody>), ApiError> {
    if req.split.is_empty() {
        return Err(ApiError::bad_request("a split names at least one field"));
    }
    let fields = split::fields(&req.split).map_err(|e| ApiError::bad_request(e.to_string()))?;
    let output = req.export.output(&state)?;
    let selection = req.export.select(&state, &member).await?;
    let parts = export::split(&selection, &fields);
    if parts.is_empty() {
        return Ok((
            StatusCode::OK,
            Json(SplitBody {
                records: 0,
                format: None,
                file: None,
                files: vec![],
            }),
        ));
    }
    let labels: Vec<String> = parts.iter().map(|p| p.label.clone()).collect();
    let names = split::file_names(&labels, "adi");
    let mut files = Vec::with_capacity(parts.len());
    for (part, name) in parts.iter().zip(&names) {
        let adif =
            export::encoded(&part.records, output).map_err(|e| unencodable(e).with("file", name.clone().into()))?;
        files.push((name.clone(), req.export.encoding.encode(&adif).into_owned()));
    }
    let mut tx = WriteTx::begin(&state.db).await?;
    let mut bodies = Vec::with_capacity(parts.len());
    let export_id = new_id();
    for (part, name) in parts.iter().zip(&names) {
        let batch = export::create(
            &mut tx,
            &member.access,
            req.export.kind,
            &part.targets,
            Some(&part.label),
            &export_id,
        )
        .await?;
        bodies.push(FileBody {
            name: name.clone(),
            label: part.label.clone(),
            missing: part.missing.clone(),
            records: part.records.len(),
            batch: batch.into(),
        });
    }
    tx.commit().await?;
    let (format, bytes) = match files.len() {
        1 => ("adi", files.pop().expect("one file").1),
        _ => ("zip", split::archive(&files)),
    };
    Ok((
        StatusCode::CREATED,
        Json(SplitBody {
            records: selection.records.len(),
            format: Some(format),
            file: Some(BASE64_STANDARD.encode(bytes)),
            files: bodies,
        }),
    ))
}

pub async fn apply(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(item): Path<Item>,
) -> Result<Json<OutcomeBody>, ApiError> {
    let mut tx = WriteTx::begin(&state.db).await?;
    let outcome = export::apply(&mut tx, &member.access, &item.id).await?;
    tx.commit().await?;
    Ok(Json(outcome.into()))
}

pub async fn dismiss(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(item): Path<Item>,
) -> Result<Json<BatchBody>, ApiError> {
    let mut tx = WriteTx::begin(&state.db).await?;
    let batch = export::dismiss(&mut tx, &member.access, &item.id).await?;
    tx.commit().await?;
    Ok(Json(batch.into()))
}

pub async fn undismiss(
    State(state): State<Arc<AppState>>,
    member: Member,
    Path(item): Path<Item>,
) -> Result<Json<BatchBody>, ApiError> {
    let mut tx = WriteTx::begin(&state.db).await?;
    let batch = export::undismiss(&mut tx, &member.access, &item.id).await?;
    tx.commit().await?;
    Ok(Json(batch.into()))
}

#[derive(Debug, Deserialize)]
pub struct ExportIdRequest {
    export_id: String,
}

#[derive(Debug, Serialize)]
pub struct ExportOutcomeBody {
    batches: usize,
    #[serde(flatten)]
    outcome: OutcomeBody,
}

pub async fn apply_export(
    State(state): State<Arc<AppState>>,
    member: Member,
    Json(req): Json<ExportIdRequest>,
) -> Result<Json<ExportOutcomeBody>, ApiError> {
    let mut tx = WriteTx::begin(&state.db).await?;
    let result = export::apply_export(&mut tx, &member.access, &req.export_id).await?;
    tx.commit().await?;
    Ok(Json(ExportOutcomeBody {
        batches: result.batches,
        outcome: result.outcome.into(),
    }))
}

#[derive(Debug, Serialize)]
pub struct DismissedBody {
    batches: usize,
}

pub async fn dismiss_export(
    State(state): State<Arc<AppState>>,
    member: Member,
    Json(req): Json<ExportIdRequest>,
) -> Result<Json<DismissedBody>, ApiError> {
    let mut tx = WriteTx::begin(&state.db).await?;
    let batches = export::dismiss_export(&mut tx, &member.access, &req.export_id).await?;
    tx.commit().await?;
    Ok(Json(DismissedBody { batches }))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serde_json::{Value, json};

    use super::*;
    use crate::web::testing::{Auth, Harness};

    async fn create_qso(h: &Harness, cookie: &str, call: &str, time_on: &str) -> Value {
        h.create_qso(
            cookie,
            &h.station.id,
            call,
            time_on,
            &[("NAME", "太郎"), ("QSL_SENT", "R")],
        )
        .await["qso"]
            .clone()
    }

    #[tokio::test]
    async fn changes_fields_of_rows_at_their_revisions() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let first = create_qso(&h, &cookie, "JA1XYZ", "0100").await;
        let second = create_qso(&h, &cookie, "JA1ABC", "0200").await;
        let missing = QsoId::generate().to_string();

        let request = json!({
            "qsos": [
                { "id": first["id"], "revision": 1 },
                { "id": second["id"], "revision": 9 },
                { "id": missing, "revision": 1 },
            ],
            "fields": { "qsl_sent": "y", "QSLSDATE": "20261023", "NAME": "" },
        });
        let (status, outcome) = h
            .send("POST", &h.path("/bulk"), Auth::Session(&cookie), Some(request))
            .await;
        assert_eq!(status, StatusCode::OK, "{outcome}");
        assert_eq!(outcome["updated"][0]["fields"]["QSL_SENT"], "y");
        assert_eq!(outcome["updated"][0]["fields"]["QSLSDATE"], "20261023");
        assert_eq!(outcome["updated"][0]["fields"].get("NAME"), None);
        assert_eq!(outcome["updated"][0]["revision"], 2);
        assert_eq!(
            outcome["skipped"],
            json!([
                { "id": second["id"], "reason": "revision", "current_revision": 1 },
                { "id": missing, "reason": "missing" },
            ])
        );
    }

    #[rstest]
    #[case::required_cleared(json!({ "CALL": "" }), json!(null))]
    #[case::invalid_value(json!({ "QSL_SENT": "Z" }), json!(null))]
    #[case::no_field(json!({}), json!(null))]
    #[case::invalid_id(json!({ "QSL_SENT": "Y" }), json!("not-a-uuid"))]
    #[tokio::test]
    async fn refuses_changes_it_cannot_apply(#[case] fields: Value, #[case] id: Value) {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let created = create_qso(&h, &cookie, "JA1XYZ", "0100").await;
        let id = match id {
            Value::Null => created["id"].clone(),
            id => id,
        };
        let request = json!({ "qsos": [{ "id": id, "revision": 1 }], "fields": fields });
        let (status, body) = h
            .send("POST", &h.path("/bulk"), Auth::Session(&cookie), Some(request))
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }

    #[tokio::test]
    async fn keeps_the_instrument_unless_the_request_names_one_or_null() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let rig = h.world.instrument("IC-705").await;
        let created = create_qso(&h, &cookie, "JA1XYZ", "0100").await;
        let mut expected = vec![];
        for (revision, change) in [
            (1, json!({ "instrument_id": rig.id })),
            (2, json!({ "fields": { "NAME": "Hanako" } })),
            (3, json!({ "instrument_id": null })),
        ] {
            let mut request = change;
            request["qsos"] = json!([{ "id": created["id"], "revision": revision }]);
            let (status, outcome) = h
                .send("POST", &h.path("/bulk"), Auth::Session(&cookie), Some(request))
                .await;
            assert_eq!(status, StatusCode::OK, "{outcome}");
            let updated = &outcome["updated"][0];
            expected.push((updated["instrument_id"].clone(), updated["fields"]["MY_RIG"].clone()));
        }
        assert_eq!(
            expected,
            [
                (json!(rig.id), json!("IC-705")),
                (json!(rig.id), json!("IC-705")),
                (Value::Null, Value::Null),
            ]
        );
    }

    #[tokio::test]
    async fn exports_a_batch_and_applies_it_once() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let first = create_qso(&h, &cookie, "JA1XYZ", "0100").await;
        let second = create_qso(&h, &cookie, "JA1ABC", "0200").await;
        let exports = h.path("/exports");

        let request = json!({ "kind": "qsl_card", "q": "sent:r,Q" });
        let (status, exported) = h
            .send("POST", &exports, Auth::Session(&cookie), Some(request.clone()))
            .await;
        assert_eq!(status, StatusCode::CREATED, "{exported}");
        assert_eq!(exported["records"], 2);
        assert_eq!(exported["batch"]["kind"], "qsl_card");
        assert_eq!(exported["batch"]["qso_count"], 2);
        assert_eq!(exported["batch"]["created_by"], h.world.operator.id);
        assert!(exported["batch"].get("qsos").is_none());
        let adif = exported["adif"].as_str().unwrap();
        assert!(adif.contains("<PROGRAMID:13>jl1his-logger"), "{adif}");
        assert!(adif.contains("<NAME:2>太郎"), "{adif}");
        assert!(adif.find("JA1XYZ").unwrap() < adif.find("JA1ABC").unwrap());

        let batch_uri = format!("{exports}/{}", exported["batch"]["id"].as_str().unwrap());
        let (_, list) = h.send("GET", &exports, Auth::Session(&cookie), None).await;
        assert_eq!(list, json!([exported["batch"]]));
        let (status, batch) = h.send("GET", &batch_uri, Auth::Session(&cookie), None).await;
        assert_eq!(status, StatusCode::OK);
        let mut recorded: Vec<Value> = vec![
            json!({ "id": first["id"], "revision": 1 }),
            json!({ "id": second["id"], "revision": 1 }),
        ];
        recorded.sort_by_key(|t| t["id"].as_str().unwrap().to_owned());
        assert_eq!(batch["qsos"], json!(recorded));

        let (status, applied) = h
            .send("POST", &format!("{batch_uri}/apply"), Auth::Session(&cookie), None)
            .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert_eq!(applied["updated"].as_array().unwrap().len(), 2);
        assert_eq!(applied["updated"][0]["fields"]["QSL_SENT"], "Y");
        assert_eq!(applied["skipped"], json!([]));
        let (status, _) = h
            .send("POST", &format!("{batch_uri}/apply"), Auth::Session(&cookie), None)
            .await;
        assert_eq!(status, StatusCode::CONFLICT);

        let (status, nothing) = h.send("POST", &exports, Auth::Session(&cookie), Some(request)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            nothing,
            json!({ "batch": null, "records": 0, "adif": null, "file": null })
        );
    }

    #[tokio::test]
    async fn splits_an_export_into_files_and_batches() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let station = h.station.id.clone();
        let portable = &[("STATION_CALLSIGN", "JL1HIS/P"), ("NAME", "太郎")];
        let home = &[("STATION_CALLSIGN", "JL1HIS")];
        h.create_qso(&cookie, &station, "JA1XYZ", "0100", portable).await;
        h.create_qso(&cookie, &station, "JA1ABC", "0200", home).await;
        h.create_qso(&cookie, &station, "JA1DEF", "0300", home).await;
        let split = h.path("/exports/split");
        let send = |body: Value| h.send("POST", &split, Auth::Session(&cookie), Some(body));

        let (status, exported) = send(json!({ "kind": "lotw", "split": ["station_callsign"] })).await;
        assert_eq!(status, StatusCode::CREATED, "{exported}");
        assert_eq!((&exported["records"], &exported["format"]), (&json!(3), &json!("zip")));
        let files = exported["files"].as_array().unwrap();
        assert_eq!(
            files
                .iter()
                .map(|f| (
                    f["name"].as_str().unwrap(),
                    f["label"].as_str().unwrap(),
                    f["records"].as_u64().unwrap()
                ))
                .collect::<Vec<_>>(),
            [("JL1HIS.adi", "JL1HIS", 2), ("JL1HIS_P.adi", "JL1HIS/P", 1)]
        );
        assert_eq!(files[1]["batch"]["label"], "JL1HIS/P");
        assert_eq!(files[1]["batch"]["qso_count"], 1);
        assert_eq!(files[1]["batch"]["kind"], "lotw");
        let bytes = BASE64_STANDARD.decode(exported["file"].as_str().unwrap()).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let names: Vec<_> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().unwrap().into_owned())
            .collect();
        assert_eq!(names, ["JL1HIS.adi", "JL1HIS_P.adi"]);
        let mut text = String::new();
        std::io::Read::read_to_string(&mut archive.by_name("JL1HIS_P.adi").unwrap(), &mut text).unwrap();
        assert!(text.contains("<CALL:6>JA1XYZ") && !text.contains("JA1ABC"), "{text}");

        let (_, list) = h.send("GET", &h.path("/exports"), Auth::Session(&cookie), None).await;
        let labels: Vec<_> = list.as_array().unwrap().iter().map(|b| b["label"].clone()).collect();
        assert_eq!(labels, [json!("JL1HIS/P"), json!("JL1HIS")]);

        let (status, by_grid) = send(json!({ "kind": "adif", "split": ["STATION_CALLSIGN", "MY_GRIDSQUARE"] })).await;
        assert_eq!(status, StatusCode::CREATED, "{by_grid}");
        let files = by_grid["files"].as_array().unwrap();
        assert_eq!(files[0]["name"], "JL1HIS_none.adi");
        assert_eq!(files[0]["missing"], json!(["MY_GRIDSQUARE"]));

        let (status, one) = send(json!({ "kind": "adif", "split": ["STATION_CALLSIGN"], "q": "call:JA1XYZ" })).await;
        assert_eq!(status, StatusCode::CREATED, "{one}");
        assert_eq!(one["format"], "adi");
        assert_eq!(one["files"].as_array().unwrap().len(), 1);
        assert_eq!(one["files"][0]["missing"], json!([]));
        let adi = BASE64_STANDARD.decode(one["file"].as_str().unwrap()).unwrap();
        assert!(
            std::str::from_utf8(&adi)
                .unwrap()
                .contains("<STATION_CALLSIGN:8>JL1HIS/P")
        );

        let (status, none) = send(json!({ "kind": "adif", "split": ["STATION_CALLSIGN"], "q": "call:JA9ZZZ" })).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(none, json!({ "records": 0, "format": null, "file": null, "files": [] }));

        for body in [
            json!({ "kind": "adif", "split": [] }),
            json!({ "kind": "adif", "split": ["1X"] }),
        ] {
            let (status, _) = send(body).await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }
        let (status, refused) =
            send(json!({ "kind": "adif", "split": ["STATION_CALLSIGN"], "encoding": "ascii" })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            (&refused["file"], &refused["record"]),
            (&json!("JL1HIS_P.adi"), &json!(1))
        );
        let (_, list) = h.send("GET", &h.path("/exports"), Auth::Session(&cookie), None).await;
        assert_eq!(list.as_array().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn dismisses_batches_and_handles_whole_exports() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let station = h.station.id.clone();
        h.create_qso(&cookie, &station, "JA1XYZ", "0100", &[("STATION_CALLSIGN", "JL1HIS/P")])
            .await;
        h.create_qso(&cookie, &station, "JA1ABC", "0200", &[("STATION_CALLSIGN", "JL1HIS")])
            .await;
        h.create_qso(&cookie, &station, "JA1DEF", "0300", &[("STATION_CALLSIGN", "JL1HIS")])
            .await;
        let exports = h.path("/exports");
        let send = async |method, path: String, body| h.send(method, &path, Auth::Session(&cookie), body).await;

        let (status, exported) = send(
            "POST",
            format!("{exports}/split"),
            Some(json!({ "kind": "lotw", "split": ["STATION_CALLSIGN"] })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{exported}");
        let files = exported["files"].as_array().unwrap();
        let export_id = files[0]["batch"]["export_id"].as_str().unwrap().to_owned();
        assert_eq!(files[1]["batch"]["export_id"], export_id);
        let home = format!("{exports}/{}", files[0]["batch"]["id"].as_str().unwrap());

        let (status, dismissed) = send("POST", format!("{home}/dismiss"), None).await;
        assert_eq!(status, StatusCode::OK, "{dismissed}");
        assert!(dismissed["dismissed_at"].is_string());
        let (status, refused) = send("POST", format!("{home}/apply"), None).await;
        assert_eq!(
            (status, &refused["code"]),
            (StatusCode::CONFLICT, &json!("batch_dismissed"))
        );
        let (status, _) = send("POST", format!("{home}/dismiss"), None).await;
        assert_eq!(status, StatusCode::CONFLICT);
        let (status, restored) = send("DELETE", format!("{home}/dismiss"), None).await;
        assert_eq!(status, StatusCode::OK, "{restored}");
        assert_eq!(restored["dismissed_at"], Value::Null);

        let (status, applied) = send(
            "POST",
            format!("{exports}/apply"),
            Some(json!({ "export_id": export_id })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert_eq!(applied["batches"], 2);
        assert_eq!(applied["updated"].as_array().unwrap().len(), 3);
        assert_eq!(applied["skipped"], json!([]));
        let (status, again) = send(
            "POST",
            format!("{exports}/apply"),
            Some(json!({ "export_id": export_id })),
        )
        .await;
        assert_eq!((status, &again["batches"]), (StatusCode::OK, &json!(0)));
        let (status, _) = send(
            "POST",
            format!("{exports}/dismiss"),
            Some(json!({ "export_id": "nowhere" })),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, plain) = send("POST", exports.clone(), Some(json!({ "kind": "adif" }))).await;
        assert_eq!(status, StatusCode::CREATED, "{plain}");
        let batch = format!("{exports}/{}", plain["batch"]["id"].as_str().unwrap());
        let (status, _) = send("POST", format!("{batch}/dismiss"), None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, none) = send(
            "POST",
            format!("{exports}/dismiss"),
            Some(json!({ "export_id": plain["batch"]["export_id"] })),
        )
        .await;
        assert_eq!((status, &none["batches"]), (StatusCode::OK, &json!(0)));
    }

    #[tokio::test]
    async fn lists_whole_exports_by_state_a_page_at_a_time() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let station = h.station.id.clone();
        h.create_qso(&cookie, &station, "JA1XYZ", "0100", &[("STATION_CALLSIGN", "JL1HIS/P")])
            .await;
        h.create_qso(&cookie, &station, "JA1ABC", "0200", &[("STATION_CALLSIGN", "JL1HIS")])
            .await;
        let exports = h.path("/exports");
        let send = async |method, path: String, body| {
            let (status, reply) = h.send(method, &path, Auth::Session(&cookie), body).await;
            assert!(status.is_success(), "{reply}");
            reply
        };
        let list = async |query: String| {
            let page = send("GET", format!("{exports}?{query}"), None).await;
            let batches = page["batches"].as_array().unwrap();
            let ids: Vec<Value> = batches.iter().map(|b| b["export_id"].clone()).collect();
            (ids, page["next"].clone(), page["total"].clone())
        };

        let split = json!({ "kind": "lotw", "split": ["STATION_CALLSIGN"] });
        let split = send("POST", format!("{exports}/split"), Some(split)).await;
        let eqsl = send("POST", exports.clone(), Some(json!({ "kind": "eqsl" }))).await;
        let adif = send("POST", exports.clone(), Some(json!({ "kind": "adif" }))).await;
        let card = send("POST", exports.clone(), Some(json!({ "kind": "qsl_card" }))).await;
        let split_id = split["files"][0]["batch"]["export_id"].clone();
        let eqsl_id = eqsl["batch"]["export_id"].clone();
        let adif_id = adif["batch"]["export_id"].clone();
        let card_id = card["batch"]["export_id"].clone();

        assert_eq!(
            list("state=pending&limit=2".into()).await,
            (vec![card_id.clone(), eqsl_id.clone()], eqsl_id.clone(), json!(3))
        );
        let after = eqsl_id.as_str().unwrap();
        assert_eq!(
            list(format!("state=pending&limit=1&after={after}")).await,
            (vec![split_id.clone(), split_id.clone()], Value::Null, json!(3))
        );
        assert_eq!(
            list("state=finished".into()).await,
            (vec![adif_id.clone()], Value::Null, json!(1))
        );

        let card_uri = format!("{exports}/{}", card["batch"]["id"].as_str().unwrap());
        send("POST", format!("{card_uri}/dismiss"), None).await;
        assert_eq!(list("state=pending".into()).await.2, json!(2));
        assert_eq!(
            list("state=finished".into()).await.0,
            [card_id.clone(), adif_id.clone()]
        );
        send("DELETE", format!("{card_uri}/dismiss"), None).await;
        assert_eq!(list("state=pending".into()).await.2, json!(3));

        let first = split["files"][0]["batch"]["id"].as_str().unwrap();
        let second = split["files"][1]["batch"]["id"].as_str().unwrap();
        send("POST", format!("{exports}/{first}/apply"), None).await;
        assert_eq!(list("state=pending".into()).await.2, json!(3));
        send("POST", format!("{exports}/{second}/dismiss"), None).await;
        assert_eq!(list("state=pending".into()).await.0, [card_id, eqsl_id]);
        assert_eq!(list("state=finished".into()).await.2, json!(2));

        let every = send("GET", exports.clone(), None).await;
        assert_eq!(every.as_array().unwrap().len(), 5);
    }

    #[rstest]
    #[case::unknown_state("state=applied")]
    #[case::no_rows("state=pending&limit=0")]
    #[case::limit_without_state("limit=10")]
    #[case::after_without_state("after=0193")]
    #[tokio::test]
    async fn refuses_lists_it_cannot_page(#[case] query: &str) {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let path = format!("{}?{query}", h.path("/exports"));
        let (status, _) = h.send("GET", &path, Auth::Session(&cookie), None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn exports_in_another_encoding() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        create_qso(&h, &cookie, "JA1XYZ", "0100").await;
        let exports = h.path("/exports");
        let export = |body: Value| h.send("POST", &exports, Auth::Session(&cookie), Some(body));

        let (status, exported) = export(json!({ "kind": "adif", "encoding": "shift_jis" })).await;
        assert_eq!(status, StatusCode::CREATED, "{exported}");
        let adif = exported["adif"].as_str().unwrap();
        assert!(adif.contains("<NAME:4>太郎 "), "{adif}");
        let file = BASE64_STANDARD.decode(exported["file"].as_str().unwrap()).unwrap();
        assert_eq!(file, Encoding::ShiftJis.encode(adif).into_owned());

        let (status, refused) = export(json!({ "kind": "adif", "encoding": "ascii" })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            (
                &refused["code"],
                &refused["record"],
                &refused["field"],
                &refused["character"]
            ),
            (&json!("unencodable"), &json!(1), &json!("NAME"), &json!("太"))
        );
        let (status, entities) = export(json!({ "kind": "adif", "encoding": "ascii", "unencodable": "entity" })).await;
        assert_eq!(status, StatusCode::CREATED);
        assert!(
            entities["adif"]
                .as_str()
                .unwrap()
                .contains("<NAME:16>&#22826;&#37070; ")
        );
        assert_eq!(entities["file"], Value::Null);

        let (status, _) = export(json!({ "kind": "adif", "encoding": "shift_jis", "length": "codepoints" })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (_, list) = h.send("GET", &exports, Auth::Session(&cookie), None).await;
        assert_eq!(list.as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn exports_with_the_requested_lengths_and_parks() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        create_qso(&h, &cookie, "JA1XYZ", "0100").await;
        let exports = h.path("/exports");

        let (status, exported) = h
            .send(
                "POST",
                &exports,
                Auth::Session(&cookie),
                Some(json!({ "kind": "adif", "length": "bytes" })),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED);
        assert!(exported["adif"].as_str().unwrap().contains("<NAME:6>太郎"));

        let (status, _) = h
            .send(
                "POST",
                &exports,
                Auth::Session(&cookie),
                Some(json!({ "kind": "adif", "park": "JP-0001" })),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, none) = h
            .send(
                "POST",
                &exports,
                Auth::Session(&cookie),
                Some(json!({ "kind": "pota" })),
            )
            .await;
        assert_eq!((status, &none["records"]), (StatusCode::OK, &json!(0)));
        let (status, _) = h
            .send(
                "POST",
                &exports,
                Auth::Session(&cookie),
                Some(json!({ "kind": "cabrillo" })),
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }
}
