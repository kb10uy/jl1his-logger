use crate::domain::qso::{Qso, QsoError};

pub fn parks(qso: &Qso) -> Vec<String> {
    let split = |refs: &str| -> Vec<String> {
        refs.split(',')
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .map(str::to_owned)
            .collect()
    };
    match qso.get("MY_POTA_REF").map(split).filter(|p| !p.is_empty()) {
        Some(parks) => parks,
        None if qso.get("MY_SIG").is_some_and(|s| s.trim().eq_ignore_ascii_case("POTA")) => {
            qso.get("MY_SIG_INFO").map(split).unwrap_or_default()
        }
        None => vec![],
    }
}

pub fn project(qso: &Qso) -> Result<Vec<Qso>, QsoError> {
    parks(qso)
        .into_iter()
        .map(|park| {
            let mut record = qso.clone();
            record.set("MY_POTA_REF", Some(park.clone()))?;
            record.set("MY_SIG", Some("POTA".to_owned()))?;
            record.set("MY_SIG_INFO", Some(park))?;
            Ok(record)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::domain::qso::tests::sample;

    fn with(fields: &[(&str, &str)]) -> Qso {
        let mut qso = sample();
        for (name, value) in fields {
            qso.set(name, Some((*value).to_owned())).unwrap();
        }
        qso
    }

    #[rstest]
    #[case::two_parks(&[("MY_POTA_REF", "JP-0001, JP-0002")], &["JP-0001", "JP-0002"])]
    #[case::with_location(&[("MY_POTA_REF", "JP-0001@JP-13")], &["JP-0001@JP-13"])]
    #[case::sig_only(&[("MY_SIG", "pota"), ("MY_SIG_INFO", "JP-0003")], &["JP-0003"])]
    #[case::ref_wins(&[("MY_POTA_REF", "JP-0001"), ("MY_SIG", "POTA"), ("MY_SIG_INFO", "JP-0003")], &["JP-0001"])]
    #[case::other_sig(&[("MY_SIG", "WWFF"), ("MY_SIG_INFO", "JAFF-0001")], &[])]
    #[case::none(&[], &[])]
    fn reads_the_parks_of_a_qso(#[case] fields: &[(&str, &str)], #[case] expected: &[&str]) {
        assert_eq!(parks(&with(fields)), expected);
    }

    #[test]
    fn projects_one_record_per_park() {
        let qso = with(&[("MY_POTA_REF", "JP-0001,JP-0002"), ("POTA_REF", "JP-0100")]);
        let records = project(&qso).unwrap();
        let refs: Vec<_> = records
            .iter()
            .map(|r| {
                (
                    r.get("MY_POTA_REF").unwrap(),
                    r.get("MY_SIG").unwrap(),
                    r.get("MY_SIG_INFO").unwrap(),
                    r.get("POTA_REF").unwrap(),
                )
            })
            .collect();
        assert_eq!(
            refs,
            [
                ("JP-0001", "POTA", "JP-0001", "JP-0100"),
                ("JP-0002", "POTA", "JP-0002", "JP-0100")
            ]
        );
        assert_eq!(records[0].get("CALL"), qso.get("CALL"));
        assert!(project(&sample()).unwrap().is_empty());
    }
}
