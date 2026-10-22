use std::{collections::BTreeMap, sync::Arc};

use axum::{
    body::Bytes,
    extract::{Path, State},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    domain::{
        adif::{self, LengthUnit},
        confirmation::Source,
        qso::Qso,
    },
    store::{
        StoreError, access,
        import::{MergeReport, NewReport, import_new, merge},
        station,
    },
    web::{
        AppState,
        auth::Caller,
        error::ApiError,
        export::{SkippedBody, adif_document},
        extract::{Json, Query},
    },
};

pub const MAX_FILE: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceRequest {
    New,
    Lotw,
    Eqsl,
    Card,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormatRequest {
    #[default]
    Adi,
    Adx,
}

#[derive(Debug, Deserialize)]
pub struct ImportQuery {
    source: SourceRequest,
    station_id: Option<String>,
    #[serde(default)]
    format: FormatRequest,
    length: Option<LengthUnit>,
    #[serde(default)]
    apply: bool,
}

#[derive(Debug, Serialize)]
pub struct RecordIds {
    record: usize,
    ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct OutsideBand {
    record: usize,
    freq: String,
    band: String,
}

#[derive(Debug, Serialize)]
pub struct Invalid {
    record: usize,
    error: String,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ImportBody {
    New {
        applied: bool,
        read: usize,
        created: usize,
        existing: usize,
        duplicates: Vec<RecordIds>,
        outside_band: Vec<OutsideBand>,
        invalid: Vec<Invalid>,
        rejects: Option<String>,
    },
    Merge {
        applied: bool,
        read: usize,
        not_confirmations: usize,
        merged: usize,
        already: usize,
        unmatched: Vec<usize>,
        ambiguous: Vec<RecordIds>,
        invalid: Vec<Invalid>,
        skipped: Vec<SkippedBody>,
        differences: BTreeMap<&'static str, usize>,
        rejects: Option<String>,
    },
}

fn invalid(records: Vec<(usize, String)>) -> Vec<Invalid> {
    records
        .into_iter()
        .map(|(record, error)| Invalid { record, error })
        .collect()
}

fn record_ids(records: Vec<(usize, Vec<String>)>) -> Vec<RecordIds> {
    records
        .into_iter()
        .map(|(record, ids)| RecordIds { record, ids })
        .collect()
}

fn rejects(records: &[Qso], unit: LengthUnit) -> Option<String> {
    (!records.is_empty()).then(|| adif_document(records, unit))
}

impl ImportBody {
    fn new(report: NewReport, applied: bool, unit: LengthUnit) -> ImportBody {
        ImportBody::New {
            applied,
            read: report.read,
            created: report.created,
            existing: report.existing,
            duplicates: record_ids(report.duplicates),
            outside_band: report
                .outside_band
                .into_iter()
                .map(|(record, freq, band)| OutsideBand { record, freq, band })
                .collect(),
            invalid: invalid(report.invalid),
            rejects: rejects(&report.rejects, unit),
        }
    }

    fn merge(report: MergeReport, applied: bool, unit: LengthUnit) -> ImportBody {
        ImportBody::Merge {
            applied,
            read: report.read,
            not_confirmations: report.not_confirmations,
            merged: report.merged,
            already: report.already,
            unmatched: report.unmatched,
            ambiguous: record_ids(report.ambiguous),
            invalid: invalid(report.invalid),
            skipped: report
                .skipped
                .into_iter()
                .map(|(id, reason)| SkippedBody::new(id, reason))
                .collect(),
            differences: report.differences,
            rejects: rejects(&report.rejects, unit),
        }
    }
}

pub async fn import(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path(logbook_id): Path<String>,
    Query(query): Query<ImportQuery>,
    body: Bytes,
) -> Result<Json<ImportBody>, ApiError> {
    let unit = query.length.unwrap_or(state.config.adif.length);
    let text = std::str::from_utf8(&body).map_err(|_| StoreError::Invalid("the file is not UTF-8".into()))?;
    let records = match query.format {
        FormatRequest::Adi => adif::read_adi(text, unit),
        FormatRequest::Adx => adif::read_adx(text),
    }
    .map_err(|e| StoreError::Invalid(format!("the file is not ADIF: {e}")))?;

    let mut conn = state.db.acquire().await?;
    let access = access::logbook(&mut conn, &caller.principal, &logbook_id).await?;
    let station = match &query.station_id {
        Some(id) => Some(station::get(&mut conn, &access, id).await?),
        None => None,
    };
    drop(conn);

    let source = match query.source {
        SourceRequest::New => {
            let station = station.ok_or_else(|| StoreError::Invalid("new QSOs need a station_id".into()))?;
            let report = import_new(&state.db, &access, &station, records, query.apply).await?;
            return Ok(Json(ImportBody::new(report, query.apply, unit)));
        }
        SourceRequest::Lotw => Source::Lotw,
        SourceRequest::Eqsl => Source::Eqsl,
        SourceRequest::Card => Source::Card,
    };
    let today = Utc::now().format("%Y%m%d").to_string();
    let report = merge(
        &state.db,
        &access,
        source,
        station.as_ref(),
        records,
        query.apply,
        &today,
    )
    .await?;
    Ok(Json(ImportBody::merge(report, query.apply, unit)))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::{Value, json};

    use crate::web::testing::{Auth, Harness};

    const NEW: &str = "header\n<EOH>\n\
        <CALL:6>JA1AAA <QSO_DATE:8>20261022 <TIME_ON:4>0101 <BAND:3>40m <MODE:2>CW <NAME:2>太郎 <EOR>\n\
        <CALL:6>JA1BBB <QSO_DATE:8>20261022 <TIME_ON:4>0202 <BAND:3>40m <MODE:2>CW <EOR>\n\
        <CALL:6>JA1CCC <QSO_DATE:8>20261022 <TIME_ON:4>0303 <MODE:2>CW <EOR>\n";

    const LOTW: &str = "ARRL Logbook of the World Status Report\n<EOH>\n\
        <CALL:6>JA1AAA <QSO_DATE:8>20261022 <TIME_ON:6>010500 <BAND:3>40M <MODE:2>CW \
        <QSL_RCVD:1>Y <QSLRDATE:8>20261101 <EOR>\n\
        <CALL:6>JA1ZZZ <QSO_DATE:8>20261022 <TIME_ON:4>0101 <BAND:3>40m <MODE:2>CW <QSL_RCVD:1>Y <EOR>\n";

    fn imports(h: &Harness, query: &str) -> String {
        format!("/api/logbooks/{}/imports?{query}", h.world.logbook.id)
    }

    async fn count(h: &Harness, cookie: &str) -> usize {
        let changes = format!("/api/logbooks/{}/changes", h.world.logbook.id);
        let (_, body) = h.send("GET", &changes, Auth::Session(cookie), None).await;
        body["qsos"].as_array().unwrap().len()
    }

    #[tokio::test]
    async fn previews_then_imports_new_qsos_once() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let station = format!("source=new&station_id={}", h.station.id);

        let (status, preview) = h.send_text(&imports(&h, &station), Auth::Session(&cookie), NEW).await;
        assert_eq!(status, StatusCode::OK, "{preview}");
        assert_eq!(
            (
                &preview["applied"],
                &preview["read"],
                &preview["created"],
                &preview["existing"]
            ),
            (&json!(false), &json!(3), &json!(2), &json!(0))
        );
        assert_eq!(preview["invalid"][0]["record"], 3);
        assert!(preview["invalid"][0]["error"].as_str().unwrap().contains("BAND"));
        let rejects = preview["rejects"].as_str().unwrap();
        assert!(rejects.contains("<CALL:6>JA1CCC"), "{rejects}");
        assert!(!rejects.contains("JA1AAA"), "{rejects}");
        assert_eq!(count(&h, &cookie).await, 0);

        let apply = format!("{station}&apply=true");
        let (status, applied) = h.send_text(&imports(&h, &apply), Auth::Session(&cookie), NEW).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!((&applied["applied"], &applied["created"]), (&json!(true), &json!(2)));
        assert_eq!(count(&h, &cookie).await, 2);

        let (_, again) = h.send_text(&imports(&h, &apply), Auth::Session(&cookie), NEW).await;
        assert_eq!((&again["created"], &again["existing"]), (&json!(0), &json!(2)));

        let (status, _) = h
            .send_text(&imports(&h, "source=new"), Auth::Session(&cookie), NEW)
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = h
            .send_text(
                &imports(&h, "source=new&station_id=nothing"),
                Auth::Session(&cookie),
                NEW,
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, body) = h
            .send_text(&imports(&h, "source=paper"), Auth::Session(&cookie), NEW)
            .await;
        assert_eq!((status, &body["status"]), (StatusCode::BAD_REQUEST, &json!("error")));
    }

    #[tokio::test]
    async fn reads_adx() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let adx = "<?xml version=\"1.0\"?>\n<ADX><HEADER></HEADER><RECORDS><RECORD>\
                   <CALL>JA1XYZ</CALL><QSO_DATE>20261022</QSO_DATE><TIME_ON>0101</TIME_ON>\
                   <BAND>40m</BAND><MODE>CW</MODE></RECORD></RECORDS></ADX>";
        let query = format!("source=new&format=adx&apply=true&station_id={}", h.station.id);
        let (status, report) = h.send_text(&imports(&h, &query), Auth::Session(&cookie), adx).await;
        assert_eq!(status, StatusCode::OK, "{report}");
        assert_eq!(report["created"], 1);
        assert_eq!(report["rejects"], Value::Null);
    }

    #[tokio::test]
    async fn merges_confirmations_into_matching_qsos() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let station = format!("source=new&apply=true&station_id={}", h.station.id);
        h.send_text(&imports(&h, &station), Auth::Session(&cookie), NEW).await;

        let (status, preview) = h
            .send_text(&imports(&h, "source=lotw"), Auth::Session(&cookie), LOTW)
            .await;
        assert_eq!(status, StatusCode::OK, "{preview}");
        assert_eq!(
            (
                &preview["applied"],
                &preview["read"],
                &preview["merged"],
                &preview["unmatched"]
            ),
            (&json!(false), &json!(2), &json!(1), &json!([2]))
        );
        assert!(preview["rejects"].as_str().unwrap().contains("JA1ZZZ"));

        let (_, applied) = h
            .send_text(&imports(&h, "source=lotw&apply=true"), Auth::Session(&cookie), LOTW)
            .await;
        assert_eq!((&applied["merged"], &applied["skipped"]), (&json!(1), &json!([])));
        let search = format!("/api/logbooks/{}/qsos?call=JA1AAA", h.world.logbook.id);
        let (_, page) = h.send("GET", &search, Auth::Session(&cookie), None).await;
        let fields = &page["qsos"][0]["fields"];
        assert_eq!(
            (&fields["LOTW_QSL_RCVD"], &fields["LOTW_QSLRDATE"]),
            (&json!("Y"), &json!("20261101"))
        );

        let (_, rerun) = h
            .send_text(&imports(&h, "source=lotw&apply=true"), Auth::Session(&cookie), LOTW)
            .await;
        assert_eq!((&rerun["merged"], &rerun["already"]), (&json!(0), &json!(1)));
    }
}
