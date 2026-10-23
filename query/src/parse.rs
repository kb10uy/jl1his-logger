use std::fmt;

use crate::{callsign, date::Day, record::Record};

pub const MAX_VALUES: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition {
    Call(Vec<String>),
    Base(Vec<String>),
    Field { name: String, values: Vec<String> },
    Station(Vec<String>),
    Park(Vec<String>),
    Date { from: Option<Day>, until: Option<Day> },
    Present(String),
    Word(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    pub negated: bool,
    pub condition: Condition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    UnclosedQuote,
    Empty,
    UnknownField(String),
    BadFieldName(String),
    MissingValue,
    TooManyValues(usize),
    BadDate(String),
}

impl ErrorKind {
    pub fn code(&self) -> &'static str {
        match self {
            ErrorKind::UnclosedQuote => "unclosed_quote",
            ErrorKind::Empty => "empty",
            ErrorKind::UnknownField(_) => "unknown_field",
            ErrorKind::BadFieldName(_) => "bad_field_name",
            ErrorKind::MissingValue => "missing_value",
            ErrorKind::TooManyValues(_) => "too_many_values",
            ErrorKind::BadDate(_) => "bad_date",
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorKind::UnclosedQuote => write!(f, "a quote is not closed"),
            ErrorKind::Empty => write!(f, "the term is empty"),
            ErrorKind::UnknownField(key) => write!(f, "{key} is not a search field"),
            ErrorKind::BadFieldName(name) => write!(f, "{name:?} is not an ADIF field name"),
            ErrorKind::MissingValue => write!(f, "the term has no value"),
            ErrorKind::TooManyValues(n) => write!(f, "the term lists {n} values; at most {MAX_VALUES} are taken"),
            ErrorKind::BadDate(value) => write!(f, "{value:?} is not a date or a date range"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    pub index: usize,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (term {:?} at {})", self.kind, self.text, self.start)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub negated: bool,
    pub key: Option<String>,
    pub field: Option<String>,
    pub values: Vec<String>,
    pub term: Result<Term, ErrorKind>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    terms: Vec<Term>,
}

impl Query {
    pub fn parse(query: &str) -> Result<Query, Error> {
        let terms = tokens(query)
            .into_iter()
            .enumerate()
            .map(|(index, token)| {
                token.term.map_err(|kind| Error {
                    kind,
                    index,
                    start: token.start,
                    end: token.end,
                    text: token.text,
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(Query { terms })
    }

    pub fn terms(&self) -> &[Term] {
        &self.terms
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    pub fn matches(&self, record: &impl Record) -> bool {
        self.terms.iter().all(|term| term.matches(record))
    }
}

pub fn tokens(query: &str) -> Vec<Token> {
    let chars: Vec<char> = query.chars().collect();
    let mut tokens = vec![];
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        let mut quoted = false;
        while i < chars.len() && (quoted || !chars[i].is_whitespace()) {
            if chars[i] == '"' {
                quoted = !quoted;
            }
            i += 1;
        }
        tokens.push(token(start, i, chars[start..i].iter().collect(), quoted));
    }
    tokens
}

fn token(start: usize, end: usize, text: String, unclosed: bool) -> Token {
    let (negated, body) = match text.strip_prefix('-') {
        Some(rest) if !rest.is_empty() => (true, rest),
        _ => (false, text.as_str()),
    };
    let mut token = Token {
        start,
        end,
        text: text.clone(),
        negated,
        key: None,
        field: None,
        values: vec![],
        term: Err(ErrorKind::UnclosedQuote),
    };
    let colon = outside_quotes(body).find(|&(_, c)| c == ':').map(|(at, _)| at);
    let mut absent = false;
    let condition = match colon {
        None => {
            let word = unquote(body).trim_matches(' ').to_owned();
            token.values = vec![word.clone()];
            match word.is_empty() {
                true => Err(ErrorKind::Empty),
                false => Ok(Condition::Word(word.to_ascii_lowercase())),
            }
        }
        Some(at) => {
            let key = body[..at].to_ascii_lowercase();
            absent = key == "no";
            token.values = split_values(&body[at + 1..]);
            let condition = condition(&key, &body[..at], &token.values, &mut token.field);
            token.key = Some(match key.strip_prefix("adif.") {
                Some(_) => "adif".to_owned(),
                None => key,
            });
            condition
        }
    };
    token.term = match unclosed {
        true => Err(ErrorKind::UnclosedQuote),
        false => condition.map(|condition| Term {
            negated: negated != absent,
            condition,
        }),
    };
    token
}

fn condition(key: &str, written: &str, values: &[String], field: &mut Option<String>) -> Result<Condition, ErrorKind> {
    let adif = match key.strip_prefix("adif.") {
        Some(_) => {
            let name = field_name(&written["adif.".len()..])?;
            *field = Some(name.clone());
            Some(name)
        }
        None => None,
    };
    let enumerated = |name: &str| Some(name.to_owned());
    let field_of_key = match key {
        "band" => enumerated("BAND"),
        "mode" => enumerated("MODE"),
        "contest" => enumerated("CONTEST_ID"),
        "sent" => enumerated("QSL_SENT"),
        "rcvd" => enumerated("QSL_RCVD"),
        "dxcc" => enumerated("DXCC"),
        _ => adif,
    };
    let known = field_of_key.is_some() || matches!(key, "call" | "base" | "station" | "park" | "date" | "has" | "no");
    if !known {
        return Err(ErrorKind::UnknownField(key.to_owned()));
    }
    if values.is_empty() {
        return Err(ErrorKind::MissingValue);
    }
    if values.len() > MAX_VALUES {
        return Err(ErrorKind::TooManyValues(values.len()));
    }
    let mapped = |f: fn(&str) -> String| values.iter().map(|v| f(v)).collect::<Vec<_>>();
    Ok(match (key, field_of_key) {
        (_, Some(name)) => Condition::Field {
            name,
            values: mapped(|v| v.to_ascii_uppercase()),
        },
        ("call", None) => Condition::Call(mapped(|v| v.to_ascii_uppercase())),
        ("base", None) => Condition::Base(mapped(callsign::base)),
        ("station", None) => Condition::Station(mapped(|v| v.to_ascii_lowercase())),
        ("park", None) => Condition::Park(mapped(|v| v.replace(' ', "").to_ascii_uppercase())),
        ("date", None) => date(values)?,
        (_, None) => {
            let [value] = values else {
                return Err(ErrorKind::BadFieldName(values.join(",")));
            };
            let name = match value.to_ascii_lowercase().as_str() {
                "park" => "MY_POTA_REF".to_owned(),
                "sent" => "QSL_SENT".to_owned(),
                "rcvd" => "QSL_RCVD".to_owned(),
                "contest" => "CONTEST_ID".to_owned(),
                _ => field_name(value)?,
            };
            *field = Some(name.clone());
            Condition::Present(name)
        }
    })
}

fn date(values: &[String]) -> Result<Condition, ErrorKind> {
    let [value] = values else {
        return Err(ErrorKind::BadDate(values.join(",")));
    };
    let bad = || ErrorKind::BadDate(value.clone());
    let day = |s: &str| Day::parse_iso(s).ok_or_else(bad);
    let (from, until) = if let Some((from, until)) = value.split_once("..") {
        let from = (!from.is_empty()).then(|| day(from)).transpose()?;
        let until = (!until.is_empty()).then(|| day(until).map(Day::next)).transpose()?;
        if from.is_none() && until.is_none() {
            return Err(bad());
        }
        (from, until)
    } else if let Some(rest) = value.strip_prefix(">=") {
        (Some(day(rest)?), None)
    } else if let Some(rest) = value.strip_prefix("<=") {
        (None, Some(day(rest)?.next()))
    } else if let Some(rest) = value.strip_prefix('>') {
        (Some(day(rest)?.next()), None)
    } else if let Some(rest) = value.strip_prefix('<') {
        (None, Some(day(rest)?))
    } else {
        let day = day(value)?;
        (Some(day), Some(day.next()))
    };
    Ok(Condition::Date { from, until })
}

fn field_name(name: &str) -> Result<String, ErrorKind> {
    let mut bytes = name.bytes();
    let valid = bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !name.eq_ignore_ascii_case("EOR")
        && !name.eq_ignore_ascii_case("EOH");
    match valid {
        true => Ok(name.to_ascii_uppercase()),
        false => Err(ErrorKind::BadFieldName(name.to_owned())),
    }
}

fn outside_quotes(s: &str) -> impl Iterator<Item = (usize, char)> + '_ {
    let mut quoted = false;
    s.char_indices().filter(move |&(_, c)| {
        if c == '"' {
            quoted = !quoted;
        }
        !quoted && c != '"'
    })
}

fn split_values(value: &str) -> Vec<String> {
    let mut values = vec![];
    let mut start = 0;
    for (at, _) in outside_quotes(value).filter(|&(_, c)| c == ',') {
        values.push(&value[start..at]);
        start = at + 1;
    }
    values.push(&value[start..]);
    values
        .into_iter()
        .map(|v| unquote(v).trim_matches(' ').to_owned())
        .filter(|v| !v.is_empty())
        .collect()
}

fn unquote(s: &str) -> String {
    s.replace('"', "")
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn day(s: &str) -> Option<Day> {
        Day::parse_iso(s)
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[rstest]
    #[case("call:ja1*", false, Condition::Call(strings(&["JA1*"])))]
    #[case("base:JA1XYZ/P", false, Condition::Base(strings(&["JA1XYZ"])))]
    #[case("band:40m,20m", false, Condition::Field { name: "BAND".into(), values: strings(&["40M", "20M"]) })]
    #[case("-mode:FT8", true, Condition::Field { name: "MODE".into(), values: strings(&["FT8"]) })]
    #[case("contest:ALLJA1", false, Condition::Field { name: "CONTEST_ID".into(), values: strings(&["ALLJA1"]) })]
    #[case("sent:R,Q", false, Condition::Field { name: "QSL_SENT".into(), values: strings(&["R", "Q"]) })]
    #[case("rcvd:y,v", false, Condition::Field { name: "QSL_RCVD".into(), values: strings(&["Y", "V"]) })]
    #[case("dxcc:339", false, Condition::Field { name: "DXCC".into(), values: strings(&["339"]) })]
    #[case("adif.tx_pwr:10", false, Condition::Field { name: "TX_PWR".into(), values: strings(&["10"]) })]
    #[case("station:\"Mt Takao\"", false, Condition::Station(strings(&["mt takao"])))]
    #[case("station:\"a,b\",c", false, Condition::Station(strings(&["a,b", "c"])))]
    #[case("park:jp-0012", false, Condition::Park(strings(&["JP-0012"])))]
    #[case("has:freq", false, Condition::Present("FREQ".into()))]
    #[case("no:park", true, Condition::Present("MY_POTA_REF".into()))]
    #[case("Yokohama", false, Condition::Word("yokohama".into()))]
    #[case("\"two words\"", false, Condition::Word("two words".into()))]
    #[case("-\"two words\"", true, Condition::Word("two words".into()))]
    #[case("BAND:40m", false, Condition::Field { name: "BAND".into(), values: strings(&["40M"]) })]
    fn parses_terms(#[case] query: &str, #[case] negated: bool, #[case] condition: Condition) {
        assert_eq!(Query::parse(query).unwrap().terms(), [Term { negated, condition }]);
    }

    #[test]
    fn reads_no_as_a_negated_presence() {
        let present = |negated| Term {
            negated,
            condition: Condition::Present("FREQ".into()),
        };
        assert_eq!(Query::parse("no:freq").unwrap().terms(), [present(true)]);
        assert_eq!(Query::parse("-no:freq").unwrap().terms(), [present(false)]);
        assert_eq!(tokens("no:freq")[0].key.as_deref(), Some("no"));
    }

    #[rstest]
    #[case("date:2026-10-01..2026-10-19", day("2026-10-01"), day("2026-10-20"))]
    #[case("date:2026-10-01..", day("2026-10-01"), None)]
    #[case("date:..2026-10-19", None, day("2026-10-20"))]
    #[case("date:>=2026-10-18", day("2026-10-18"), None)]
    #[case("date:>2026-10-18", day("2026-10-19"), None)]
    #[case("date:<=2026-10-18", None, day("2026-10-19"))]
    #[case("date:<2026-10-18", None, day("2026-10-18"))]
    #[case("date:2026-10-18", day("2026-10-18"), day("2026-10-19"))]
    fn parses_dates(#[case] query: &str, #[case] from: Option<Day>, #[case] until: Option<Day>) {
        assert_eq!(
            Query::parse(query).unwrap().terms()[0].condition,
            Condition::Date { from, until }
        );
    }

    #[rstest]
    #[case("freq:7", ErrorKind::UnknownField("freq".into()), 0, 0, 6)]
    #[case("band:40m  call:", ErrorKind::MissingValue, 1, 10, 15)]
    #[case("call:,", ErrorKind::MissingValue, 0, 0, 6)]
    #[case("date:2026-13-01", ErrorKind::BadDate("2026-13-01".into()), 0, 0, 15)]
    #[case("date:..", ErrorKind::BadDate("..".into()), 0, 0, 7)]
    #[case("date:2026-10-01,2026-10-02", ErrorKind::BadDate("2026-10-01,2026-10-02".into()), 0, 0, 26)]
    #[case("adif.1X:1", ErrorKind::BadFieldName("1X".into()), 0, 0, 9)]
    #[case("has:a-b", ErrorKind::BadFieldName("a-b".into()), 0, 0, 7)]
    #[case("名前 \"open", ErrorKind::UnclosedQuote, 1, 3, 8)]
    #[case("\"\"", ErrorKind::Empty, 0, 0, 2)]
    fn reports_bad_terms(
        #[case] query: &str,
        #[case] kind: ErrorKind,
        #[case] index: usize,
        #[case] start: usize,
        #[case] end: usize,
    ) {
        let error = Query::parse(query).unwrap_err();
        assert_eq!(
            (error.kind, error.index, error.start, error.end),
            (kind, index, start, end)
        );
    }

    #[test]
    fn caps_the_number_of_values() {
        let values = vec!["40m"; MAX_VALUES + 1].join(",");
        let error = Query::parse(&format!("band:{values}")).unwrap_err();
        assert_eq!(error.kind, ErrorKind::TooManyValues(MAX_VALUES + 1));
    }

    #[test]
    fn keeps_every_token_with_its_span() {
        let tokens = tokens("  call:JA1* \"a b\" -adif.TX_PWR:10,5 bogus:1");
        let summary: Vec<_> = tokens
            .iter()
            .map(|t| {
                (
                    t.start,
                    t.end,
                    t.key.as_deref(),
                    t.field.as_deref(),
                    t.values.clone(),
                    t.term.is_ok(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (2, 11, Some("call"), None, strings(&["JA1*"]), true),
                (12, 17, None, None, strings(&["a b"]), true),
                (18, 35, Some("adif"), Some("TX_PWR"), strings(&["10", "5"]), true),
                (36, 43, Some("bogus"), None, strings(&["1"]), false),
            ]
        );
        assert!(tokens[2].negated);
    }

    #[test]
    fn parses_nothing_from_blanks() {
        assert!(Query::parse(" \t ").unwrap().is_empty());
    }
}
