use crate::domain::{
    qsl,
    qso::{Qso, QsoError},
};

const REPORTED: [&str; 7] = ["DXCC", "CQZ", "ITUZ", "STATE", "CNTY", "GRIDSQUARE", "IOTA"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Lotw,
    Eqsl,
    Card,
}

pub type Changes = Vec<(&'static str, String)>;

impl Source {
    fn received_field(self) -> &'static str {
        match self {
            Source::Lotw => "LOTW_QSL_RCVD",
            Source::Eqsl => "EQSL_QSL_RCVD",
            Source::Card => "QSL_RCVD",
        }
    }

    fn date_field(self) -> &'static str {
        match self {
            Source::Lotw => "LOTW_QSLRDATE",
            Source::Eqsl => "EQSL_QSLRDATE",
            Source::Card => "QSLRDATE",
        }
    }

    pub fn confirmation(self, record: &Qso, today: &str) -> Result<Option<Changes>, QsoError> {
        let first = |names: &[&str]| names.iter().find_map(|name| record.get(name)).map(str::trim);
        let received = match self {
            Source::Lotw => first(&["LOTW_QSL_RCVD", "QSL_RCVD"]).filter(|v| is(v, &["Y"])),
            Source::Eqsl => match first(&["EQSL_QSL_RCVD", "QSL_RCVD"]) {
                None => Some("Y"),
                Some(v) => Some(v).filter(|v| is(v, &["Y"])),
            },
            Source::Card => first(&["QSL_RCVD"]).filter(|v| is(v, &["Y", "V"])),
        };
        let Some(received) = received else {
            return Ok(None);
        };
        let date = first(&[self.date_field(), "QSLRDATE"]).unwrap_or(today);

        let mut changes = vec![
            (self.received_field(), received.to_ascii_uppercase()),
            (self.date_field(), date.to_owned()),
        ];
        if self == Source::Card
            && let Some(via) = record.get("QSL_RCVD_VIA").map(str::trim)
        {
            changes.push(("QSL_RCVD_VIA", via.to_ascii_uppercase()));
        }
        for (field, value) in &changes {
            qsl::check(field, value)?;
        }
        Ok(Some(changes))
    }

    pub fn received(self, qso: &Qso) -> bool {
        qso.get(self.received_field()).is_some_and(|v| is(v, &["Y", "V"]))
    }
}

fn is(value: &str, accepted: &[&str]) -> bool {
    accepted.iter().any(|a| value.trim().eq_ignore_ascii_case(a))
}

pub fn differences(record: &Qso, candidate: &Qso) -> Vec<&'static str> {
    REPORTED
        .iter()
        .copied()
        .filter(|name| match record.get(name).map(str::trim).filter(|v| !v.is_empty()) {
            None => false,
            Some(value) => !candidate
                .get(name)
                .is_some_and(|c| c.trim().eq_ignore_ascii_case(value)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::domain::qso::tests::with;

    fn record(fields: &[(&str, &str)]) -> Qso {
        Qso::from_fields(fields.iter().copied()).unwrap()
    }

    #[rstest]
    #[case::lotw_report(
        Source::Lotw,
        &[("QSL_RCVD", "Y"), ("QSLRDATE", "20261101")],
        Some(vec![("LOTW_QSL_RCVD", "Y"), ("LOTW_QSLRDATE", "20261101")])
    )]
    #[case::lotw_own_fields_first(
        Source::Lotw,
        &[("LOTW_QSL_RCVD", "y"), ("LOTW_QSLRDATE", "20261102"), ("QSL_RCVD", "N"), ("QSLRDATE", "20261101")],
        Some(vec![("LOTW_QSL_RCVD", "Y"), ("LOTW_QSLRDATE", "20261102")])
    )]
    #[case::lotw_unconfirmed(Source::Lotw, &[("QSL_RCVD", "N")], None)]
    #[case::lotw_without_a_field(Source::Lotw, &[], None)]
    #[case::eqsl_inbox(Source::Eqsl, &[], Some(vec![("EQSL_QSL_RCVD", "Y"), ("EQSL_QSLRDATE", "20261122")]))]
    #[case::eqsl_dated(
        Source::Eqsl,
        &[("EQSL_QSL_RCVD", "Y"), ("EQSL_QSLRDATE", "20261103")],
        Some(vec![("EQSL_QSL_RCVD", "Y"), ("EQSL_QSLRDATE", "20261103")])
    )]
    #[case::eqsl_requested(Source::Eqsl, &[("EQSL_QSL_RCVD", "R")], None)]
    #[case::card_verified(
        Source::Card,
        &[("QSL_RCVD", "v"), ("QSL_RCVD_VIA", "b")],
        Some(vec![("QSL_RCVD", "V"), ("QSLRDATE", "20261122"), ("QSL_RCVD_VIA", "B")])
    )]
    #[case::card_absent(Source::Card, &[], None)]
    fn reads_the_confirmation_of_each_source(
        #[case] source: Source,
        #[case] fields: &[(&str, &str)],
        #[case] expected: Option<Vec<(&str, &str)>>,
    ) {
        let expected = expected.map(|c| c.into_iter().map(|(f, v)| (f, v.to_owned())).collect::<Vec<_>>());
        assert_eq!(source.confirmation(&with(fields), "20261122").unwrap(), expected);
    }

    #[test]
    fn refuses_a_malformed_confirmation_date() {
        let qso = record(&[("QSL_RCVD", "Y"), ("QSLRDATE", "2026-11-01")]);
        assert!(Source::Lotw.confirmation(&qso, "20261122").is_err());
    }

    #[rstest]
    #[case(Source::Lotw, &[("LOTW_QSL_RCVD", "Y")], true)]
    #[case(Source::Lotw, &[("QSL_RCVD", "Y")], false)]
    #[case(Source::Card, &[("QSL_RCVD", "v")], true)]
    #[case(Source::Card, &[("QSL_RCVD", " y ")], true)]
    #[case(Source::Card, &[("QSL_RCVD", "R")], false)]
    #[case(Source::Eqsl, &[], false)]
    fn knows_when_a_qso_is_already_received(
        #[case] source: Source,
        #[case] fields: &[(&str, &str)],
        #[case] received: bool,
    ) {
        assert_eq!(source.received(&record(fields)), received);
    }

    #[test]
    fn lists_the_reported_fields_that_differ() {
        let stored = with(&[("GRIDSQUARE", "PM95"), ("DXCC", "339")]);
        let report = record(&[("GRIDSQUARE", "pm95"), ("DXCC", "339"), ("CQZ", "25"), ("STATE", "")]);
        assert_eq!(differences(&report, &stored), ["CQZ"]);
    }
}
