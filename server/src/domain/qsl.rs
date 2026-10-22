use crate::domain::qso::{QsoError, parse_date};

pub const FIELDS: [&str; 14] = [
    "QSL_SENT",
    "QSL_SENT_VIA",
    "QSLSDATE",
    "QSL_RCVD",
    "QSL_RCVD_VIA",
    "QSLRDATE",
    "EQSL_QSL_SENT",
    "EQSL_QSLSDATE",
    "EQSL_QSL_RCVD",
    "EQSL_QSLRDATE",
    "LOTW_QSL_SENT",
    "LOTW_QSLSDATE",
    "LOTW_QSL_RCVD",
    "LOTW_QSLRDATE",
];

const SENT: &[&str] = &["Y", "N", "R", "Q", "I"];
const RCVD: &[&str] = &["Y", "N", "R", "I", "V"];
const VIA: &[&str] = &["B", "D", "E", "M"];

pub fn field(name: &str) -> Option<&'static str> {
    FIELDS
        .iter()
        .copied()
        .find(|field| field.eq_ignore_ascii_case(name.trim()))
}

pub fn check(field: &'static str, value: &str) -> Result<(), QsoError> {
    let valid = match field {
        _ if value.is_empty() => true,
        _ if field.ends_with("DATE") => parse_date(value).is_some(),
        _ if field.ends_with("_VIA") => VIA.iter().any(|v| v.eq_ignore_ascii_case(value)),
        _ if field.ends_with("_SENT") => SENT.iter().any(|v| v.eq_ignore_ascii_case(value)),
        _ => RCVD.iter().any(|v| v.eq_ignore_ascii_case(value)),
    };
    match valid {
        true => Ok(()),
        false => Err(QsoError::Invalid {
            name: field,
            value: value.to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("qsl_sent", Some("QSL_SENT"))]
    #[case(" LoTW_QSLRDATE ", Some("LOTW_QSLRDATE"))]
    #[case("CALL", None)]
    fn finds_qsl_fields_ignoring_case(#[case] name: &str, #[case] expected: Option<&str>) {
        assert_eq!(field(name), expected);
    }

    #[rstest]
    #[case("QSL_SENT", "q", true)]
    #[case("QSL_SENT", "V", false)]
    #[case("QSL_RCVD", "V", true)]
    #[case("EQSL_QSL_RCVD", "Q", false)]
    #[case("QSL_SENT_VIA", "b", true)]
    #[case("QSL_RCVD_VIA", "W", false)]
    #[case("QSLSDATE", "20261022", true)]
    #[case("LOTW_QSLRDATE", "20261322", false)]
    #[case("EQSL_QSLSDATE", "2026-10-22", false)]
    #[case("LOTW_QSL_SENT", "", true)]
    fn checks_enumerations_and_dates(#[case] name: &'static str, #[case] value: &str, #[case] valid: bool) {
        assert_eq!(check(name, value).is_ok(), valid);
    }
}
