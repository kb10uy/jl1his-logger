#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Day(i64);

impl Day {
    pub fn new(year: i64, month: u32, day: u32) -> Option<Day> {
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let days_in_month = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return None,
        };
        (1..=days_in_month)
            .contains(&day)
            .then(|| Day(days_from_civil(year, month, day)))
    }

    pub fn parse_iso(s: &str) -> Option<Day> {
        let b = s.as_bytes();
        if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
            return None;
        }
        Day::from_digits(s.get(0..4)?, s.get(5..7)?, s.get(8..10)?)
    }

    pub fn parse_adif(s: &str) -> Option<Day> {
        if s.len() != 8 {
            return None;
        }
        Day::from_digits(s.get(0..4)?, s.get(4..6)?, s.get(6..8)?)
    }

    fn from_digits(year: &str, month: &str, day: &str) -> Option<Day> {
        fn digits(s: &str) -> Option<&str> {
            s.bytes().all(|b| b.is_ascii_digit()).then_some(s)
        }
        Day::new(
            digits(year)?.parse().ok()?,
            digits(month)?.parse().ok()?,
            digits(day)?.parse().ok()?,
        )
    }

    pub fn days_since_epoch(self) -> i64 {
        self.0
    }

    pub fn unix_seconds(self) -> i64 {
        self.0 * 86_400
    }

    pub fn next(self) -> Day {
        Day(self.0 + 1)
    }
}

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("1970-01-01", Some(0))]
    #[case("2026-10-23", Some(20_749))]
    #[case("2000-02-29", Some(11_016))]
    #[case("1900-02-29", None)]
    #[case("2026-13-01", None)]
    #[case("2026-1-01", None)]
    #[case("2026/10/23", None)]
    #[case("+026-10-23", None)]
    #[case("２０２6-10-23", None)]
    fn parses_iso_days(#[case] input: &str, #[case] expected: Option<i64>) {
        assert_eq!(Day::parse_iso(input).map(Day::days_since_epoch), expected);
    }

    #[rstest]
    #[case("20261023", Some(20_749))]
    #[case("2026102", None)]
    #[case("2026-10-2", None)]
    fn parses_adif_days(#[case] input: &str, #[case] expected: Option<i64>) {
        assert_eq!(Day::parse_adif(input).map(Day::days_since_epoch), expected);
    }
}
