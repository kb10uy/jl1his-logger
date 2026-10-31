use std::{collections::BTreeMap, sync::Arc};

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use base64::{Engine, prelude::BASE64_STANDARD};
use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    domain::{
        adif::{Encoding, LengthUnit, Output, Unencodable},
        qso::QsoId,
    },
    store::{
        WriteTx,
        bulk::{self, Change, Outcome, SkipReason, Skipped},
        export::{self, ExportBatch, ExportKind},
        qso::{Filter, Target},
    },
    web::{
        AppState,
        api::logbook::member::{Item, Member},
        api::qso::QsoBody,
        error::ApiError,
        extract::Json,
        query::text,
    },
};

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
    kind: ExportKind,
    created_by: String,
    created_at: String,
    applied_at: Option<String>,
    qso_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    qsos: Option<Vec<TargetBody>>,
}

impl From<ExportBatch> for BatchBody {
    fn from(batch: ExportBatch) -> BatchBody {
        BatchBody {
            id: batch.id,
            kind: batch.kind,
            created_by: batch.created_by,
            created_at: batch.created_at,
            applied_at: batch.applied_at,
            qso_count: batch.qso_count,
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

pub async fn list(State(state): State<Arc<AppState>>, member: Member) -> Result<Json<Vec<BatchBody>>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let batches = export::list(&mut conn, &member.access).await?;
    Ok(Json(batches.into_iter().map(BatchBody::from).collect()))
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
    let filter = Filter {
        query: text(req.q.as_deref())?,
        ..Filter::default()
    };
    let unit = req
        .encoding
        .export_unit(req.length, state.config.adif.length)
        .ok_or_else(|| ApiError::bad_request(format!("lengths in {} are counted in bytes", req.encoding.name())))?;
    let output = Output {
        unit,
        encoding: req.encoding,
        unencodable: req.unencodable,
    };
    let mut conn = state.db.acquire().await?;
    let selection = export::select(&mut conn, &member.access, req.kind, &filter, req.park.as_deref()).await?;
    drop(conn);
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
    let adif = export::encoded(&selection.records, output).map_err(|e| {
        ApiError::bad_request(e.to_string())
            .with("code", "unencodable".into())
            .with("record", e.record.into())
            .with("field", e.field.clone().into())
            .with("character", e.character.to_string().into())
    })?;
    let file = matches!(req.encoding, Encoding::ShiftJis | Encoding::Windows1252)
        .then(|| BASE64_STANDARD.encode(req.encoding.encode(&adif)));
    let mut tx = WriteTx::begin(&state.db).await?;
    let batch = export::create(&mut tx, &member.access, req.kind, &selection.targets).await?;
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
