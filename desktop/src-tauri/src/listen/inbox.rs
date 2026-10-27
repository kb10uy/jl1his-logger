use std::time::{Duration, Instant};

use serde::Serialize;

use crate::listen::datagram::Fields;

const SAME_QSO: Duration = Duration::from_secs(120);
const MAX_HELD: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Incoming {
    pub id: u64,
    pub program: Option<String>,
    pub fields: Fields,
}

#[derive(Default)]
pub struct Inbox {
    next: u64,
    pending: Vec<Incoming>,
    seen: Vec<(String, Instant)>,
}

impl Inbox {
    pub fn push(&mut self, program: Option<String>, fields: Fields, now: Instant) -> bool {
        if self.pending.len() >= MAX_HELD {
            return false;
        }
        self.seen.retain(|(_, at)| now.duration_since(*at) < SAME_QSO);
        if let Some(key) = same_qso(&fields) {
            if self.seen.iter().any(|(seen, _)| *seen == key) {
                return false;
            }
            if self.seen.len() >= MAX_HELD {
                self.seen.remove(0);
            }
            self.seen.push((key, now));
        }
        self.next += 1;
        self.pending.push(Incoming {
            id: self.next,
            program,
            fields,
        });
        true
    }

    pub fn ack(&mut self, ids: &[u64]) {
        self.pending.retain(|incoming| !ids.contains(&incoming.id));
    }

    pub fn pending(&self) -> &[Incoming] {
        &self.pending
    }
}

fn same_qso(fields: &Fields) -> Option<String> {
    let get = |name| fields.get(name).map(|v| v.trim()).unwrap_or("");
    let call = get("CALL");
    if call.is_empty() {
        return None;
    }
    Some(format!(
        "{}|{}|{}|{}|{}",
        call.to_ascii_uppercase(),
        get("QSO_DATE"),
        get("TIME_ON"),
        get("BAND").to_ascii_lowercase(),
        get("MODE").to_ascii_uppercase()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qso(call: &str, time_on: &str) -> Fields {
        [
            ("CALL", call),
            ("QSO_DATE", "20261027"),
            ("TIME_ON", time_on),
            ("BAND", "20m"),
            ("MODE", "FT8"),
        ]
        .iter()
        .map(|(n, v)| (n.to_string(), v.to_string()))
        .collect()
    }

    fn calls(inbox: &Inbox) -> Vec<(u64, &str)> {
        inbox
            .pending
            .iter()
            .map(|i| (i.id, i.fields["CALL"].as_str()))
            .collect()
    }

    #[test]
    fn holds_what_it_received_until_acknowledged() {
        let mut inbox = Inbox::default();
        let now = Instant::now();
        assert!(inbox.push(Some("WSJT-X".to_owned()), qso("JA1ABC", "120015"), now));
        assert!(inbox.push(None, qso("JA2XYZ", "120115"), now));
        assert_eq!(calls(&inbox), [(1, "JA1ABC"), (2, "JA2XYZ")]);
        inbox.ack(&[1]);
        assert_eq!(calls(&inbox), [(2, "JA2XYZ")]);
    }

    #[test]
    fn drops_the_same_qso_arriving_again_for_two_minutes() {
        let mut inbox = Inbox::default();
        let now = Instant::now();
        assert!(inbox.push(None, qso("JA1ABC", "120015"), now));
        inbox.ack(&[1]);
        let mut again = qso("ja1abc", "120015");
        again.insert("BAND".to_owned(), "20M".to_owned());
        assert!(!inbox.push(None, again, now + Duration::from_secs(1)));
        assert!(inbox.push(None, qso("JA1ABC", "121015"), now + Duration::from_secs(2)));
        assert!(inbox.push(None, qso("JA1ABC", "120015"), now + SAME_QSO));
        assert_eq!(calls(&inbox), [(2, "JA1ABC"), (3, "JA1ABC")]);
    }

    #[test]
    fn keeps_every_record_without_a_call() {
        let mut inbox = Inbox::default();
        let now = Instant::now();
        let mut fields = qso("JA1ABC", "120015");
        fields.remove("CALL");
        assert!(inbox.push(None, fields.clone(), now));
        assert!(inbox.push(None, fields, now));
        assert_eq!(inbox.pending.len(), 2);
    }

    #[test]
    fn drops_what_arrives_while_full() {
        let mut inbox = Inbox::default();
        let now = Instant::now();
        for n in 0..MAX_HELD {
            assert!(inbox.push(None, qso(&format!("JA{n}ABC"), "120015"), now));
        }
        assert!(!inbox.push(None, qso("JL1HIS", "120015"), now));
        assert_eq!(inbox.pending.len(), MAX_HELD);
        inbox.ack(&[1]);
        assert!(inbox.push(None, qso("JL1HIS", "120015"), now));
        assert_eq!(inbox.seen.len(), MAX_HELD);
        assert_eq!(inbox.seen[0].0, "JA1ABC|20261027|120015|20m|FT8");
    }
}
