use std::collections::HashMap;

use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::{Query, Record, tokens};

#[derive(Serialize)]
struct TokenJson<'a> {
    start: usize,
    end: usize,
    text: &'a str,
    negated: bool,
    key: Option<&'a str>,
    field: Option<&'a str>,
    values: &'a [String],
    error: Option<ErrorJson>,
}

#[derive(Serialize)]
struct ErrorJson {
    code: &'static str,
    message: String,
}

#[wasm_bindgen]
pub fn analyze(query: &str) -> String {
    let tokens = tokens(query);
    let json: Vec<TokenJson> = tokens
        .iter()
        .map(|t| TokenJson {
            start: t.start,
            end: t.end,
            text: &t.text,
            negated: t.negated,
            key: t.key.as_deref(),
            field: t.field.as_deref(),
            values: &t.values,
            error: t.term.as_ref().err().map(|kind| ErrorJson {
                code: kind.code(),
                message: kind.to_string(),
            }),
        })
        .collect();
    serde_json::to_string(&json).expect("tokens serialize")
}

struct Row {
    fields: HashMap<String, String>,
    station: Option<String>,
    instrument: Option<String>,
}

impl Record for Row {
    fn field(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(String::as_str)
    }

    fn station_name(&self) -> Option<&str> {
        self.station.as_deref()
    }

    fn instrument_name(&self) -> Option<&str> {
        self.instrument.as_deref()
    }
}

#[wasm_bindgen(js_name = callBase)]
pub fn call_base(call: &str) -> String {
    crate::call_base(call)
}

#[wasm_bindgen]
pub struct Matcher(Query);

#[wasm_bindgen]
impl Matcher {
    #[wasm_bindgen(constructor)]
    pub fn new(query: &str) -> Result<Matcher, JsError> {
        Query::parse(query)
            .map(Matcher)
            .map_err(|e| JsError::new(&e.to_string()))
    }

    pub fn matches(
        &self,
        fields: &js_sys::Object,
        station_name: Option<String>,
        instrument_name: Option<String>,
    ) -> bool {
        let fields = js_sys::Object::entries(fields)
            .iter()
            .filter_map(|entry| {
                let entry = js_sys::Array::from(&entry);
                Some((entry.get(0).as_string()?, entry.get(1).as_string()?))
            })
            .collect();
        self.0.matches(&Row {
            fields,
            station: station_name,
            instrument: instrument_name,
        })
    }
}
