use std::{borrow::Cow, collections::BTreeSet};

use adif_manip::{
    LengthMode,
    document::AdifDocument,
    encoding_rs::{self, SHIFT_JIS, WINDOWS_1252},
    error::AdifError,
    write,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::qso::{Qso, QsoError};

pub const ADIF_VERSION: &str = include!(concat!(env!("OUT_DIR"), "/adif/version.rs"));
pub const PROGRAM_ID: &str = "jl1his-logger";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LengthUnit {
    #[default]
    Codepoints,
    Bytes,
    Utf16,
}

impl LengthUnit {
    const ALL: [LengthUnit; 3] = [LengthUnit::Codepoints, LengthUnit::Bytes, LengthUnit::Utf16];

    pub fn name(self) -> &'static str {
        match self {
            LengthUnit::Codepoints => "codepoints",
            LengthUnit::Bytes => "bytes",
            LengthUnit::Utf16 => "UTF-16 units",
        }
    }

    fn mode(self, encoding: Encoding) -> LengthMode {
        match (self, encoding.legacy()) {
            (LengthUnit::Codepoints, _) => LengthMode::Codepoints,
            (LengthUnit::Bytes, Some(legacy)) => LengthMode::EncodedBytes(legacy),
            (LengthUnit::Bytes, None) => LengthMode::Bytes,
            (LengthUnit::Utf16, _) => LengthMode::Utf16Units,
        }
    }

    fn of(mode: LengthMode) -> LengthUnit {
        match mode {
            LengthMode::Bytes | LengthMode::EncodedBytes(_) => LengthUnit::Bytes,
            LengthMode::Codepoints | LengthMode::Graphemes => LengthUnit::Codepoints,
            LengthMode::Utf16Units => LengthUnit::Utf16,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
pub enum Encoding {
    #[default]
    #[serde(rename = "utf-8")]
    Utf8,
    #[serde(rename = "shift_jis")]
    ShiftJis,
    #[serde(rename = "windows-1252")]
    Windows1252,
    #[serde(rename = "ascii")]
    Ascii,
}

impl Encoding {
    pub fn name(self) -> &'static str {
        match self {
            Encoding::Utf8 => "UTF-8",
            Encoding::ShiftJis => "Shift_JIS",
            Encoding::Windows1252 => "Windows-1252",
            Encoding::Ascii => "ASCII",
        }
    }

    fn legacy(self) -> Option<&'static encoding_rs::Encoding> {
        match self {
            Encoding::ShiftJis => Some(SHIFT_JIS),
            Encoding::Windows1252 => Some(WINDOWS_1252),
            Encoding::Utf8 | Encoding::Ascii => None,
        }
    }

    pub fn decode(self, bytes: &[u8]) -> Option<Cow<'_, str>> {
        match self.legacy() {
            Some(legacy) => legacy.decode_without_bom_handling_and_without_replacement(bytes),
            None if self == Encoding::Ascii && !bytes.is_ascii() => None,
            None => std::str::from_utf8(bytes).ok().map(Cow::Borrowed),
        }
    }

    pub fn encode(self, text: &str) -> Cow<'_, [u8]> {
        match self.legacy() {
            Some(legacy) => legacy.encode(text).0,
            None => Cow::Borrowed(text.as_bytes()),
        }
    }

    fn encodes(self, c: char) -> bool {
        match self.legacy() {
            _ if c.is_ascii() => true,
            Some(legacy) => !legacy.encode(c.encode_utf8(&mut [0; 4])).2,
            None => self == Encoding::Utf8,
        }
    }

    /// Returns the length unit of an export: lengths in a legacy encoding or ASCII are its bytes.
    pub fn export_unit(self, requested: Option<LengthUnit>, default: LengthUnit) -> Option<LengthUnit> {
        match (self, requested) {
            (Encoding::Utf8, requested) => Some(requested.unwrap_or(default)),
            (_, None | Some(LengthUnit::Bytes)) => Some(LengthUnit::Bytes),
            (_, Some(_)) => None,
        }
    }
}

