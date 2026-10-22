use chrono::NaiveDate;
use serde_json::{Map, Value};

use crate::domain::{
    band::format_frequency_mhz,
    qso::{Qso, QsoError},
};

pub const STATION_PROFILE_ID: &str = "station_profile_id";

pub fn qso_from_body(body: &Map<String, Value>) -> Result<Qso, QsoError> {
    let mut fields = vec![];
    for (key, value) in body {
        if key.eq_ignore_ascii_case(STATION_PROFILE_ID) {
            continue;
        }
        let name = key.to_ascii_uppercase();
        let value = match value {
            Value::Null => continue,
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => {
                return Err(QsoError::Invalid {
                    name: "field",
                    value: format!("{key}: {value}"),
                });
            }
        };
        let value = match name.as_str() {
            "QSO_DATE" | "QSO_DATE_OFF" => date(&value),
            "FREQ" => frequency("FREQ", &value)?,
            "FREQ_RX" => frequency("FREQ_RX", &value)?,
            _ => value,
        };
        fields.push((name, value));
    }
    Qso::from_fields(fields)
}

fn date(value: &str) -> String {
    match NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d") {
        Ok(date) => date.format("%Y%m%d").to_string(),
        Err(_) => value.to_owned(),
    }
}

fn frequency(name: &'static str, value: &str) -> Result<String, QsoError> {
    value
        .trim()
        .parse::<u64>()
        .map(format_frequency_mhz)
        .map_err(|_| QsoError::Invalid {
            name,
            value: value.to_owned(),
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn body(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn reads_the_body_hislog_and_speech_to_qso_send() {
        let qso = qso_from_body(&body(json!({
            "station_profile_id": 3,
            "call": "JA1XYZ",
            "qso_date": "2026-10-20",
            "time_on": "094127",
            "band": "40m",
            "freq": "7041500",
            "mode": "CW",
            "rst_sent": 599,
            "name": "太郎",
            "submode": null,
        })))
        .unwrap();

        let fields: Vec<(&str, &str)> = qso.iter().collect();
        assert_eq!(
            fields,
            [
                ("CALL", "JA1XYZ"),
                ("QSO_DATE", "20261020"),
                ("TIME_ON", "094127"),
                ("BAND", "40m"),
                ("MODE", "CW"),
                ("FREQ", "7.0415"),
                ("RST_SENT", "599"),
                ("NAME", "太郎"),
            ]
        );
        assert_eq!(
            qso_from_body(&body(json!({ "freq": 432940000u64, "qso_date": "20261003" })))
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            [("QSO_DATE", "20261003"), ("FREQ", "432.94")]
        );
    }

    #[test]
    fn refuses_what_is_not_an_adif_value() {
        for value in [
            json!({ "freq": "7.041" }),
            json!({ "call": ["JA1XYZ"] }),
            json!({ "call": "JA1XYZ", "CALL": "JA1XYZ" }),
            json!({ "my:city": "Minato" }),
        ] {
            assert!(qso_from_body(&body(value.clone())).is_err(), "{value}");
        }
    }
}
