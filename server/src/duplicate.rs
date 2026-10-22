use std::ops::RangeInclusive;

use crate::qso::{Derived, Qso};

pub const WINDOW_SECONDS: i64 = 15 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateKey {
    pub call: String,
    pub band: String,
    pub mode: String,
    pub qso_time: i64,
}

impl DuplicateKey {
    pub fn of(qso: &Qso, derived: &Derived) -> Option<DuplicateKey> {
        Some(DuplicateKey {
            call: qso.get("CALL")?.to_owned(),
            band: qso.get("BAND")?.to_owned(),
            mode: qso.get("MODE")?.to_owned(),
            qso_time: derived.qso_time,
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
    use crate::qso::tests::sample;

    fn key(call: &str, band: &str, mode: &str, qso_time: i64) -> DuplicateKey {
        DuplicateKey {
            call: call.into(),
            band: band.into(),
            mode: mode.into(),
            qso_time,
        }
    }

    #[test]
    fn takes_the_key_from_a_checked_qso() {
        let qso = sample();
        let derived = qso.check().unwrap().derived;
        assert_eq!(
            DuplicateKey::of(&qso, &derived),
            Some(key("JA1XYZ", "40m", "CW", 1_792_641_906))
        );
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