/// Decodes a file in `encoding`, or without one as UTF-8 and failing that as `fallback`.
/// The error is the encoding the file is not valid in.
pub fn decode(
    bytes: &[u8],
    encoding: Option<Encoding>,
    fallback: Encoding,
) -> Result<(Cow<'_, str>, Encoding), Encoding> {
    let encoding = match encoding {
        Some(encoding) => encoding,
        None if std::str::from_utf8(bytes).is_ok() => Encoding::Utf8,
        None => fallback,
    };
    encoding.decode(bytes).map(|text| (text, encoding)).ok_or(encoding)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unencodable {
    #[default]
    Error,
    Replace,
    Entity,
    Omit,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("record {record}: {field} has {character:?}, which the encoding cannot represent")]
pub struct UnencodableError {
    pub record: usize,
    pub field: String,
    pub character: char,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lengths {
    Fixed(LengthUnit),
    Detect { default: LengthUnit },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Read {
    pub records: Vec<Result<Qso, QsoError>>,
    pub lengths: Vec<LengthUnit>,
}

pub fn read_adi(text: &str, encoding: Encoding, lengths: Lengths) -> Result<Read, AdifError> {
    let units = match lengths {
        Lengths::Fixed(unit) => vec![unit],
        Lengths::Detect { default } => {
            let first = if encoding.legacy().is_some() {
                LengthUnit::Bytes
            } else {
                default
            };
            std::iter::once(first)
                .chain(LengthUnit::ALL.into_iter().filter(|&u| u != first))
                .collect()
        }
    };
    let modes: Vec<_> = units.iter().map(|u| u.mode(encoding)).collect();
    let document = adif_manip::read_adi_detecting(text, &modes)?;
    let lengths: BTreeSet<_> = document
        .records()
        .iter()
        .flat_map(|record| record.fields().values().filter_map(|field| field.length_mode()))
        .map(LengthUnit::of)
        .collect();
    Ok(Read {
        records: records(&document),
        lengths: lengths.into_iter().collect(),
    })
}

pub fn read_adx(text: &str) -> Result<Read, AdifError> {
    Ok(Read {
        records: records(&adif_manip::read_adx(text)?),
        lengths: vec![],
    })
}

fn records(document: &AdifDocument) -> Vec<Result<Qso, QsoError>> {
    document
        .records()
        .iter()
        .map(|record| Qso::from_fields(record.fields().iter().map(|(name, field)| (name, field.value()))))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Output {
    pub unit: LengthUnit,
    pub encoding: Encoding,
    pub unencodable: Unencodable,
}

impl Output {
    pub fn utf8(unit: LengthUnit) -> Output {
        Output {
            unit,
            encoding: Encoding::Utf8,
            unencodable: Unencodable::Error,
        }
    }

    fn mode(self) -> LengthMode {
        self.unit.mode(self.encoding)
    }
}

pub fn header(program_version: &str, created: DateTime<Utc>, output: Output) -> String {
    let created = created.format("%Y%m%d %H%M%S").to_string();
    let fields = [
        ("ADIF_VER", ADIF_VERSION),
        ("CREATED_TIMESTAMP", &created),
        ("PROGRAMID", PROGRAM_ID),
        ("PROGRAMVERSION", program_version),
    ];
    write::header(&format!("Generated by {PROGRAM_ID}\n"), fields, output.mode()).expect("the header is valid ADI")
}

/// Writes the file as text; `output.encoding.encode` turns it into the file's bytes.
pub fn document(
    program_version: &str,
    created: DateTime<Utc>,
    records: &[Qso],
    output: Output,
) -> Result<String, UnencodableError> {
    let mut out = header(program_version, created, output);
    for (index, qso) in records.iter().enumerate() {
        out.push_str(&record(qso, output).map_err(|(field, character)| UnencodableError {
            record: index + 1,
            field,
            character,
        })?);
    }
    Ok(out)
}

fn record(qso: &Qso, output: Output) -> Result<String, (String, char)> {
    let mut fields = vec![];
    for (name, value) in qso.iter() {
        match fit(value, output).map_err(|c| (name.to_owned(), c))? {
            Some(value) => fields.push((name, value)),
            None => continue,
        }
    }
    Ok(write::record(fields, output.mode()).expect("QSO field names are valid ADIF field names"))
}

fn fit(value: &str, output: Output) -> Result<Option<Cow<'_, str>>, char> {
    let encoding = output.encoding;
    let Some(first) = value.chars().find(|&c| !encoding.encodes(c)) else {
        return Ok(Some(Cow::Borrowed(value)));
    };
    let substitute = |with: fn(char) -> String| {
        value
            .chars()
            .map(|c| if encoding.encodes(c) { c.to_string() } else { with(c) })
            .collect::<String>()
    };
    match output.unencodable {
        Unencodable::Error => Err(first),
        Unencodable::Omit => Ok(None),
        Unencodable::Replace => Ok(Some(Cow::Owned(substitute(|_| "?".to_owned())))),
        Unencodable::Entity => Ok(Some(Cow::Owned(substitute(|c| format!("&#{};", u32::from(c)))))),
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const EXPORT_CODEPOINTS: &str = include_str!("../../../fixtures/adif/export-codepoints.adi");
    const EXPORT_BYTES: &str = include_str!("../../../fixtures/adif/export-bytes.adi");
    const IMPORT: &str = include_str!("../../../fixtures/adif/import.adi");

    fn exported() -> Qso {
        Qso::from_fields([
            ("CALL", "JA1XYZ"),
            ("QSO_DATE", "20261022"),
            ("TIME_ON", "040506"),
            ("BAND", "40m"),
            ("MODE", "CW"),
            ("FREQ", "7.041"),
            ("RST_SENT", "599"),
            ("RST_RCVD", "599"),
            ("STATION_CALLSIGN", "JL1HIS"),
            ("OPERATOR", "JL1HIS"),
            ("NAME", "太郎"),
            ("QTH", "東京都港区"),
            ("CNTY", "100103"),
            ("QSL_SENT", "R"),
            ("MY_POTA_REF", "JP-0001,JP-0002"),
            ("MY_RIG", "IC-705"),
            ("APP_HISLOG_NOTE", "<EOR> in a note"),
        ])
        .unwrap()
    }

    fn created() -> DateTime<Utc> {
        "2026-10-22T04:05:06Z".parse().unwrap()
    }

    fn read_utf8(text: &str, unit: LengthUnit) -> Vec<Result<Qso, QsoError>> {
        read_adi(text, Encoding::Utf8, Lengths::Fixed(unit)).unwrap().records
    }

    fn output(encoding: Encoding, unencodable: Unencodable) -> Output {
        Output {
            unit: LengthUnit::Bytes,
            encoding,
            unencodable,
        }
    }

    #[rstest]
    #[case::codepoints(LengthUnit::Codepoints, EXPORT_CODEPOINTS)]
    #[case::bytes(LengthUnit::Bytes, EXPORT_BYTES)]
    fn writes_the_shared_fixture(#[case] unit: LengthUnit, #[case] expected: &str) {
        let written = document("0.1.0", created(), &[exported()], Output::utf8(unit)).unwrap();
        assert_eq!(written, expected);
    }

    #[rstest]
    #[case::codepoints(LengthUnit::Codepoints, EXPORT_CODEPOINTS)]
    #[case::bytes(LengthUnit::Bytes, EXPORT_BYTES)]
    fn reads_back_what_it_writes(#[case] unit: LengthUnit, #[case] text: &str) {
        assert_eq!(read_utf8(text, unit), [Ok(exported())]);
    }

    #[test]
    fn misreads_lengths_counted_in_the_other_unit() {
        let misread = read_utf8(EXPORT_BYTES, LengthUnit::Codepoints);
        assert!(!matches!(misread.as_slice(), [Ok(qso)] if *qso == exported()));
    }

    #[rstest]
    #[case::codepoints(EXPORT_CODEPOINTS, LengthUnit::Codepoints)]
    #[case::bytes(EXPORT_BYTES, LengthUnit::Bytes)]
    fn detects_the_unit_and_reports_it(#[case] text: &str, #[case] unit: LengthUnit) {
        let read = read_adi(
            text,
            Encoding::Utf8,
            Lengths::Detect {
                default: LengthUnit::Codepoints,
            },
        )
        .unwrap();
        assert_eq!(read.records, [Ok(exported())]);
        assert_eq!(read.lengths, [unit]);
    }

    #[rstest]
    #[case::hamlog_bytes("<NAME:4>太郎 <QTH:5>ｶﾅｶﾞﾜ <EOR>\n", LengthUnit::Bytes)]
    #[case::zlog_characters("<NAME:2>太郎 <QTH:5>ｶﾅｶﾞﾜ <EOR>\n", LengthUnit::Codepoints)]
    fn reads_shift_jis_lengths(#[case] text: &str, #[case] unit: LengthUnit) {
        let bytes = Encoding::ShiftJis.encode(text);
        let (decoded, encoding) = decode(&bytes, None, Encoding::ShiftJis).unwrap();
        assert_eq!(encoding, Encoding::ShiftJis);
        let read = read_adi(
            &decoded,
            encoding,
            Lengths::Detect {
                default: LengthUnit::Codepoints,
            },
        )
        .unwrap();
        let qso = read.records[0].as_ref().unwrap();
        assert_eq!((qso.get("NAME"), qso.get("QTH")), (Some("太郎"), Some("ｶﾅｶﾞﾜ")));
        assert_eq!(read.lengths, [unit]);
    }

    #[test]
    fn decodes_utf8_first_then_the_fallback() {
        assert_eq!(
            decode("太郎".as_bytes(), None, Encoding::ShiftJis),
            Ok((Cow::Borrowed("太郎"), Encoding::Utf8))
        );
        let shift_jis = Encoding::ShiftJis.encode("太郎");
        assert_eq!(decode(&shift_jis, None, Encoding::ShiftJis).unwrap().0, "太郎");
        assert_eq!(
            decode(&shift_jis, Some(Encoding::Utf8), Encoding::ShiftJis),
            Err(Encoding::Utf8)
        );
        assert_eq!(decode(&[0x82], None, Encoding::ShiftJis), Err(Encoding::ShiftJis));
        assert_eq!(
            decode("é".as_bytes(), Some(Encoding::Ascii), Encoding::ShiftJis),
            Err(Encoding::Ascii)
        );
    }

    #[test]
    fn exports_shift_jis_with_its_byte_lengths() {
        let text = document(
            "0.1.0",
            created(),
            &[exported()],
            output(Encoding::ShiftJis, Unencodable::Error),
        )
        .unwrap();
        assert!(text.contains("<NAME:4>太郎 <QTH:10>東京都港区 "));
        let bytes = Encoding::ShiftJis.encode(&text);
        let (decoded, _) = decode(&bytes, Some(Encoding::ShiftJis), Encoding::ShiftJis).unwrap();
        let read = read_adi(&decoded, Encoding::ShiftJis, Lengths::Fixed(LengthUnit::Bytes)).unwrap();
        assert_eq!(read.records, [Ok(exported())]);
    }

    #[rstest]
    #[case::replace(Unencodable::Replace, Some("<NAME:2>?? "))]
    #[case::entity(Unencodable::Entity, Some("<NAME:16>&#22826;&#37070; "))]
    #[case::omit(Unencodable::Omit, None)]
    fn handles_characters_the_encoding_cannot_represent(#[case] unencodable: Unencodable, #[case] name: Option<&str>) {
        let text = document(
            "0.1.0",
            created(),
            &[exported()],
            output(Encoding::Windows1252, unencodable),
        )
        .unwrap();
        match name {
            Some(name) => assert!(text.contains(name), "{text}"),
            None => assert!(!text.contains("<NAME:"), "{text}"),
        }
        assert!(text.contains("<CALL:6>JA1XYZ "));
    }

    #[test]
    fn refuses_characters_the_encoding_cannot_represent() {
        let records = [Qso::from_fields([("CALL", "JA1XYZ")]).unwrap(), exported()];
        assert_eq!(
            document(
                "0.1.0",
                created(),
                &records,
                output(Encoding::Ascii, Unencodable::Error)
            ),
            Err(UnencodableError {
                record: 2,
                field: "NAME".into(),
                character: '太',
            })
        );
    }

    #[rstest]
    #[case(Encoding::Utf8, None, Some(LengthUnit::Codepoints))]
    #[case(Encoding::Utf8, Some(LengthUnit::Utf16), Some(LengthUnit::Utf16))]
    #[case(Encoding::ShiftJis, None, Some(LengthUnit::Bytes))]
    #[case(Encoding::Ascii, Some(LengthUnit::Bytes), Some(LengthUnit::Bytes))]
    #[case(Encoding::Windows1252, Some(LengthUnit::Codepoints), None)]
    fn counts_legacy_exports_in_bytes(
        #[case] encoding: Encoding,
        #[case] requested: Option<LengthUnit>,
        #[case] unit: Option<LengthUnit>,
    ) {
        assert_eq!(encoding.export_unit(requested, LengthUnit::Codepoints), unit);
    }

    #[test]
    fn reads_an_import_with_lowercase_names_and_type_indicators() {
        let records: Vec<Qso> = read_utf8(IMPORT, LengthUnit::Codepoints)
            .into_iter()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(records.len(), 2);

        let first = &records[0];
        assert_eq!(first.get("CALL"), Some("JA1XYZ"));
        assert_eq!(first.get("BAND"), Some("40M"));
        assert_eq!(first.get("FREQ"), Some("7.041"));
        assert_eq!(first.get("NAME"), Some("太郎"));
        assert_eq!(first.get("QTH"), Some("東京都港区"));
        assert_eq!(first.get("MY_POTA_REF"), Some("JP-0001,JP-0002"));
        assert_eq!(first.fields.get("APP_WAVELOG_ID").map(String::as_str), Some("1284"));
        assert_eq!(first.check().unwrap().derived.freq_hz, Some(7_041_000));

        let second = &records[1];
        assert_eq!(second.get("FREQ"), None);
        assert_eq!(second.get("CNTY"), Some("0101"));
        let derived = second.check().unwrap().derived;
        assert_eq!((derived.call_base.as_str(), derived.freq_hz), ("JA1ZZZ", None));
    }

    #[test]
    fn reports_a_bad_record_without_dropping_the_others() {
        let text = "<CALL:6>JA1XYZ <EOR>\n<CALL:6>JA1XYZ <NAMÉ:4>Taro <EOR>\n<CALL:6>JA1ZZZ <EOR>\n";
        let records = read_utf8(text, LengthUnit::Codepoints);
        assert_eq!(records.len(), 3);
        assert!(records[0].is_ok() && records[2].is_ok());
        assert_eq!(records[1], Err(QsoError::FieldName("NAMÉ".into())));
    }

    #[test]
    fn reads_adx() {
        let text = "<?xml version=\"1.0\"?>\n<ADX><HEADER><ADIF_VER>3.1.5</ADIF_VER></HEADER><RECORDS>\
                    <RECORD><CALL>JA1XYZ</CALL><NAME>太郎</NAME><APP PROGRAMID=\"HISLOG\" FIELDNAME=\"ID\" TYPE=\"S\">7</APP></RECORD>\
                    </RECORDS></ADX>";
        let records = read_adx(text).unwrap().records;
        let qso = records[0].as_ref().unwrap();
        assert_eq!(qso.get("NAME"), Some("太郎"));
        assert_eq!(qso.get("APP_HISLOG_ID"), Some("7"));
    }
}
