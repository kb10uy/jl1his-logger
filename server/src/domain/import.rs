use std::collections::{BTreeMap, HashMap};

use sha2::{Digest, Sha256};

use crate::domain::qso::{Qso, QsoError, QsoId};

const MAX_UNIX_MILLIS: u64 = (1 << 48) - 1;

#[derive(Debug, Clone)]
pub struct RecordIds {
    logbook_id: String,
    station_id: Option<String>,
    seen: HashMap<[u8; 32], u64>,
}

impl RecordIds {
    pub fn new(logbook_id: &str) -> RecordIds {
        RecordIds {
            logbook_id: logbook_id.to_owned(),
            station_id: None,
            seen: HashMap::new(),
        }
    }

    pub fn for_station(logbook_id: &str, station_id: &str) -> RecordIds {
        RecordIds {
            station_id: Some(station_id.to_owned()),
            ..Self::new(logbook_id)
        }
    }

    pub fn next(&mut self, record: &Qso) -> Result<QsoId, QsoError> {
        let qso_time = record.qso_time()?;
        let millis = u64::try_from(qso_time)
            .ok()
            .and_then(|seconds| seconds.checked_mul(1000))
            .filter(|millis| *millis <= MAX_UNIX_MILLIS)
            .ok_or_else(|| QsoError::invalid("QSO_DATE", record.get("QSO_DATE").unwrap_or_default()))?;

        let content: [u8; 32] = Sha256::digest(canonical(record)).into();
        let occurrence = self.seen.entry(content).or_default();
        let mut input = vec![];
        put(&mut input, self.logbook_id.as_bytes());
        if let Some(station_id) = &self.station_id {
            put(&mut input, station_id.as_bytes());
        }
        input.extend(content);
        input.extend(occurrence.to_le_bytes());
        *occurrence += 1;

        let digest = Sha256::digest(&input);
        let rest: [u8; 10] = digest[..10].try_into().expect("a SHA-256 digest has ten bytes");
        Ok(QsoId::with_time(millis, &rest))
    }
}

fn canonical(record: &Qso) -> Vec<u8> {
    let sorted: BTreeMap<&str, &str> = record.iter().collect();
    let mut out = vec![];
    for (name, value) in sorted {
        put(&mut out, name.as_bytes());
        put(&mut out, value.as_bytes());
    }
    out
}

fn put(out: &mut Vec<u8>, bytes: &[u8]) {
    let length = u64::try_from(bytes.len()).expect("a field fits in u64");
    out.extend(length.to_le_bytes());
    out.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::qso::tests::{sample, with};

    fn time_of(id: QsoId) -> String {
        id.to_string()[..13].to_owned()
    }

    #[test]
    fn derives_the_same_ids_on_a_rerun() {
        let records = [sample(), sample()];
        let first: Vec<QsoId> = {
            let mut ids = RecordIds::new("log");
            records.iter().map(|r| ids.next(r).unwrap()).collect()
        };
        let again: Vec<QsoId> = {
            let mut ids = RecordIds::new("log");
            records.iter().map(|r| ids.next(r).unwrap()).collect()
        };
        assert_eq!(first, again);
        assert_ne!(first[0], first[1]);
    }

    #[test]
    fn takes_the_qso_time_as_the_timestamp() {
        let id = RecordIds::new("log").next(&sample()).unwrap();
        assert_eq!(time_of(id), time_of(QsoId::with_time(1_792_641_906_000, &[0; 10])));
    }

    #[test]
    fn ignores_field_order_and_name_case() {
        let reordered = Qso::from_fields(sample().iter().collect::<Vec<_>>().into_iter().rev()).unwrap();
        let lowercase = Qso::from_fields(
            sample()
                .iter()
                .map(|(name, value)| (name.to_ascii_lowercase(), value.to_owned())),
        )
        .unwrap();
        let id = RecordIds::new("log").next(&sample()).unwrap();
        assert_eq!(RecordIds::new("log").next(&reordered).unwrap(), id);
        assert_eq!(RecordIds::new("log").next(&lowercase).unwrap(), id);
    }

    #[test]
    fn depends_on_the_logbook_and_the_values() {
        let id = RecordIds::new("log").next(&sample()).unwrap();
        assert_ne!(RecordIds::new("other").next(&sample()).unwrap(), id);

        let edited = with(&[("RST_SENT", "579")]);
        assert_ne!(RecordIds::new("log").next(&edited).unwrap(), id);

        let mut ids = RecordIds::new("log");
        ids.next(&edited).unwrap();
        assert_eq!(ids.next(&sample()).unwrap(), id);
    }

    #[test]
    fn scopes_station_records_without_changing_import_ids() {
        let record = sample();
        let id = RecordIds::for_station("log", "home").next(&record).unwrap();
        assert_eq!(RecordIds::for_station("log", "home").next(&record).unwrap(), id);
        assert_ne!(RecordIds::for_station("log", "portable").next(&record).unwrap(), id);
        assert_ne!(RecordIds::for_station("other", "home").next(&record).unwrap(), id);
        assert_ne!(RecordIds::new("log").next(&record).unwrap(), id);
    }

    #[test]
    fn refuses_a_qso_before_the_epoch() {
        let old = with(&[("QSO_DATE", "19691231")]);
        assert_eq!(
            RecordIds::new("log").next(&old),
            Err(QsoError::invalid("QSO_DATE", "19691231"))
        );
    }
}
