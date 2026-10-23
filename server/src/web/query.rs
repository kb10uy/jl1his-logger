use chrono::DateTime;
use serde::{Deserialize, Serialize};

use crate::{store::qso::TimeRange, web::error::ApiError};

#[derive(Debug, Deserialize)]
pub struct RangeQuery {
    pub from: Option<String>,
    pub until: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RewriteRequest {
    pub from: Option<String>,
    pub until: Option<String>,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct Count {
    pub count: usize,
}

pub fn text(query: Option<&str>) -> Result<jl1his_query::Query, ApiError> {
    jl1his_query::Query::parse(query.unwrap_or_default()).map_err(|e| {
        ApiError::bad_request(e.to_string())
            .with("code", e.kind.code().into())
            .with("term", e.text.into())
            .with("index", e.index.into())
            .with("start", e.start.into())
            .with("end", e.end.into())
    })
}

pub fn time(name: &str, value: &str) -> Result<i64, ApiError> {
    DateTime::parse_from_rfc3339(value)
        .map(|t| t.timestamp())
        .map_err(|_| ApiError::bad_request(format!("{name} {value:?} is not an RFC 3339 time")))
}

pub fn range(from: Option<&str>, until: Option<&str>) -> Result<TimeRange, ApiError> {
    Ok(TimeRange {
        from: from.map(|t| time("from", t)).transpose()?,
        until: until.map(|t| time("until", t)).transpose()?,
    })
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(Some("2026-10-22T01:00:00Z"), None, Ok((Some(1_792_630_800), None)))]
    #[case(None, Some("2026-10-22T10:00:00+09:00"), Ok((None, Some(1_792_630_800))))]
    #[case(None, None, Ok((None, None)))]
    #[case(Some("20261022"), None, Err(()))]
    #[case(None, Some("2026-10-22"), Err(()))]
    fn reads_rfc_3339_ranges(
        #[case] from: Option<&str>,
        #[case] until: Option<&str>,
        #[case] expected: Result<(Option<i64>, Option<i64>), ()>,
    ) {
        let read = range(from, until).map(|r| (r.from, r.until)).map_err(|_| ());
        assert_eq!(read, expected);
    }
}
