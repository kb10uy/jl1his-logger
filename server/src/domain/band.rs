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

pub const BANDS: &[Band] = &[
    band("2190m", 135_700, 137_800),
    band("630m", 472_000, 479_000),
    band("560m", 501_000, 504_000),
    band("160m", 1_800_000, 2_000_000),
    band("80m", 3_500_000, 4_000_000),
    band("60m", 5_060_000, 5_450_000),
    band("40m", 7_000_000, 7_300_000),
    band("30m", 10_100_000, 10_150_000),
    band("20m", 14_000_000, 14_350_000),
    band("17m", 18_068_000, 18_168_000),
    band("15m", 21_000_000, 21_450_000),
    band("12m", 24_890_000, 24_990_000),
    band("10m", 28_000_000, 29_700_000),
    band("8m", 40_000_000, 45_000_000),
    band("6m", 50_000_000, 54_000_000),
    band("5m", 54_000_001, 69_900_000),
    band("4m", 70_000_000, 71_000_000),
    band("2m", 144_000_000, 148_000_000),
    band("1.25m", 222_000_000, 225_000_000),
    band("70cm", 420_000_000, 450_000_000),
    band("33cm", 902_000_000, 928_000_000),
    band("23cm", 1_240_000_000, 1_300_000_000),
    band("13cm", 2_300_000_000, 2_450_000_000),
    band("9cm", 3_300_000_000, 3_500_000_000),
    band("6cm", 5_650_000_000, 5_925_000_000),
    band("3cm", 10_000_000_000, 10_500_000_000),
    band("1.25cm", 24_000_000_000, 24_250_000_000),
    band("6mm", 47_000_000_000, 47_200_000_000),
    band("4mm", 75_500_000_000, 81_000_000_000),
    band("2.5mm", 119_980_000_000, 123_000_000_000),
    band("2mm", 134_000_000_000, 149_000_000_000),
    band("1mm", 241_000_000_000, 250_000_000_000),
    band("submm", 300_000_000_000, 7_500_000_000_000),
];

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

pub fn parse_frequency_hz(freq: &str) -> Option<u64> {
    let (int, frac) = freq.trim().split_once('.').unwrap_or((freq.trim(), ""));
    if (int.is_empty() && frac.is_empty()) || !int.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mhz: u64 = match int {
        "" => 0,
        int => int.parse().ok()?,
    };
    let mut digits = frac.bytes().map(|b| u64::from(b - b'0')).chain(std::iter::repeat(0));
    let micro = digits.by_ref().take(6).fold(0, |acc, d| acc * 10 + d);
    let round_up = digits.next().is_some_and(|d| d >= 5);
    mhz.checked_mul(1_000_000)?
        .checked_add(micro)?
        .checked_add(u64::from(round_up))
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
    use serde::Deserialize;

    use super::*;

    #[derive(Deserialize)]
    struct Fixture {
        bands: Vec<FixtureBand>,
    }

    #[derive(Deserialize)]
    struct FixtureBand {
        band: String,
        lower_hz: u64,
        upper_hz: u64,
    }

    #[test]
    fn matches_the_shared_band_table() {
        let fixture: Fixture = serde_json::from_str(include_str!("../../../fixtures/reference/bands.json")).unwrap();
        let bands: Vec<(&str, u64, u64)> = BANDS.iter().map(|b| (b.name, b.lower_hz, b.upper_hz)).collect();
        let expected: Vec<(&str, u64, u64)> = fixture
            .bands
            .iter()
            .map(|b| (b.band.as_str(), b.lower_hz, b.upper_hz))
            .collect();
        assert_eq!(bands, expected);
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
