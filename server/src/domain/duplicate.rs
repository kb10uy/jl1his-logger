use std::ops::RangeInclusive;

use crate::domain::qso::{Qso, QsoError};

pub const WINDOW_SECONDS: i64 = 15 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateKey {
    pub call: String,
    pub band: String,
    pub mode: String,
    pub qso_time: i64,
}

impl DuplicateKey {
    pub fn of(qso: &Qso) -> Result<DuplicateKey, QsoError> {
        let required = |name: &'static str| qso.get(name).map(str::to_owned).ok_or(QsoError::Missing(name));
        Ok(DuplicateKey {
            call: required("CALL")?,
            band: required("BAND")?,
            mode: required("MODE")?,
            qso_time: qso.qso_time()?,
        })
    }

    pub fn time_range(&self) -> RangeInclusive<i64> {
        self.qso_time - WINDOW_SECONDS..=self.qso_time + WINDOW_SECONDS
    }

    pub fn matches(&self, other: &DuplicateKey) -> bool {
        let same = |a: &str, b: &str| a.trim().eq_ignore_ascii_case(b.trim());
        same(&self.call, &other.call)
            && same(&self.band, &other.band)
            && same(&self.mode, &other.mode)
            && self.time_range().contains(&other.qso_time)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::domain::qso::tests::sample;

    fn key(call: &str, band: &str, mode: &str, qso_time: i64) -> DuplicateKey {
        DuplicateKey {
            call: call.into(),
            band: band.into(),
            mode: mode.into(),
            qso_time,
        }
    }

    #[test]
    fn takes_the_key_from_a_qso() {
        let mut qso = sample();
        assert_eq!(DuplicateKey::of(&qso), Ok(key("JA1XYZ", "40m", "CW", 1_792_641_906)));
        qso.set("STATION_CALLSIGN", None).unwrap();
        assert!(DuplicateKey::of(&qso).is_ok());
        qso.set("MODE", None).unwrap();
        assert_eq!(DuplicateKey::of(&qso), Err(QsoError::Missing("MODE")));
    }

    #[rstest]
    #[case::same(key("JA1XYZ", "40m", "CW", 1000), true)]
    #[case::case_and_spaces(key(" ja1xyz", "40M", "cw", 1000), true)]
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
}
