use std::collections::BTreeMap;

use adif_manip::LengthMode;
use serde::{Deserialize, Serialize};

const WSJTX_MAGIC: u32 = 0xadbc_cbda;
const WSJTX_LOGGED_ADIF: u32 = 12;
const LENGTHS: [LengthMode; 3] = [LengthMode::Codepoints, LengthMode::Bytes, LengthMode::Utf16Units];

pub type Fields = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum EntryMessage {
    Set { fields: Fields },
    Clear,
    Log,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Datagram {
    Qsos {
        program: Option<String>,
        records: Vec<Fields>,
    },
    Entry(EntryMessage),
    Subscribe,
    Unsubscribe,
    Ignored,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Body {
    Set { fields: Fields },
    Clear,
    Log,
    Subscribe,
    Unsubscribe,
}

#[derive(Deserialize)]
struct Message {
    v: u32,
    #[serde(flatten)]
    body: Body,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Reply<'a> {
    Form { fields: &'a Fields },
    Bye,
}

#[derive(Serialize)]
struct Envelope<'a> {
    v: u32,
    #[serde(flatten)]
    body: Reply<'a>,
}

pub fn form(fields: &Fields) -> Vec<u8> {
    encode(Reply::Form { fields })
}

pub fn bye() -> Vec<u8> {
    encode(Reply::Bye)
}

fn encode(body: Reply<'_>) -> Vec<u8> {
    serde_json::to_vec(&Envelope { v: 1, body }).expect("a reply always serializes")
}

pub fn read(bytes: &[u8]) -> Datagram {
    if bytes.starts_with(&WSJTX_MAGIC.to_be_bytes()) {
        return wsjtx(bytes).unwrap_or(Datagram::Ignored);
    }
    let text = String::from_utf8_lossy(bytes);
    if text.trim_start().starts_with('{') {
        return match serde_json::from_str::<Message>(&text) {
            Ok(Message { v: 1, body }) => match body {
                Body::Set { fields } => Datagram::Entry(EntryMessage::Set { fields }),
                Body::Clear => Datagram::Entry(EntryMessage::Clear),
                Body::Log => Datagram::Entry(EntryMessage::Log),
                Body::Subscribe => Datagram::Subscribe,
                Body::Unsubscribe => Datagram::Unsubscribe,
            },
            _ => Datagram::Ignored,
        };
    }
    adif(&text, None)
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn u32(&mut self) -> Option<u32> {
        let (head, rest) = self.0.split_first_chunk::<4>()?;
        self.0 = rest;
        Some(u32::from_be_bytes(*head))
    }

    fn utf8(&mut self) -> Option<Option<String>> {
        let length = self.u32()?;
        if length == u32::MAX {
            return Some(None);
        }
        let length = usize::try_from(length).ok()?;
        if self.0.len() < length {
            return None;
        }
        let (head, rest) = self.0.split_at(length);
        self.0 = rest;
        Some(Some(String::from_utf8_lossy(head).into_owned()))
    }
}

fn wsjtx(bytes: &[u8]) -> Option<Datagram> {
    let mut reader = Reader(bytes);
    reader.u32()?;
    reader.u32()?;
    let kind = reader.u32()?;
    let id = reader.utf8()?;
    if kind != WSJTX_LOGGED_ADIF {
        return Some(Datagram::Ignored);
    }
    let text = reader.utf8()??;
    Some(adif(&text, id))
}

fn adif(text: &str, program: Option<String>) -> Datagram {
    let Ok(document) = adif_manip::read_adi_detecting(text, &LENGTHS) else {
        return Datagram::Ignored;
    };
    let records: Vec<Fields> = document
        .records()
        .iter()
        .map(|record| {
            record
                .fields()
                .iter()
                .filter(|(_, field)| !field.value().is_empty())
                .map(|(name, field)| (name.to_ascii_uppercase(), field.value().to_owned()))
                .collect::<Fields>()
        })
        .filter(|fields| !fields.is_empty())
        .collect();
    if records.is_empty() {
        return Datagram::Ignored;
    }
    let program = program
        .filter(|p| !p.is_empty())
        .or_else(|| document.header("PROGRAMID").map(str::to_owned));
    Datagram::Qsos { program, records }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const RECORD: &str = "<call:6>JA1ABC <gridsquare:4>PM95 <mode:3>FT8 <rst_sent:3>-10 <rst_rcvd:3>-12 \
        <qso_date:8>20261027 <time_on:6>120015 <band:3>20m <freq:9>14.075123 \
        <station_callsign:6>JL1HIS <my_gridsquare:6>PM95uq <comment:0> <eor>";

    fn utf8(out: &mut Vec<u8>, value: Option<&str>) {
        match value {
            Some(value) => {
                out.extend((value.len() as u32).to_be_bytes());
                out.extend(value.as_bytes());
            }
            None => out.extend(u32::MAX.to_be_bytes()),
        }
    }

    fn wsjtx_message(kind: u32, id: Option<&str>, rest: &[Option<&str>]) -> Vec<u8> {
        let mut out = vec![];
        out.extend(WSJTX_MAGIC.to_be_bytes());
        out.extend(3u32.to_be_bytes());
        out.extend(kind.to_be_bytes());
        utf8(&mut out, id);
        for value in rest {
            utf8(&mut out, *value);
        }
        out
    }

    fn fields(pairs: &[(&str, &str)]) -> Fields {
        pairs.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect()
    }

    fn logged() -> Fields {
        fields(&[
            ("BAND", "20m"),
            ("CALL", "JA1ABC"),
            ("FREQ", "14.075123"),
            ("GRIDSQUARE", "PM95"),
            ("MODE", "FT8"),
            ("MY_GRIDSQUARE", "PM95uq"),
            ("QSO_DATE", "20261027"),
            ("RST_RCVD", "-12"),
            ("RST_SENT", "-10"),
            ("STATION_CALLSIGN", "JL1HIS"),
            ("TIME_ON", "120015"),
        ])
    }

    #[test]
    fn reads_logged_adif_from_wsjtx() {
        let text = format!("\n<adif_ver:5>3.1.0\n<programid:6>WSJT-X\n<EOH>\n{RECORD}");
        let message = wsjtx_message(WSJTX_LOGGED_ADIF, Some("WSJT-X - IC-705"), &[Some(&text)]);
        assert_eq!(
            read(&message),
            Datagram::Qsos {
                program: Some("WSJT-X - IC-705".to_owned()),
                records: vec![logged()],
            }
        );
    }

    #[rstest]
    #[case::heartbeat(0, &[Some("3"), Some("2.7.0"), Some("abc")])]
    #[case::qso_logged(5, &[Some("anything")])]
    #[case::no_adif(12, &[None])]
    #[case::cut_short(12, &[])]
    fn ignores_the_rest_of_wsjtx(#[case] kind: u32, #[case] rest: &[Option<&str>]) {
        assert_eq!(read(&wsjtx_message(kind, Some("WSJT-X"), rest)), Datagram::Ignored);
    }

    #[test]
    fn reads_adif_text_without_a_header() {
        let text = format!("{RECORD}\n{}", RECORD.replace("JA1ABC", "JA2XYZ"));
        let Datagram::Qsos { program, records } = read(text.as_bytes()) else {
            panic!("not read as QSOs");
        };
        assert_eq!(program, None);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0], logged());
        assert_eq!(records[1]["CALL"], "JA2XYZ");
    }

    #[test]
    fn names_the_program_from_the_adif_header() {
        let text = format!("JTDX log\n<programid:4>JTDX <eoh>{RECORD}");
        let Datagram::Qsos { program, .. } = read(text.as_bytes()) else {
            panic!("not read as QSOs");
        };
        assert_eq!(program.as_deref(), Some("JTDX"));
    }

    #[rstest]
    #[case(r#"{"v": 1, "type": "set", "fields": {"CALL": "JA1ABC", "SRX_STRING": "13M"}}"#,
        EntryMessage::Set { fields: fields(&[("CALL", "JA1ABC"), ("SRX_STRING", "13M")]) })]
    #[case(r#" {"type": "clear", "v": 1}"#, EntryMessage::Clear)]
    #[case(r#"{"v": 1, "type": "log"}"#, EntryMessage::Log)]
    fn reads_entry_messages(#[case] json: &str, #[case] expected: EntryMessage) {
        assert_eq!(read(json.as_bytes()), Datagram::Entry(expected));
    }

    #[rstest]
    #[case(r#"{"v": 1, "type": "subscribe"}"#, Datagram::Subscribe)]
    #[case(r#"{"type": "unsubscribe", "v": 1}"#, Datagram::Unsubscribe)]
    fn reads_subscriptions(#[case] json: &str, #[case] expected: Datagram) {
        assert_eq!(read(json.as_bytes()), expected);
    }

    #[test]
    fn writes_the_form_and_bye() {
        let form = form(&fields(&[("CALL", "JA1ABC"), ("NAME", "")]));
        assert_eq!(
            String::from_utf8(form.clone()).unwrap(),
            r#"{"v":1,"type":"form","fields":{"CALL":"JA1ABC","NAME":""}}"#
        );
        assert_eq!(String::from_utf8(bye()).unwrap(), r#"{"v":1,"type":"bye"}"#);
        assert_eq!(read(&form), Datagram::Ignored);
        assert_eq!(read(&bye()), Datagram::Ignored);
    }

    #[rstest]
    #[case::other_version(r#"{"v": 2, "type": "clear"}"#)]
    #[case::subscribe_of_another_version(r#"{"v": 2, "type": "subscribe"}"#)]
    #[case::no_version(r#"{"type": "clear"}"#)]
    #[case::unknown_type(r#"{"v": 1, "type": "tune"}"#)]
    #[case::number_value(r#"{"v": 1, "type": "set", "fields": {"RST_RCVD": 599}}"#)]
    #[case::broken(r#"{"v": 1, "#)]
    #[case::not_adif("hello")]
    #[case::empty("")]
    fn ignores_what_it_cannot_read(#[case] text: &str) {
        assert_eq!(read(text.as_bytes()), Datagram::Ignored);
    }
}
