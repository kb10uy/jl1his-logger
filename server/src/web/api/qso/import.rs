use std::{collections::BTreeMap, sync::Arc};

use axum::{body::Bytes, extract::State};
use serde::{Deserialize, Serialize};

use crate::{
    domain::{
        adif::{self, Encoding, LengthUnit, Lengths},
        confirmation::Source,
        qso::{Qso, Warning},
    },
    store::{
        export,
        import::{MergeReport, NewReport, import_new, merge},
        logbook::{instrument, station},
        today,
    },
    web::{
        AppState,
        api::logbook::member::Member,
        api::qso::export::SkippedBody,
        error::ApiError,
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
    instrument_id: Option<String>,
    #[serde(default)]
    format: FormatRequest,
    length: Option<LengthUnit>,
    encoding: Option<Encoding>,
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
pub struct ModeWarning {
    record: usize,
    #[serde(flatten)]
    warning: Warning,
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
        encoding: Encoding,
        lengths: Vec<LengthUnit>,
        read: usize,
        created: usize,
        existing: usize,
        duplicates: Vec<RecordIds>,
        outside_band: Vec<OutsideBand>,
        modes: Vec<ModeWarning>,
        invalid: Vec<Invalid>,
        rejects: Option<String>,
    },
    Merge {
        applied: bool,
        encoding: Encoding,
        lengths: Vec<LengthUnit>,
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
    (!records.is_empty()).then(|| export::document(records, unit))
}

struct Reading {
    encoding: Encoding,
    lengths: Vec<LengthUnit>,
    unit: LengthUnit,
}

impl ImportBody {
    fn new(report: NewReport, applied: bool, reading: Reading) -> ImportBody {
        ImportBody::New {
            applied,
            encoding: reading.encoding,
            lengths: reading.lengths,
            read: report.read,
            created: report.created,
            existing: report.existing,
            duplicates: record_ids(report.duplicates),
            outside_band: report
                .outside_band
                .into_iter()
                .map(|(record, freq, band)| OutsideBand { record, freq, band })
                .collect(),
            modes: report
                .modes
                .into_iter()
                .map(|(record, warning)| ModeWarning { record, warning })
                .collect(),
            invalid: invalid(report.invalid),
            rejects: rejects(&report.rejects, reading.unit),
        }
    }

    fn merge(report: MergeReport, applied: bool, reading: Reading) -> ImportBody {
        ImportBody::Merge {
            applied,
            encoding: reading.encoding,
            lengths: reading.lengths,
            read: report.read,
            not_confirmations: report.not_confirmations,
            merged: report.merged,
            already: report.already,
            unmatched: report.unmatched,
            ambiguous: record_ids(report.ambiguous),
            invalid: invalid(report.invalid),
            skipped: report.skipped.into_iter().map(SkippedBody::from).collect(),
            differences: report.differences,
            rejects: rejects(&report.rejects, reading.unit),
        }
    }
}

pub async fn import(
    State(state): State<Arc<AppState>>,
    member: Member,
    Query(query): Query<ImportQuery>,
    body: Bytes,
) -> Result<Json<ImportBody>, ApiError> {
    let access = member.access;
    let default = state.config.adif.length;
    let (text, encoding) =
        adif::decode(&body, query.encoding, state.config.adif.fallback_encoding).map_err(|e| match e {
            Encoding::Utf8 => ApiError::bad_request("the file is not UTF-8").with("code", "not_utf8".into()),
            e => ApiError::bad_request(format!("the file is not {}", e.name()))
                .with("code", "not_encoding".into())
                .with("encoding", e.name().into()),
        })?;
    let lengths = match query.length {
        Some(unit) => Lengths::Fixed(unit),
        None => Lengths::Detect { default },
    };
    let read = match query.format {
        FormatRequest::Adi => adif::read_adi(&text, encoding, lengths),
        FormatRequest::Adx => adif::read_adx(&text),
    }
    .map_err(|e| {
        ApiError::bad_request(format!("the file is not ADIF: {e}"))
            .with("code", "not_adif".into())
            .with("detail", e.to_string().into())
    })?;
    let records = read.records;
    let reading = Reading {
        encoding,
        lengths: read.lengths,
        unit: query.length.unwrap_or(default),
    };

    let station = match &query.station_id {
        Some(id) => {
            let mut conn = state.db.acquire().await?;
            Some(station::get(&mut conn, &access, id).await?)
        }
        None => None,
    };

    let source = match query.source {
        SourceRequest::New => {
            let station = station.ok_or_else(|| ApiError::bad_request("new QSOs need a station_id"))?;
            let instrument = match &query.instrument_id {
                Some(id) => {
                    let mut conn = state.db.acquire().await?;
                    Some(instrument::get(&mut conn, &access, id).await?)
                }
                None => None,
            };
            let report = import_new(&state.db, &access, &station, instrument.as_ref(), records, query.apply).await?;
            return Ok(Json(ImportBody::new(report, query.apply, reading)));
        }
        _ if query.instrument_id.is_some() => {
            return Err(ApiError::bad_request("an instrument_id is only for new QSOs"));
        }
        SourceRequest::Lotw => Source::Lotw,
        SourceRequest::Eqsl => Source::Eqsl,
        SourceRequest::Card => Source::Card,
    };
    let report = merge(
        &state.db,
        &access,
        source,
        station.as_ref(),
        records,
        query.apply,
        &today(),
    )
    .await?;
    Ok(Json(ImportBody::merge(report, query.apply, reading)))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::{Value, json};

    use crate::{
        domain::adif::Encoding,
        web::testing::{Auth, Harness},
    };

    const NEW: &str = "header\n<EOH>\n\
        <CALL:6>JA1AAA <QSO_DATE:8>20261022 <TIME_ON:4>0101 <BAND:3>40m <MODE:2>CW <NAME:2>太郎 <EOR>\n\
        <CALL:6>JA1BBB <QSO_DATE:8>20261022 <TIME_ON:4>0202 <BAND:3>40m <MODE:2>CW <EOR>\n\
        <CALL:6>JA1CCC <QSO_DATE:8>20261022 <TIME_ON:4>0303 <MODE:2>CW <EOR>\n";

    const LOTW: &str = "ARRL Logbook of the World Status Report\n<EOH>\n\
        <CALL:6>JA1AAA <QSO_DATE:8>20261022 <TIME_ON:6>010500 <BAND:3>40M <MODE:2>CW \
        <QSL_RCVD:1>Y <QSLRDATE:8>20261101 <EOR>\n\
        <CALL:6>JA1ZZZ <QSO_DATE:8>20261022 <TIME_ON:4>0101 <BAND:3>40m <MODE:2>CW <QSL_RCVD:1>Y <EOR>\n";

    fn imports(h: &Harness, query: &str) -> String {
        h.path(&format!("/imports?{query}"))
    }

    async fn count(h: &Harness, cookie: &str) -> usize {
        let (_, body) = h.send("GET", &h.path("/changes"), Auth::Session(cookie), None).await;
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
    async fn imports_new_qsos_with_an_instrument() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let rig = h.world.instrument("IC-705").await;
        let query = format!(
            "source=new&apply=true&station_id={}&instrument_id={}",
            h.station.id, rig.id
        );
        let (status, report) = h.send_text(&imports(&h, &query), Auth::Session(&cookie), NEW).await;
        assert_eq!((status, &report["created"]), (StatusCode::OK, &json!(2)), "{report}");
        let (_, changes) = h.send("GET", &h.path("/changes"), Auth::Session(&cookie), None).await;
        for qso in changes["qsos"].as_array().unwrap() {
            assert_eq!(
                (&qso["instrument_id"], &qso["fields"]["MY_RIG"]),
                (&json!(rig.id), &json!("IC-705"))
            );
        }

        let merge = format!("source=lotw&instrument_id={}", rig.id);
        let (status, _) = h.send_text(&imports(&h, &merge), Auth::Session(&cookie), LOTW).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let unknown = format!("source=new&station_id={}&instrument_id=nothing", h.station.id);
        let (status, _) = h.send_text(&imports(&h, &unknown), Auth::Session(&cookie), NEW).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn reports_modes_outside_the_adif_table() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let adi = "header\n<EOH>\n\
                   <CALL:6>JA1AAA<QSO_DATE:8>20261022<TIME_ON:4>0101<BAND:3>40m<MODE:3>FT9<EOR>\n\
                   <CALL:6>JA1BBB<QSO_DATE:8>20261022<TIME_ON:4>0202<BAND:3>40m<MODE:3>FT8<SUBMODE:3>FT4<EOR>\n\
                   <CALL:6>JA1CCC<QSO_DATE:8>20261022<TIME_ON:4>0303<BAND:3>40m<MODE:4>C4FM<EOR>\n";
        let query = format!("source=new&station_id={}", h.station.id);
        let (status, report) = h.send_text(&imports(&h, &query), Auth::Session(&cookie), adi).await;
        assert_eq!(status, StatusCode::OK, "{report}");
        assert_eq!(report["created"], 3);
        assert_eq!(
            report["modes"],
            json!([
                { "record": 1, "kind": "unknown_mode", "mode": "FT9" },
                { "record": 2, "kind": "submode_of_another_mode", "mode": "FT8", "submode": "FT4", "expected": "MFSK" },
            ])
        );
    }

    #[tokio::test]
    async fn reads_a_shift_jis_file_and_reports_how() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let bytes = Encoding::ShiftJis
            .encode("<CALL:6>JA1AAA <QSO_DATE:8>20261022 <TIME_ON:4>0101 <BAND:3>40m <MODE:2>CW <NAME:4>太郎 <EOR>\n");
        let station = format!("source=new&station_id={}", h.station.id);

        let (status, preview) = h
            .send_bytes(&imports(&h, &station), Auth::Session(&cookie), &bytes)
            .await;
        assert_eq!(status, StatusCode::OK, "{preview}");
        assert_eq!(
            (&preview["encoding"], &preview["lengths"], &preview["created"]),
            (&json!("shift_jis"), &json!(["bytes"]), &json!(1))
        );

        let (_, utf8) = h.send_text(&imports(&h, &station), Auth::Session(&cookie), NEW).await;
        assert_eq!(
            (&utf8["encoding"], &utf8["lengths"]),
            (&json!("utf-8"), &json!(["codepoints"]))
        );

        let utf8_only = format!("{station}&encoding=utf-8");
        let (status, refused) = h
            .send_bytes(&imports(&h, &utf8_only), Auth::Session(&cookie), &bytes)
            .await;
        assert_eq!(
            (status, &refused["code"]),
            (StatusCode::BAD_REQUEST, &json!("not_utf8"))
        );
        let ascii = format!("{station}&encoding=ascii");
        let (status, refused) = h.send_bytes(&imports(&h, &ascii), Auth::Session(&cookie), &bytes).await;
        assert_eq!(
            (status, &refused["code"], &refused["encoding"]),
            (StatusCode::BAD_REQUEST, &json!("not_encoding"), &json!("ASCII"))
        );
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
        let (_, page) = h
            .send("GET", &h.path("/qsos?call=JA1AAA"), Auth::Session(&cookie), None)
            .await;
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
