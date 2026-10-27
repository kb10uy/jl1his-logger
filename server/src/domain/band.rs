pub use jl1his_query::millionths as parse_frequency_hz;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Band {
    pub name: &'static str,
    pub lower_hz: u64,
    pub upper_hz: u64,
}

const fn band(name: &'static str, lower_hz: u64, upper_hz: u64) -> Band {
    Band {
        name,
        lower_hz,
        upper_hz,
    }
}

include!(concat!(env!("OUT_DIR"), "/adif/bands.rs"));

impl Band {
    pub fn named(name: &str) -> Option<&'static Band> {
        BANDS.iter().find(|b| b.name.eq_ignore_ascii_case(name.trim()))
    }

    pub fn of_frequency(hz: u64) -> Option<&'static Band> {
        BANDS.iter().find(|b| b.contains(hz))
    }

    pub fn contains(&self, hz: u64) -> bool {
        (self.lower_hz..=self.upper_hz).contains(&hz)
    }
}

pub fn format_frequency_mhz(hz: u64) -> String {
    let micro = format!("{:06}", hz % 1_000_000);
    match micro.trim_end_matches('0') {
        "" => format!("{}", hz / 1_000_000),
        micro => format!("{}.{micro}", hz / 1_000_000),
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(0, band("2190m", 135_700, 137_800))]
    #[case(15, band("5m", 54_000_001, 69_900_000))]
    #[case(32, band("submm", 300_000_000_000, 7_500_000_000_000))]
    fn follows_the_adif_band_enumeration(#[case] index: usize, #[case] expected: Band) {
        assert_eq!(BANDS[index], expected);
    }

    #[rstest]
    #[case(7_041_500, "7.0415")]
    #[case(432_940_000, "432.94")]
    #[case(14_000_000, "14")]
    #[case(135_700, "0.1357")]
    #[case(10_489_550_001, "10489.550001")]
    fn formats_frequencies_in_mhz(#[case] hz: u64, #[case] expected: &str) {
        assert_eq!(format_frequency_mhz(hz), expected);
        assert_eq!(parse_frequency_hz(expected), Some(hz));
    }

    #[rstest]
    #[case("7.041", Some(7_041_000))]
    #[case("7", Some(7_000_000))]
    #[case("432.94", Some(432_940_000))]
    #[case(" 14.0745 ", Some(14_074_500))]
    #[case("0.1365", Some(136_500))]
    #[case(".475", Some(475_000))]
    #[case("7.", Some(7_000_000))]
    #[case("7.0410004", Some(7_041_000))]
    #[case("7.0410005", Some(7_041_001))]
    #[case("", None)]
    #[case(".", None)]
    #[case("-7.041", None)]
    #[case("7.041.5", None)]
    #[case("7,041", None)]
    #[case("7.041MHz", None)]
    #[case("99999999999999999999", None)]
    fn parses_frequencies_in_mhz(#[case] freq: &str, #[case] hz: Option<u64>) {
        assert_eq!(parse_frequency_hz(freq), hz);
    }

    #[rstest]
    #[case(136_000, Some("2190m"))]
    #[case(7_000_000, Some("40m"))]
    #[case(7_300_000, Some("40m"))]
    #[case(54_000_000, Some("6m"))]
    #[case(54_000_001, Some("5m"))]
    #[case(145_500_000, Some("2m"))]
    #[case(432_940_000, Some("70cm"))]
    #[case(100_000_000, None)]
    #[case(7_300_001, None)]
    fn finds_the_band_of_a_frequency(#[case] hz: u64, #[case] name: Option<&str>) {
        assert_eq!(Band::of_frequency(hz).map(|b| b.name), name);
    }

    #[rstest]
    #[case("40m", Some("40m"))]
    #[case("40M", Some("40m"))]
    #[case("70CM", Some("70cm"))]
    #[case("SUBMM", Some("submm"))]
    #[case("41m", None)]
    fn finds_bands_by_name_ignoring_case(#[case] name: &str, #[case] found: Option<&str>) {
        assert_eq!(Band::named(name).map(|b| b.name), found);
    }

    #[test]
    fn bands_are_ordered_and_disjoint() {
        for pair in BANDS.windows(2) {
            assert!(pair[0].lower_hz <= pair[0].upper_hz, "{:?}", pair[0]);
            assert!(pair[0].upper_hz < pair[1].lower_hz, "{:?} {:?}", pair[0], pair[1]);
        }
    }
}
