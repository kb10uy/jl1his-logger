use crate::{
    callsign,
    date::Day,
    parse::{Condition, Term},
};

pub trait Record {
    fn field(&self, name: &str) -> Option<&str>;
    fn station_name(&self) -> Option<&str>;
}

fn value<'r>(record: &'r impl Record, name: &str) -> Option<&'r str> {
    record
        .field(name)
        .map(|v| v.trim_matches(' '))
        .filter(|v| !v.is_empty())
}

fn contains(haystack: Option<&str>, needle: &str) -> bool {
    haystack.is_some_and(|h| h.to_ascii_lowercase().contains(needle))
}

impl Term {
    pub fn matches(&self, record: &impl Record) -> bool {
        self.condition.matches(record) != self.negated
    }
}

impl Condition {
    fn matches(&self, record: &impl Record) -> bool {
        match self {
            Condition::Call(patterns) => value(record, "CALL")
                .map(str::to_ascii_uppercase)
                .is_some_and(|call| patterns.iter().any(|p| wildcard(p, &call))),
            Condition::Base(bases) => record
                .field("CALL")
                .map(callsign::base)
                .is_some_and(|base| bases.contains(&base)),
            Condition::Field { name, values } => {
                value(record, name).is_some_and(|v| values.iter().any(|x| x.eq_ignore_ascii_case(v)))
            }
            Condition::Station(needles) => needles.iter().any(|needle| {
                contains(record.station_name(), needle) || contains(record.field("STATION_CALLSIGN"), needle)
            }),
            Condition::Park(parks) => record.field("MY_POTA_REF").is_some_and(|refs| {
                refs.replace(' ', "")
                    .split(',')
                    .any(|r| parks.iter().any(|p| p.eq_ignore_ascii_case(r)))
            }),
            Condition::Date { from, until } => value(record, "QSO_DATE")
                .and_then(Day::parse_adif)
                .is_some_and(|day| from.is_none_or(|from| day >= from) && until.is_none_or(|until| day < until)),
            Condition::Present(name) => value(record, name).is_some(),
            Condition::Word(word) => ["CALL", "NAME", "QTH", "COMMENT"]
                .iter()
                .any(|name| contains(record.field(name), word)),
        }
    }
}

fn wildcard(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some((p, t));
                p += 1;
            }
            Some(&c) if c == '?' || c == text[t] => {
                p += 1;
                t += 1;
            }
            _ => match star {
                Some((sp, st)) => {
                    p = sp + 1;
                    t = st + 1;
                    star = Some((sp, st + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rstest::rstest;

    use crate::Query;

    use super::*;

    struct Row {
        fields: HashMap<&'static str, &'static str>,
        station: Option<&'static str>,
    }

    impl Record for Row {
        fn field(&self, name: &str) -> Option<&str> {
            self.fields.get(name).copied()
        }

        fn station_name(&self) -> Option<&str> {
            self.station
        }
    }

    fn row() -> Row {
        Row {
            fields: HashMap::from([
                ("CALL", "JA1XYZ/P"),
                ("QSO_DATE", "20261019"),
                ("TIME_ON", "0931"),
                ("BAND", "40m"),
                ("MODE", "SSB"),
                ("QSL_SENT", "r"),
                ("STATION_CALLSIGN", "JL1HIS/1"),
                ("MY_POTA_REF", "JP-0012, JP-0345"),
                ("NAME", "Yuki"),
                ("QTH", "Yokohama"),
                ("FREQ", " "),
                ("TX_PWR", "10"),
            ]),
            station: Some("Mt. Takao"),
        }
    }

    #[rstest]
    #[case("", true)]
    #[case("call:JA1*", true)]
    #[case("call:ja1xyz/?", true)]
    #[case("call:JA1XYZ", false)]
    #[case("call:*XYZ*", true)]
    #[case("call:J*Z", false)]
    #[case("call:J*P", true)]
    #[case("base:JA1XYZ", true)]
    #[case("base:ja1xyz/qrp", true)]
    #[case("band:20m,40M", true)]
    #[case("-band:40m", false)]
    #[case("mode:FT8", false)]
    #[case("sent:R,Q", true)]
    #[case("rcvd:N", false)]
    #[case("no:rcvd", true)]
    #[case("station:takao", true)]
    #[case("station:his/1", true)]
    #[case("station:home", false)]
    #[case("park:JP-0345", true)]
    #[case("park:JP-034", false)]
    #[case("date:2026-10-19", true)]
    #[case("date:2026-10-01..2026-10-18", false)]
    #[case("date:>=2026-10-19", true)]
    #[case("date:>2026-10-19", false)]
    #[case("date:<2026-10-20", true)]
    #[case("no:freq", true)]
    #[case("has:freq", false)]
    #[case("has:park", true)]
    #[case("adif.TX_PWR:10", true)]
    #[case("adif.tx_pwr:5,20", false)]
    #[case("adif.GRIDSQUARE:PM95", false)]
    #[case("yokohama", true)]
    #[case("yuk", true)]
    #[case("-yuki", false)]
    #[case("tokyo", false)]
    #[case("call:JA1* band:40m sent:R", true)]
    #[case("call:JA1* band:20m", false)]
    fn matches_rows(#[case] query: &str, #[case] expected: bool) {
        assert_eq!(Query::parse(query).unwrap().matches(&row()), expected);
    }

    #[rstest]
    #[case("A*", "ABC", true)]
    #[case("*", "", true)]
    #[case("A?C", "ABC", true)]
    #[case("A?C", "AC", false)]
    #[case("*B*B", "ABAB", true)]
    #[case("*B*B", "ABA", false)]
    #[case("ABC", "ABCD", false)]
    fn matches_wildcards(#[case] pattern: &str, #[case] text: &str, #[case] expected: bool) {
        assert_eq!(wildcard(pattern, text), expected);
    }
}
