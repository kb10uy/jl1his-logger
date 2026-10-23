#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bound {
    pub value: u64,
    pub inclusive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub low: Option<Bound>,
    pub high: Option<Bound>,
}

impl Range {
    pub fn parse(value: &str) -> Option<Range> {
        let bound = |s: &str, inclusive| millionths(s).map(|value| Bound { value, inclusive });
        let range = |low, high| Some(Range { low, high });
        if let Some((low, high)) = value.split_once("..") {
            let open = |s: &str| match s {
                "" => Some(None),
                s => bound(s, true).map(Some),
            };
            let (low, high) = (open(low)?, open(high)?);
            if low.is_none() && high.is_none() {
                return None;
            }
            range(low, high)
        } else if let Some(rest) = value.strip_prefix(">=") {
            range(Some(bound(rest, true)?), None)
        } else if let Some(rest) = value.strip_prefix("<=") {
            range(None, Some(bound(rest, true)?))
        } else if let Some(rest) = value.strip_prefix('>') {
            range(Some(bound(rest, false)?), None)
        } else if let Some(rest) = value.strip_prefix('<') {
            range(None, Some(bound(rest, false)?))
        } else {
            let exact = bound(value, true)?;
            range(Some(exact), Some(exact))
        }
    }

    pub fn contains(&self, value: u64) -> bool {
        self.low.is_none_or(|low| match low.inclusive {
            true => value >= low.value,
            false => value > low.value,
        }) && self.high.is_none_or(|high| match high.inclusive {
            true => value <= high.value,
            false => value < high.value,
        })
    }
}

pub fn millionths(number: &str) -> Option<u64> {
    let number = number.trim();
    let (int, frac) = number.split_once('.').unwrap_or((number, ""));
    if (int.is_empty() && frac.is_empty()) || !int.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let whole: u64 = match int {
        "" => 0,
        int => int.parse().ok()?,
    };
    let mut digits = frac.bytes().map(|b| u64::from(b - b'0')).chain(std::iter::repeat(0));
    let micro = digits.by_ref().take(6).fold(0, |acc, d| acc * 10 + d);
    let round_up = digits.next().is_some_and(|d| d >= 5);
    whole
        .checked_mul(1_000_000)?
        .checked_add(micro)?
        .checked_add(u64::from(round_up))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn at(value: u64, inclusive: bool) -> Option<Bound> {
        Some(Bound { value, inclusive })
    }

    #[rstest]
    #[case("7", Some(7_000_000))]
    #[case("7.041", Some(7_041_000))]
    #[case(" 14.0740 ", Some(14_074_000))]
    #[case(".5", Some(500_000))]
    #[case("7.", Some(7_000_000))]
    #[case("0.0000005", Some(1))]
    #[case("0.0000004", Some(0))]
    #[case("", None)]
    #[case(".", None)]
    #[case("7.0.1", None)]
    #[case("-5", None)]
    #[case("10W", None)]
    fn reads_millionths(#[case] number: &str, #[case] expected: Option<u64>) {
        assert_eq!(millionths(number), expected);
    }

    #[rstest]
    #[case("14..14.35", at(14_000_000, true), at(14_350_000, true))]
    #[case("50..", at(50_000_000, true), None)]
    #[case("..5", None, at(5_000_000, true))]
    #[case(">=5", at(5_000_000, true), None)]
    #[case(">5", at(5_000_000, false), None)]
    #[case("<=5", None, at(5_000_000, true))]
    #[case("<5", None, at(5_000_000, false))]
    #[case("14.074", at(14_074_000, true), at(14_074_000, true))]
    fn parses_ranges(#[case] value: &str, #[case] low: Option<Bound>, #[case] high: Option<Bound>) {
        assert_eq!(Range::parse(value), Some(Range { low, high }));
    }

    #[rstest]
    #[case("..")]
    #[case("")]
    #[case(">=")]
    #[case("a..5")]
    #[case("1..b")]
    fn rejects_bad_ranges(#[case] value: &str) {
        assert_eq!(Range::parse(value), None);
    }

    #[rstest]
    #[case("5..10", 5, true)]
    #[case("5..10", 10, true)]
    #[case("5..10", 11, false)]
    #[case(">5", 5, false)]
    #[case("<5", 4, true)]
    #[case("<5", 5, false)]
    #[case("5", 5, true)]
    #[case("5", 6, false)]
    fn tells_whether_a_range_holds(#[case] range: &str, #[case] whole: u64, #[case] expected: bool) {
        assert_eq!(Range::parse(range).unwrap().contains(whole * 1_000_000), expected);
    }
}
