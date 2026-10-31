mod columns;
mod id;

use std::{collections::BTreeMap, str::FromStr};

use chrono::NaiveDate;
use jl1his_query::millionths;
use serde::Serialize;

use crate::domain::{
    band::{Band, parse_frequency_hz},
    callsign, mode,
};

pub use columns::Columns;
pub use id::{InvalidQsoId, QsoId};

pub const MAX_FIELDS: usize = 1024;
pub const MAX_VALUE_BYTES: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QsoError {
    #[error("{0} is required")]
    Missing(&'static str),
    #[error("{name} {value:?} is not valid")]
    Invalid { name: String, value: String },
    #[error("{0:?} is not a valid ADIF field name")]
    FieldName(String),
    #[error("{0} is given more than once")]
    Repeated(String),
    #[error("{0} belongs in the QSO columns, not the extra fields")]
    ColumnField(String),
    #[error("{name} is {bytes} bytes long; a value has at most {MAX_VALUE_BYTES}")]
    TooLong { name: String, bytes: usize },
    #[error("the QSO has {0} extra fields; at most {MAX_FIELDS} are kept")]
    TooManyFields(usize),
}

impl QsoError {
    pub fn invalid(name: impl Into<String>, value: impl Into<String>) -> QsoError {
        QsoError::Invalid {
            name: name.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Warning {
    FrequencyOutsideBand {
        freq: String,
        band: String,
    },
    PossibleDuplicate {
        ids: Vec<String>,
    },
    UnknownMode {
        mode: String,
    },
    UnknownSubmode {
        submode: String,
    },
    SubmodeOfAnotherMode {
        mode: String,
        submode: String,
        expected: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Derived {
    pub call_base: String,
    pub qso_time: i64,
    pub freq_hz: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    pub derived: Derived,
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Qso {
    pub columns: Columns,
    pub fields: BTreeMap<String, String>,
}

impl Qso {
    pub fn from_fields<I, K, V>(fields: I) -> Result<Qso, QsoError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: Into<String>,
    {
        let mut qso = Qso::default();
        for (name, value) in fields {
            let name = field_name(name.as_ref())?;
            if qso.get(&name).is_some() {
                return Err(QsoError::Repeated(name));
            }
            qso.put(name, value.into())?;
        }
        Ok(qso)
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_uppercase();
        self.columns
            .get(&name)
            .or_else(|| self.fields.get(&name).map(String::as_str))
    }

    pub fn set(&mut self, name: &str, value: Option<String>) -> Result<(), QsoError> {
        self.put(field_name(name)?, value.unwrap_or_default())
    }

    pub fn fill(&mut self, name: &str, value: &str) -> Result<(), QsoError> {
        match self.get(name) {
            Some(_) => Ok(()),
            None => self.set(name, Some(value.to_owned())),
        }
    }

    pub fn bake_station(&mut self, callsign: &str, defaults: &BTreeMap<String, String>) -> Result<(), QsoError> {
        self.fill("STATION_CALLSIGN", callsign)?;
        self.bake(defaults)
    }

    pub fn bake(&mut self, defaults: &BTreeMap<String, String>) -> Result<(), QsoError> {
        for (name, value) in defaults {
            self.fill(name, value)?;
        }
        Ok(())
    }

    pub fn rebake(
        &mut self,
        current: &Qso,
        before: &BTreeMap<String, String>,
        after: &BTreeMap<String, String>,
    ) -> Result<(), QsoError> {
        let names: Vec<&String> = before
            .keys()
            .chain(after.keys().filter(|n| !before.contains_key(*n)))
            .collect();
        for name in names {
            let (old, new) = (before.get(name), after.get(name));
            if old == new || self.get(name) != current.get(name) {
                continue;
            }
            match new {
                Some(value) => self.set(name, Some(value.clone()))?,
                None if current.get(name) == old.map(String::as_str) => self.set(name, None)?,
                None => {}
            }
        }
        Ok(())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        let columns = Columns::NAMES
            .iter()
            .filter_map(|name| self.columns.get(name).map(|value| (*name, value)));
        columns.chain(self.fields.iter().map(|(name, value)| (name.as_str(), value.as_str())))
    }

    pub fn check(&self) -> Result<Checked, QsoError> {
        for (name, value) in &self.fields {
            let normalized = field_name(name)?;
            if Columns::NAMES.contains(&normalized.as_str()) {
                return Err(QsoError::ColumnField(normalized));
            }
            if *name != normalized {
                return Err(QsoError::FieldName(name.clone()));
            }
            field_value(name, value)?;
        }
        if self.fields.len() > MAX_FIELDS {
            return Err(QsoError::TooManyFields(self.fields.len()));
        }
        for name in Columns::NAMES {
            if let Some(value) = self.columns.get(name) {
                field_value(name, value)?;
            }
        }
        let required = |name: &'static str| self.get(name).ok_or(QsoError::Missing(name));
        let call = required("CALL")?;
        required("QSO_DATE")?;
        required("TIME_ON")?;
        let band_name = required("BAND")?;
        let mode_name = required("MODE")?;
        required("STATION_CALLSIGN")?;

        let call_base = callsign::base(call);
        if call_base.is_empty() {
            return Err(QsoError::invalid("CALL", call));
        }
        let qso_time = self.qso_time()?;
        let band = Band::named(band_name).ok_or_else(|| QsoError::invalid("BAND", band_name))?;

        let mut warnings: Vec<Warning> = mode::warning(mode_name, self.get("SUBMODE")).into_iter().collect();
        let freq_hz = match self.get("FREQ") {
            None => None,
            Some(freq) => {
                let hz = parse_frequency_hz(freq).ok_or_else(|| QsoError::invalid("FREQ", freq))?;
                if !band.contains(hz) {
                    warnings.push(Warning::FrequencyOutsideBand {
                        freq: freq.to_owned(),
                        band: band_name.to_owned(),
                    });
                }
                Some(i64::try_from(hz).map_err(|_| QsoError::invalid("FREQ", freq))?)
            }
        };

        Ok(Checked {
            derived: Derived {
                call_base,
                qso_time,
                freq_hz,
            },
            warnings,
        })
    }

    pub fn qso_time(&self) -> Result<i64, QsoError> {
        let date = self.get("QSO_DATE").ok_or(QsoError::Missing("QSO_DATE"))?;
        let time = self.get("TIME_ON").ok_or(QsoError::Missing("TIME_ON"))?;
        unix_time(date, time).ok_or_else(|| match parse_date(date) {
            None => QsoError::invalid("QSO_DATE", date),
            Some(_) => QsoError::invalid("TIME_ON", time),
        })
    }

    fn put(&mut self, name: String, value: String) -> Result<(), QsoError> {
        field_value(&name, &value)?;
        let value = Some(value).filter(|v| !v.is_empty());
        match (self.columns.slot(&name), value) {
            (Some(slot), value) => *slot = value,
            (None, Some(value)) => {
                if !self.fields.contains_key(&name) && self.fields.len() >= MAX_FIELDS {
                    return Err(QsoError::TooManyFields(self.fields.len() + 1));
                }
                self.fields.insert(name, value);
            }
            (None, None) => {
                self.fields.remove(&name);
            }
        }
        Ok(())
    }
}

pub fn field_name(name: &str) -> Result<String, QsoError> {
    let mut bytes = name.bytes();
    let valid = bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !name.eq_ignore_ascii_case("EOR")
        && !name.eq_ignore_ascii_case("EOH");
    match valid {
        true => Ok(name.to_ascii_uppercase()),
        false => Err(QsoError::FieldName(name.to_owned())),
    }
}

pub fn tx_power(value: &str) -> Result<(), QsoError> {
    let places = value.split_once('.').map_or(0, |(_, fraction)| fraction.len());
    match places <= 6 && millionths(value).is_some_and(|watts| watts > 0) {
        true => Ok(()),
        false => Err(QsoError::invalid("TX_PWR", value)),
    }
}

pub fn field_value(name: &str, value: &str) -> Result<(), QsoError> {
    match value.len() {
        bytes if bytes > MAX_VALUE_BYTES => Err(QsoError::TooLong {
            name: name.to_owned(),
            bytes,
        }),
        _ => Ok(()),
    }
}

fn digits<T: FromStr>(s: &str) -> Option<T> {
    s.bytes().all(|b| b.is_ascii_digit()).then(|| s.parse().ok())?
}

pub(crate) fn parse_date(date: &str) -> Option<NaiveDate> {
    if date.len() != 8 || !date.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    NaiveDate::from_ymd_opt(digits(&date[..4])?, digits(&date[4..6])?, digits(&date[6..])?)
}

fn unix_time(date: &str, time: &str) -> Option<i64> {
    let seconds = match time.len() {
        4 => "00",
        6 => time.get(4..)?,
        _ => return None,
    };
    let time = parse_date(date)?.and_hms_opt(digits(time.get(..2)?)?, digits(time.get(2..4)?)?, digits(seconds)?)?;
    Some(time.and_utc().timestamp())
}

#[cfg(test)]
pub(crate) mod tests {
    use rstest::rstest;

    use super::*;

    pub(crate) fn sample() -> Qso {
        Qso::from_fields([
            ("CALL", "JA1XYZ"),
            ("QSO_DATE", "20261022"),
            ("TIME_ON", "040506"),
            ("BAND", "40m"),
            ("MODE", "CW"),
            ("FREQ", "7.041"),
            ("STATION_CALLSIGN", "JL1HIS"),
        ])
        .unwrap()
    }

    pub(crate) fn with(fields: &[(&str, &str)]) -> Qso {
        let mut qso = sample();
        for (name, value) in fields {
            qso.set(name, Some((*value).to_owned())).unwrap();
        }
        qso
    }

    #[test]
    fn puts_fields_with_a_column_into_the_column_only() {
        let qso = Qso::from_fields([
            ("call", "JA1XYZ"),
            ("My_Pota_Ref", "JP-0001,JP-0002"),
            ("APP_HISLOG_ID", "42"),
            ("MY_RIG", "IC-705"),
        ])
        .unwrap();
        assert_eq!(qso.columns.call.as_deref(), Some("JA1XYZ"));
        assert_eq!(qso.columns.my_pota_ref.as_deref(), Some("JP-0001,JP-0002"));
        assert_eq!(
            qso.fields,
            BTreeMap::from([
                ("APP_HISLOG_ID".to_owned(), "42".to_owned()),
                ("MY_RIG".to_owned(), "IC-705".to_owned()),
            ])
        );
        assert_eq!(qso.get("my_rig"), Some("IC-705"));
        assert_eq!(qso.get("Call"), Some("JA1XYZ"));
    }

    #[test]
    fn keeps_values_as_they_arrived() {
        let qso = Qso::from_fields([("CALL", " ja1xyz/p "), ("NAME", "太郎"), ("CNTY", "100101")]).unwrap();
        assert_eq!(qso.get("CALL"), Some(" ja1xyz/p "));
        assert_eq!(qso.get("NAME"), Some("太郎"));
        assert_eq!(qso.get("CNTY"), Some("100101"));
    }

    #[test]
    fn treats_empty_values_as_absent() {
        let mut qso = Qso::from_fields([("NAME", ""), ("MY_RIG", "")]).unwrap();
        assert_eq!(qso, Qso::default());
        qso.set("NAME", Some("Taro".into())).unwrap();
        qso.set("MY_RIG", Some("IC-705".into())).unwrap();
        qso.set("NAME", Some(String::new())).unwrap();
        qso.set("my_rig", None).unwrap();
        assert_eq!(qso, Qso::default());
    }

    #[rstest]
    #[case("")]
    #[case(" CALL")]
    #[case("CALL ")]
    #[case("CA:LL")]
    #[case("CA<LL")]
    #[case("CA,LL")]
    #[case("CA{LL}")]
    #[case("MY RIG")]
    #[case("A-B")]
    #[case("A.B")]
    #[case("_X")]
    #[case("1A")]
    #[case("NAMÉ")]
    #[case("eor")]
    #[case("EOH")]
    fn refuses_invalid_field_names(#[case] name: &str) {
        assert_eq!(
            Qso::from_fields([(name, "x")]),
            Err(QsoError::FieldName(name.to_owned()))
        );
    }

    #[rstest]
    #[case("app_hislog_note")]
    #[case("A1")]
    #[case("X_")]
    fn accepts_adif_field_names(#[case] name: &str) {
        assert_eq!(field_name(name).unwrap(), name.to_ascii_uppercase());
    }

    #[test]
    fn bounds_value_length_and_extra_field_count() {
        let long = "x".repeat(MAX_VALUE_BYTES + 1);
        let too_long = QsoError::TooLong {
            name: "NAME".into(),
            bytes: MAX_VALUE_BYTES + 1,
        };
        assert_eq!(Qso::from_fields([("NAME", long.as_str())]), Err(too_long.clone()));
        let mut qso = sample();
        assert_eq!(qso.set("NAME", Some(long.clone())), Err(too_long));
        qso.set("COMMENT", Some("x".repeat(MAX_VALUE_BYTES))).unwrap();
        qso.fields.insert("APP_X_NOTE".into(), long);
        assert!(matches!(qso.check(), Err(QsoError::TooLong { .. })));

        let mut qso = sample();
        for i in 0..MAX_FIELDS {
            qso.set(&format!("APP_X_F{i}"), Some("v".into())).unwrap();
        }
        qso.set("APP_X_F0", Some("again".into())).unwrap();
        assert!(qso.check().is_ok());
        assert_eq!(
            qso.set("APP_X_MORE", Some("v".into())),
            Err(QsoError::TooManyFields(MAX_FIELDS + 1))
        );
        qso.fields.insert("APP_X_MORE".into(), "v".into());
        assert_eq!(qso.check(), Err(QsoError::TooManyFields(MAX_FIELDS + 1)));
    }

    #[test]
    fn refuses_a_field_given_twice() {
        assert_eq!(
            Qso::from_fields([("call", "A1A"), ("CALL", "B1B")]),
            Err(QsoError::Repeated("CALL".into()))
        );
        assert_eq!(
            Qso::from_fields([("app_x", "1"), ("APP_X", "2")]),
            Err(QsoError::Repeated("APP_X".into()))
        );
    }

    #[rstest]
    #[case("CALL")]
    #[case("call")]
    #[case("QSL_SENT")]
    fn refuses_core_fields_in_the_extra_map(#[case] name: &str) {
        let mut qso = sample();
        qso.fields.insert(name.into(), "x".into());
        assert_eq!(qso.check(), Err(QsoError::ColumnField(name.to_ascii_uppercase())));
    }

    #[rstest]
    #[case("APP:NOTE")]
    #[case("app_note")]
    #[case("EOH")]
    fn refuses_invalid_extra_field_keys(#[case] name: &str) {
        let mut qso = sample();
        qso.fields.insert(name.into(), "x".into());
        assert_eq!(qso.check(), Err(QsoError::FieldName(name.into())));
    }

    #[test]
    fn iterates_columns_in_order_then_other_fields_by_name() {
        let qso = Qso::from_fields([
            ("MY_RIG", "IC-705"),
            ("MY_POTA_REF", "JP-0001"),
            ("APP_X", "1"),
            ("CALL", "JA1XYZ"),
            ("BAND", "40m"),
        ])
        .unwrap();
        let names: Vec<_> = qso.iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["CALL", "BAND", "MY_POTA_REF", "APP_X", "MY_RIG"]);
    }

    #[test]
    fn derives_the_search_columns() {
        let checked = sample().check().unwrap();
        assert_eq!(
            checked.derived,
            Derived {
                call_base: "JA1XYZ".into(),
                qso_time: 1_792_641_906,
                freq_hz: Some(7_041_000),
            }
        );
        assert!(checked.warnings.is_empty());
    }

    #[rstest]
    #[case("0405", 1_792_641_900)]
    #[case("040506", 1_792_641_906)]
    #[case("235959", 1_792_713_599)]
    fn reads_four_and_six_digit_times(#[case] time_on: &str, #[case] expected: i64) {
        let mut qso = sample();
        qso.set("TIME_ON", Some(time_on.into())).unwrap();
        assert_eq!(qso.check().unwrap().derived.qso_time, expected);
    }

    #[test]
    fn saves_a_qso_without_a_frequency() {
        let mut qso = sample();
        qso.set("FREQ", None).unwrap();
        let checked = qso.check().unwrap();
        assert_eq!(checked.derived.freq_hz, None);
        assert_eq!(qso.get("FREQ"), None);
    }

    #[rstest]
    #[case("CALL")]
    #[case("QSO_DATE")]
    #[case("TIME_ON")]
    #[case("BAND")]
    #[case("MODE")]
    #[case("STATION_CALLSIGN")]
    fn requires_the_minimum_fields(#[case] name: &'static str) {
        let mut qso = sample();
        qso.set(name, None).unwrap();
        assert_eq!(qso.check(), Err(QsoError::Missing(name)));
    }

    #[rstest]
    #[case("CALL", "/")]
    #[case("QSO_DATE", "2026-10-22")]
    #[case("QSO_DATE", "20261301")]
    #[case("QSO_DATE", "20260230")]
    #[case("QSO_DATE", "2026102")]
    #[case("QSO_DATE", "2026€1")]
    #[case("QSO_DATE", "202€01")]
    #[case("TIME_ON", "2400")]
    #[case("TIME_ON", "0460")]
    #[case("TIME_ON", "04:05")]
    #[case("TIME_ON", "04056")]
    #[case("TIME_ON", "+40506")]
    #[case("BAND", "41m")]
    #[case("FREQ", "7.041.5")]
    fn refuses_values_the_derived_columns_cannot_use(#[case] name: &str, #[case] value: &str) {
        assert_eq!(with(&[(name, value)]).check(), Err(QsoError::invalid(name, value)));
    }

    #[test]
    fn accepts_a_band_in_any_case_and_keeps_it() {
        let mut qso = sample();
        qso.set("BAND", Some("40M".into())).unwrap();
        assert!(qso.check().is_ok());
        assert_eq!(qso.get("BAND"), Some("40M"));
    }

    #[test]
    fn warns_about_a_frequency_outside_its_band() {
        let mut qso = sample();
        qso.set("FREQ", Some("14.041".into())).unwrap();
        let checked = qso.check().unwrap();
        assert_eq!(checked.derived.freq_hz, Some(14_041_000));
        assert_eq!(
            checked.warnings,
            [Warning::FrequencyOutsideBand {
                freq: "14.041".into(),
                band: "40m".into()
            }]
        );
    }

    #[test]
    fn bakes_station_values_where_the_qso_has_none() {
        let mut qso = sample();
        qso.set("STATION_CALLSIGN", None).unwrap();
        qso.set("MY_CITY", Some("Shiba".into())).unwrap();
        let defaults = BTreeMap::from([
            ("MY_CITY".to_owned(), "Minato".to_owned()),
            ("MY_CNTY".to_owned(), "100103".to_owned()),
            ("MY_RIG".to_owned(), "IC-705".to_owned()),
        ]);
        qso.bake_station("JL1HIS/1", &defaults).unwrap();
        assert_eq!(qso.get("STATION_CALLSIGN"), Some("JL1HIS/1"));
        assert_eq!(qso.get("MY_CITY"), Some("Shiba"));
        assert_eq!(qso.get("MY_CNTY"), Some("100103"));
        assert_eq!(qso.get("MY_RIG"), Some("IC-705"));

        let mut given = sample();
        given.bake_station("JL1HIS/1", &BTreeMap::new()).unwrap();
        assert_eq!(given.get("STATION_CALLSIGN"), Some("JL1HIS"));
    }

    #[test]
    fn rebakes_values_that_change_and_keeps_those_edited_in_the_request() {
        let mut current = sample();
        for (name, value) in [
            ("MY_CITY", "Minato"),
            ("MY_CNTY", "100103"),
            ("MY_SIG", "POTA"),
            ("MY_GRIDSQUARE", "PM95"),
            ("TX_PWR", "5"),
        ] {
            current.set(name, Some(value.into())).unwrap();
        }
        let map = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
            pairs.iter().map(|(n, v)| ((*n).to_owned(), (*v).to_owned())).collect()
        };
        let before = map(&[
            ("STATION_CALLSIGN", "JL1HIS"),
            ("MY_CITY", "Minato"),
            ("MY_CNTY", "100101"),
            ("MY_SIG", "POTA"),
            ("TX_PWR", "5"),
        ]);
        let after = map(&[("STATION_CALLSIGN", "JL1HIS/1"), ("MY_CITY", "Shiba"), ("TX_PWR", "10")]);
        let mut qso = current.clone();
        qso.set("MY_CITY", Some("Edited".into())).unwrap();
        qso.rebake(&current, &before, &after).unwrap();
        let values: Vec<_> = [
            "STATION_CALLSIGN",
            "MY_CITY",
            "MY_CNTY",
            "MY_SIG",
            "MY_GRIDSQUARE",
            "TX_PWR",
        ]
        .iter()
        .map(|name| qso.get(name))
        .collect();
        assert_eq!(
            values,
            [
                Some("JL1HIS/1"),
                Some("Edited"),
                Some("100103"),
                None,
                Some("PM95"),
                Some("10")
            ]
        );
    }

    #[rstest]
    #[case("100", true)]
    #[case("0.5", true)]
    #[case(".25", true)]
    #[case("0.000001", true)]
    #[case("0.0000001", false)]
    #[case("0", false)]
    #[case("0.000", false)]
    #[case("QRP", false)]
    #[case("-5", false)]
    #[case("1.2.3", false)]
    #[case("", false)]
    fn reads_tx_power_as_watts_above_zero(#[case] value: &str, #[case] valid: bool) {
        assert_eq!(tx_power(value).is_ok(), valid, "{value}");
    }

    #[test]
    fn column_names_match_the_struct() {
        let mut qso = Qso::default();
        for name in Columns::NAMES {
            qso.set(name, Some((*name).to_owned())).unwrap();
            assert_eq!(qso.columns.get(name), Some(*name));
        }
        assert!(qso.fields.is_empty());
    }
}
