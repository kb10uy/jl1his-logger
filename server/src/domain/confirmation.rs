use std::ops::RangeInclusive;

use crate::domain::{
    band::{Band, parse_frequency_hz},
    duplicate::WINDOW_SECONDS,
    qsl,
    qso::{Qso, QsoError},
};

pub const REPORTED: [&str; 7] = ["DXCC", "CQZ", "ITUZ", "STATE", "CNTY", "GRIDSQUARE", "IOTA"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Lotw,
    Eqsl,
    Card,
}

pub type Changes = Vec<(&'static str, String)>;

impl Source {
    fn received_field(self) -> &'static str {
        match self {
            Source::Lotw => "LOTW_QSL_RCVD",
            Source::Eqsl => "EQSL_QSL_RCVD",
            Source::Card => "QSL_RCVD",
        }
    }

    fn date_field(self) -> &'static str {
        match self {
            Source::Lotw => "LOTW_QSLRDATE",
            Source::Eqsl => "EQSL_QSLRDATE",
            Source::Card => "QSLRDATE",
        }
    }

    pub fn confirmation(self, record: &Qso, today: &str) -> Result<Option<Changes>, QsoError> {
        let first = |names: &[&str]| names.iter().find_map(|name| record.get(name)).map(str::trim);
        let is = |value: &str, accepted: &[&str]| accepted.iter().any(|a| value.eq_ignore_ascii_case(a));
        let received = match self {
            Source::Lotw => first(&["LOTW_QSL_RCVD", "QSL_RCVD"]).filter(|v| is(v, &["Y"])),
            Source::Eqsl => match first(&["EQSL_QSL_RCVD", "QSL_RCVD"]) {
                None => Some("Y"),
                Some(v) => Some(v).filter(|v| is(v, &["Y"])),
            },
            Source::Card => first(&["QSL_RCVD"]).filter(|v| is(v, &["Y", "V"])),
        };
        let Some(received) = received else {
            return Ok(None);
        };
        let date = first(&[self.date_field(), "QSLRDATE"]).unwrap_or(today);

        let mut changes = vec![
            (self.received_field(), received.to_ascii_uppercase()),
            (self.date_field(), date.to_owned()),
        ];
        if self == Source::Card
            && let Some(via) = record.get("QSL_RCVD_VIA").map(str::trim)
        {
            changes.push(("QSL_RCVD_VIA", via.to_ascii_uppercase()));
        }
        for (field, value) in &changes {
            qsl::check(field, value)?;
        }
        Ok(Some(changes))
    }

    pub fn received(self, qso: &Qso) -> bool {
        qso.get(self.received_field())
            .is_some_and(|v| ["Y", "V"].iter().any(|a| v.trim().eq_ignore_ascii_case(a)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchKey {
    pub call: String,
    pub band: String,
    pub modes: Vec<String>,
    pub qso_time: i64,
    pub station_callsign: Option<String>,
}

impl MatchKey {
    pub fn of(record: &Qso) -> Result<MatchKey, QsoError> {
        let call = record.get("CALL").ok_or(QsoError::Missing("CALL"))?;
        let band = match record.get("BAND") {
            Some(band) => band.trim().to_owned(),
            None => {
                let freq = record.get("FREQ").ok_or(QsoError::Missing("BAND"))?;
                parse_frequency_hz(freq)
                    .and_then(Band::of_frequency)
                    .ok_or_else(|| QsoError::Invalid {
                        name: "FREQ",
                        value: freq.to_owned(),
                    })?
                    .name
                    .to_owned()
            }
        };
        let modes = modes(record);
        if modes.is_empty() {
            return Err(QsoError::Missing("MODE"));
        }
        Ok(MatchKey {
            call: call.trim().to_owned(),
            band,
            modes,
            qso_time: record.qso_time()?,
            station_callsign: record.get("STATION_CALLSIGN").map(|s| s.trim().to_owned()),
        })
    }

    pub fn time_range(&self) -> RangeInclusive<i64> {
        self.qso_time - WINDOW_SECONDS..=self.qso_time + WINDOW_SECONDS
    }

    pub fn matches(&self, candidate: &Qso) -> bool {
        let same = |a: &str, b: Option<&str>| b.is_some_and(|b| a.eq_ignore_ascii_case(b.trim()));
        same(&self.call, candidate.get("CALL"))
            && same(&self.band, candidate.get("BAND"))
            && modes(candidate)
                .iter()
                .any(|m| self.modes.iter().any(|n| m.eq_ignore_ascii_case(n)))
            && candidate.qso_time().is_ok_and(|t| self.time_range().contains(&t))
            && self
                .station_callsign
                .as_ref()
                .is_none_or(|s| same(s, candidate.get("STATION_CALLSIGN")))
    }
}

fn modes(qso: &Qso) -> Vec<String> {
    ["MODE", "SUBMODE"]
        .iter()
        .filter_map(|name| qso.get(name))
        .map(|m| m.trim().to_owned())
        .filter(|m| !m.is_empty())
        .collect()
}

pub fn differences(record: &Qso, candidate: &Qso) -> Vec<&'static str> {
    REPORTED
        .iter()
        .copied()
        .filter(|name| match record.get(name).map(str::trim).filter(|v| !v.is_empty()) {
            None => false,
            Some(value) => !candidate
                .get(name)
                .is_some_and(|c| c.trim().eq_ignore_ascii_case(value)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::domain::qso::tests::sample;

    fn record(fields: &[(&str, &str)]) -> Qso {
        Qso::from_fields(fields.iter().copied()).unwrap()
    }

    #[rstest]
    #[case::lotw_report(
        Source::Lotw,
        &[("QSL_RCVD", "Y"), ("QSLRDATE", "20261101")],
        Some(vec![("LOTW_QSL_RCVD", "Y"), ("LOTW_QSLRDATE", "20261101")])
    )]
    #[case::lotw_own_fields_first(
        Source::Lotw,
        &[("LOTW_QSL_RCVD", "y"), ("LOTW_QSLRDATE", "20261102"), ("QSL_RCVD", "N"), ("QSLRDATE", "20261101")],
        Some(vec![("LOTW_QSL_RCVD", "Y"), ("LOTW_QSLRDATE", "20261102")])
    )]
    #[case::lotw_unconfirmed(Source::Lotw, &[("QSL_RCVD", "N")], None)]
    #[case::lotw_without_a_field(Source::Lotw, &[], None)]
    #[case::eqsl_inbox(Source::Eqsl, &[], Some(vec![("EQSL_QSL_RCVD", "Y"), ("EQSL_QSLRDATE", "20261122")]))]
    #[case::eqsl_dated(
        Source::Eqsl,
        &[("EQSL_QSL_RCVD", "Y"), ("EQSL_QSLRDATE", "20261103")],
        Some(vec![("EQSL_QSL_RCVD", "Y"), ("EQSL_QSLRDATE", "20261103")])
    )]
    #[case::eqsl_requested(Source::Eqsl, &[("EQSL_QSL_RCVD", "R")], None)]
    #[case::card_verified(
        Source::Card,
        &[("QSL_RCVD", "v"), ("QSL_RCVD_VIA", "b")],
        Some(vec![("QSL_RCVD", "V"), ("QSLRDATE", "20261122"), ("QSL_RCVD_VIA", "B")])
    )]
    #[case::card_absent(Source::Card, &[], None)]
    fn reads_the_confirmation_of_each_source(
        #[case] source: Source,
        #[case] fields: &[(&str, &str)],
        #[case] expected: Option<Vec<(&str, &str)>>,
    ) {
        let mut qso = sample();
        for (name, value) in fields {
            qso.set(name, Some((*value).to_owned())).unwrap();
        }
        let expected = expected.map(|c| c.into_iter().map(|(f, v)| (f, v.to_owned())).collect::<Vec<_>>());
        assert_eq!(source.confirmation(&qso, "20261122").unwrap(), expected);
    }

    #[test]
    fn refuses_a_malformed_confirmation_date() {
        let qso = record(&[("QSL_RCVD", "Y"), ("QSLRDATE", "2026-11-01")]);
        assert!(Source::Lotw.confirmation(&qso, "20261122").is_err());
    }

    #[rstest]
    #[case(Source::Lotw, &[("LOTW_QSL_RCVD", "Y")], true)]
    #[case(Source::Lotw, &[("QSL_RCVD", "Y")], false)]
    #[case(Source::Card, &[("QSL_RCVD", "v")], true)]
    #[case(Source::Card, &[("QSL_RCVD", "R")], false)]
    #[case(Source::Eqsl, &[], false)]
    fn knows_when_a_qso_is_already_received(
        #[case] source: Source,
        #[case] fields: &[(&str, &str)],
        #[case] received: bool,
    ) {
        assert_eq!(source.received(&record(fields)), received);
    }

    #[rstest]
    #[case::same(&[], true)]
    #[case::case_differs(&[("CALL", "ja1xyz"), ("BAND", "40M"), ("MODE", "cw")], true)]
    #[case::fourteen_minutes_later(&[("TIME_ON", "041906")], true)]
    #[case::sixteen_minutes_later(&[("TIME_ON", "042106")], false)]
    #[case::other_call(&[("CALL", "JA1XYZ/P")], false)]
    #[case::other_band(&[("BAND", "20m")], false)]
    #[case::other_mode(&[("MODE", "SSB")], false)]
    #[case::other_station(&[("STATION_CALLSIGN", "JL1HIS/P")], false)]
    fn matches_call_band_mode_time_and_station(#[case] changes: &[(&str, &str)], #[case] matched: bool) {
        let mut candidate = sample();
        for (name, value) in changes {
            candidate.set(name, Some((*value).to_owned())).unwrap();
        }
        assert_eq!(MatchKey::of(&sample()).unwrap().matches(&candidate), matched);
    }

    #[test]
    fn matches_a_submode_against_a_mode() {
        let mut stored = sample();
        stored.set("MODE", Some("MFSK".into())).unwrap();
        stored.set("SUBMODE", Some("FT4".into())).unwrap();
        let mut report = sample();
        report.set("MODE", Some("FT4".into())).unwrap();
        assert!(MatchKey::of(&report).unwrap().matches(&stored));
    }

    #[test]
    fn takes_the_band_from_the_frequency_and_ignores_a_missing_station() {
        let mut report = sample();
        report.set("BAND", None).unwrap();
        report.set("STATION_CALLSIGN", None).unwrap();
        let key = MatchKey::of(&report).unwrap();
        assert_eq!((key.band.as_str(), key.station_callsign.as_deref()), ("40m", None));
        assert!(key.matches(&sample()));

        report.set("FREQ", None).unwrap();
        assert_eq!(MatchKey::of(&report), Err(QsoError::Missing("BAND")));
    }

    #[test]
    fn lists_the_reported_fields_that_differ() {
        let mut stored = sample();
        stored.set("GRIDSQUARE", Some("PM95".into())).unwrap();
        stored.set("DXCC", Some("339".into())).unwrap();
        let report = record(&[("GRIDSQUARE", "pm95"), ("DXCC", "339"), ("CQZ", "25"), ("STATE", "")]);
        assert_eq!(differences(&report, &stored), ["CQZ"]);
    }
}
