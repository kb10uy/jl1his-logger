use std::ops::RangeInclusive;

use crate::domain::{
    band::{Band, parse_frequency_hz},
    qso::{Qso, QsoError},
};

const WINDOW_SECONDS: i64 = 15 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateKey {
    pub call: String,
    pub band: String,
    pub modes: Vec<String>,
    pub qso_time: i64,
    pub station_callsign: Option<String>,
}

impl DuplicateKey {
    pub fn of(qso: &Qso) -> Result<DuplicateKey, QsoError> {
        let call = qso.get("CALL").ok_or(QsoError::Missing("CALL"))?;
        let modes: Vec<String> = ["MODE", "SUBMODE"]
            .iter()
            .filter_map(|name| qso.get(name))
            .map(|m| m.trim().to_owned())
            .filter(|m| !m.is_empty())
            .collect();
        if modes.is_empty() {
            return Err(QsoError::Missing("MODE"));
        }
        Ok(DuplicateKey {
            call: call.trim().to_owned(),
            band: band(qso)?,
            modes,
            qso_time: qso.qso_time()?,
            station_callsign: qso.get("STATION_CALLSIGN").map(|s| s.trim().to_owned()),
        })
    }

    pub fn time_range(&self) -> RangeInclusive<i64> {
        self.qso_time - WINDOW_SECONDS..=self.qso_time + WINDOW_SECONDS
    }

    pub fn matches(&self, other: &DuplicateKey) -> bool {
        self.call.eq_ignore_ascii_case(&other.call)
            && self.band.eq_ignore_ascii_case(&other.band)
            && self
                .modes
                .iter()
                .any(|m| other.modes.iter().any(|n| m.eq_ignore_ascii_case(n)))
            && self.time_range().contains(&other.qso_time)
    }

    pub fn same_station(&self, other: &DuplicateKey) -> bool {
        match (&self.station_callsign, &other.station_callsign) {
            (None, _) => true,
            (Some(mine), Some(theirs)) => mine.eq_ignore_ascii_case(theirs),
            (Some(_), None) => false,
        }
    }
}

fn band(qso: &Qso) -> Result<String, QsoError> {
    match qso.get("BAND") {
        Some(band) => Ok(band.trim().to_owned()),
        None => {
            let freq = qso.get("FREQ").ok_or(QsoError::Missing("BAND"))?;
            parse_frequency_hz(freq)
                .and_then(Band::of_frequency)
                .map(|band| band.name.to_owned())
                .ok_or_else(|| QsoError::invalid("FREQ", freq))
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::domain::qso::tests::{sample, with};

    fn key(call: &str, band: &str, mode: &str, qso_time: i64) -> DuplicateKey {
        DuplicateKey {
            call: call.into(),
            band: band.into(),
            modes: vec![mode.into()],
            qso_time,
            station_callsign: None,
        }
    }

    #[test]
    fn takes_the_key_from_a_qso() {
        let key = DuplicateKey::of(&sample()).unwrap();
        assert_eq!(
            key,
            DuplicateKey {
                call: "JA1XYZ".into(),
                band: "40m".into(),
                modes: vec!["CW".into()],
                qso_time: 1_792_641_906,
                station_callsign: Some("JL1HIS".into()),
            }
        );
        let mut qso = with(&[("CALL", " ja1xyz "), ("SUBMODE", "PCW")]);
        qso.set("STATION_CALLSIGN", None).unwrap();
        let key = DuplicateKey::of(&qso).unwrap();
        assert_eq!((key.call.as_str(), key.station_callsign), ("ja1xyz", None));
        assert_eq!(key.modes, ["CW", "PCW"]);
        qso.set("MODE", None).unwrap();
        qso.set("SUBMODE", None).unwrap();
        assert_eq!(DuplicateKey::of(&qso), Err(QsoError::Missing("MODE")));
    }

    #[test]
    fn takes_the_band_from_the_frequency_when_band_is_missing() {
        let mut qso = sample();
        qso.set("BAND", None).unwrap();
        assert_eq!(DuplicateKey::of(&qso).unwrap().band, "40m");
        qso.set("FREQ", Some("99.9".into())).unwrap();
        assert_eq!(DuplicateKey::of(&qso), Err(QsoError::invalid("FREQ", "99.9")));
        qso.set("FREQ", None).unwrap();
        assert_eq!(DuplicateKey::of(&qso), Err(QsoError::Missing("BAND")));
    }

    #[rstest]
    #[case::same(key("JA1XYZ", "40m", "CW", 1000), true)]
    #[case::case_differs(key("ja1xyz", "40M", "cw", 1000), true)]
    #[case::window_start(key("JA1XYZ", "40m", "CW", 1000 - WINDOW_SECONDS), true)]
    #[case::window_end(key("JA1XYZ", "40m", "CW", 1000 + WINDOW_SECONDS), true)]
    #[case::before_window(key("JA1XYZ", "40m", "CW", 1000 - WINDOW_SECONDS - 1), false)]
    #[case::after_window(key("JA1XYZ", "40m", "CW", 1000 + WINDOW_SECONDS + 1), false)]
    #[case::portable(key("JA1XYZ/P", "40m", "CW", 1000), false)]
    #[case::other_band(key("JA1XYZ", "20m", "CW", 1000), false)]
    #[case::other_mode(key("JA1XYZ", "40m", "SSB", 1000), false)]
    fn matches_the_same_call_band_and_mode_nearby(#[case] other: DuplicateKey, #[case] duplicate: bool) {
        let key = key("JA1XYZ", "40m", "CW", 1000);
        assert_eq!(key.matches(&other), duplicate);
        assert_eq!(other.matches(&key), duplicate);
    }

    #[test]
    fn matches_a_submode_against_a_mode() {
        let stored = DuplicateKey::of(&with(&[("MODE", "MFSK"), ("SUBMODE", "FT4")])).unwrap();
        let report = DuplicateKey::of(&with(&[("MODE", "FT4")])).unwrap();
        assert!(report.matches(&stored));
        assert!(stored.matches(&report));
    }

    #[rstest]
    #[case::both_given(Some("JL1HIS"), Some("jl1his"), true)]
    #[case::other_station(Some("JL1HIS"), Some("JL1HIS/P"), false)]
    #[case::mine_missing(None, Some("JL1HIS"), true)]
    #[case::theirs_missing(Some("JL1HIS"), None, false)]
    fn compares_stations_only_when_it_has_one(
        #[case] mine: Option<&str>,
        #[case] theirs: Option<&str>,
        #[case] same: bool,
    ) {
        let station = |s: Option<&str>| DuplicateKey {
            station_callsign: s.map(str::to_owned),
            ..key("JA1XYZ", "40m", "CW", 1000)
        };
        assert_eq!(station(mine).same_station(&station(theirs)), same);
    }
}
