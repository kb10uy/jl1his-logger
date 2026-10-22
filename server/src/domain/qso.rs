mod columns;
mod id;

use std::{collections::BTreeMap, str::FromStr};

use chrono::NaiveDate;

use crate::domain::{
    band::{Band, parse_frequency_hz},
    callsign,
};

pub use columns::Columns;
pub use id::{InvalidQsoId, QsoId};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QsoError {
    #[error("{0} is required")]
    Missing(&'static str),
    #[error("{name} {value:?} is not valid")]
    Invalid { name: &'static str, value: String },
    #[error("{0:?} is not a valid ADIF field name")]
    FieldName(String),
    #[error("{0} is given more than once")]
    Repeated(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    FrequencyOutsideBand { freq: String, band: String },
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
            qso.put(name, value.into());
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
        let name = field_name(name)?;
        match value {
            Some(value) => self.put(name, value),
            None => match self.columns.slot(&name) {
                Some(slot) => *slot = None,
                None => {
                    self.fields.remove(&name);
                }
            },
        }
        Ok(())
    }

    pub fn fill(&mut self, name: &str, value: &str) -> Result<(), QsoError> {
        match self.get(name) {
            Some(_) => Ok(()),
            None => self.set(name, Some(value.to_owned())),
        }
    }

    pub fn bake_station(&mut self, callsign: &str, defaults: &BTreeMap<String, String>) -> Result<(), QsoError> {
        self.fill("STATION_CALLSIGN", callsign)?;
        for (name, value) in defaults {
            self.fill(name, value)?;
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
        let required = |name: &'static str| self.get(name).ok_or(QsoError::Missing(name));
        let call = required("CALL")?;
        let qso_date = required("QSO_DATE")?;
        let time_on = required("TIME_ON")?;
        let band_name = required("BAND")?;
        required("MODE")?;
        required("STATION_CALLSIGN")?;

        let invalid = |name: &'static str, value: &str| QsoError::Invalid {
            name,
            value: value.to_owned(),
        };
        let call_base = callsign::base(call);
        if call_base.is_empty() {
            return Err(invalid("CALL", call));
        }
        let qso_time = qso_time(qso_date, time_on).ok_or_else(|| match parse_date(qso_date) {
            None => invalid("QSO_DATE", qso_date),
            Some(_) => invalid("TIME_ON", time_on),
        })?;
        let band = Band::named(band_name).ok_or_else(|| invalid("BAND", band_name))?;

        let mut warnings = vec![];
        let freq_hz = match self.get("FREQ") {
            None => None,
            Some(freq) => {
                let hz = parse_frequency_hz(freq).ok_or_else(|| invalid("FREQ", freq))?;
                if !band.contains(hz) {
                    warnings.push(Warning::FrequencyOutsideBand {
                        freq: freq.to_owned(),
                        band: band_name.to_owned(),
                    });
                }
                Some(i64::try_from(hz).map_err(|_| invalid("FREQ", freq))?)
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

    pub fn qsl_required(&self) -> bool {
        self.get("QSL_SENT")
            .is_some_and(|v| v.eq_ignore_ascii_case("R") || v.eq_ignore_ascii_case("Q"))
    }

    pub fn qsl_received(&self) -> bool {
        self.get("QSL_RCVD")
            .is_some_and(|v| v.eq_ignore_ascii_case("Y") || v.eq_ignore_ascii_case("V"))
    }

    fn put(&mut self, name: String, value: String) {
        let value = Some(value).filter(|v| !v.is_empty());
        match (self.columns.slot(&name), value) {
            (Some(slot), value) => *slot = value,
            (None, Some(value)) => {
                self.fields.insert(name, value);
            }
            (None, None) => {
                self.fields.remove(&name);
            }
        }
    }
}

pub fn field_name(name: &str) -> Result<String, QsoError> {
    let valid = !name.is_empty()
        && !name.starts_with(' ')
        && !name.ends_with(' ')
        && name
            .bytes()
            .all(|b| (b.is_ascii_graphic() || b == b' ') && !b",:<>{}".contains(&b))
        && !name.eq_ignore_ascii_case("EOR")
        && !name.eq_ignore_ascii_case("EOH");
    match valid {
        true => Ok(name.to_ascii_uppercase()),
        false => Err(QsoError::FieldName(name.to_owned())),
    }
}

fn digits<T: FromStr>(s: &str) -> Option<T> {
    s.bytes().all(|b| b.is_ascii_digit()).then(|| s.parse().ok())?
}

fn parse_date(date: &str) -> Option<NaiveDate> {
    if date.len() != 8 {
        return None;
    }
    NaiveDate::from_ymd_opt(digits(&date[..4])?, digits(&date[4..6])?, digits(&date[6..])?)
}

fn qso_time(date: &str, time: &str) -> Option<i64> {
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
    #[case("NAMÉ")]
    #[case("eor")]
    #[case("EOH")]
    fn refuses_invalid_field_names(#[case] name: &str) {
        assert_eq!(
            Qso::from_fields([(name, "x")]),
            Err(QsoError::FieldName(name.to_owned()))
        );
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
    #[case("TIME_ON", "2400")]
    #[case("TIME_ON", "0460")]
    #[case("TIME_ON", "04:05")]
    #[case("TIME_ON", "04056")]
    #[case("TIME_ON", "+40506")]
    #[case("BAND", "41m")]
    #[case("FREQ", "7.041.5")]
    fn refuses_values_the_derived_columns_cannot_use(#[case] name: &'static str, #[case] value: &str) {
        let mut qso = sample();
        qso.set(name, Some(value.into())).unwrap();
        assert_eq!(
            qso.check(),
            Err(QsoError::Invalid {
                name,
                value: value.into()
            })
        );
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

    #[rstest]
    #[case(None, false)]
    #[case(Some("R"), true)]
    #[case(Some("Q"), true)]
    #[case(Some("q"), true)]
    #[case(Some("Y"), false)]
    #[case(Some("N"), false)]
    #[case(Some("I"), false)]
    fn requires_a_card_when_requested_or_queued(#[case] qsl_sent: Option<&str>, #[case] required: bool) {
        let mut qso = sample();
        qso.set("QSL_SENT", qsl_sent.map(str::to_owned)).unwrap();
        assert_eq!(qso.qsl_required(), required);
    }

    #[rstest]
    #[case(None, false)]
    #[case(Some("Y"), true)]
    #[case(Some("V"), true)]
    #[case(Some("y"), true)]
    #[case(Some("R"), false)]
    #[case(Some("N"), false)]
    #[case(Some("I"), false)]
    fn counts_a_card_received_when_confirmed_or_verified(#[case] qsl_rcvd: Option<&str>, #[case] received: bool) {
        let mut qso = sample();
        qso.set("QSL_RCVD", qsl_rcvd.map(str::to_owned)).unwrap();
        assert_eq!(qso.qsl_received(), received);
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
