use std::{mem, sync::Mutex, time::Duration};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, State, async_runtime::JoinHandle};

use crate::{
    api::{Api, ApiError},
    session::{CommandError, Connection},
};

const RETRY_MIN: Duration = Duration::from_secs(2);
const RETRY_MAX: Duration = Duration::from_secs(60);
pub const STREAM_EVENT: &str = "logbook-stream";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SseEvent {
    pub event: String,
    pub id: Option<String>,
    pub data: String,
}

#[derive(Debug, Default)]
pub struct SseParser {
    buffer: Vec<u8>,
    current: SseEvent,
    has_data: bool,
}

impl SseParser {
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        self.buffer.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some(end) = self.buffer.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                let event = mem::take(&mut self.current);
                if mem::take(&mut self.has_data) {
                    events.push(event);
                }
                continue;
            }
            if line.starts_with(':') {
                continue;
            }
            let (field, value) = match line.split_once(':') {
                Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
                None => (line, ""),
            };
            match field {
                "event" => self.current.event = value.to_owned(),
                "id" => self.current.id = Some(value.to_owned()),
                "data" => {
                    if self.has_data {
                        self.current.data.push('\n');
                    }
                    self.current.data.push_str(value);
                    self.has_data = true;
                }
                _ => {}
            }
        }
        events
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum StreamEvent {
    Hello { logbook_id: String, hello: Value },
    Qso { logbook_id: String, data: Value },
    Status { logbook_id: String, live: bool },
}

#[derive(Default)]
pub struct Watchers(Mutex<Vec<JoinHandle<()>>>);

impl Watchers {
    pub fn stop(&self) {
        for handle in self.0.lock().expect("the watchers lock is never poisoned").drain(..) {
            handle.abort();
        }
    }

    fn replace(&self, handles: Vec<JoinHandle<()>>) {
        let old = mem::replace(
            &mut *self.0.lock().expect("the watchers lock is never poisoned"),
            handles,
        );
        for handle in old {
            handle.abort();
        }
    }
}

#[tauri::command]
pub fn watch(
    app: AppHandle,
    connection: State<'_, Connection>,
    watchers: State<'_, Watchers>,
    logbook_ids: Vec<String>,
) -> Result<(), CommandError> {
    let api = connection.get().ok_or(CommandError::Unauthorized)?;
    let handles = logbook_ids
        .into_iter()
        .map(|id| tauri::async_runtime::spawn(follow(app.clone(), api.clone(), id)))
        .collect();
    watchers.replace(handles);
    Ok(())
}

fn emit(app: &AppHandle, event: StreamEvent) {
    let _ = app.emit(STREAM_EVENT, event);
}

async fn follow(app: AppHandle, api: Api, logbook_id: String) {
    let mut after = None;
    let mut generation = None;
    let mut delay = RETRY_MIN;
    loop {
        match api.events(&logbook_id, after).await {
            Ok(mut response) => {
                let mut parser = SseParser::default();
                while let Ok(Some(chunk)) = response.chunk().await {
                    for event in parser.feed(&chunk) {
                        let Ok(data) = serde_json::from_str::<Value>(&event.data) else {
                            continue;
                        };
                        match event.event.as_str() {
                            "hello" => {
                                delay = RETRY_MIN;
                                let current = data.get("cursor_generation").and_then(Value::as_str);
                                if generation.as_deref() != current {
                                    generation = current.map(str::to_owned);
                                    after = data.get("last_updated_seq").and_then(Value::as_u64);
                                }
                                emit(
                                    &app,
                                    StreamEvent::Hello {
                                        logbook_id: logbook_id.clone(),
                                        hello: data,
                                    },
                                );
                                emit(
                                    &app,
                                    StreamEvent::Status {
                                        logbook_id: logbook_id.clone(),
                                        live: true,
                                    },
                                );
                            }
                            "qso" => {
                                if let Some(seq) = event.id.as_deref().and_then(|id| id.parse().ok()) {
                                    after = Some(after.map_or(seq, |a: u64| a.max(seq)));
                                }
                                emit(
                                    &app,
                                    StreamEvent::Qso {
                                        logbook_id: logbook_id.clone(),
                                        data,
                                    },
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }
            Err(ApiError::Unauthorized) | Err(ApiError::Server { status: 404, .. }) => {
                emit(
                    &app,
                    StreamEvent::Status {
                        logbook_id,
                        live: false,
                    },
                );
                return;
            }
            Err(_) => {}
        }
        emit(
            &app,
            StreamEvent::Status {
                logbook_id: logbook_id.clone(),
                live: false,
            },
        );
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(RETRY_MAX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(event: &str, id: Option<&str>, data: &str) -> SseEvent {
        SseEvent {
            event: event.to_owned(),
            id: id.map(str::to_owned),
            data: data.to_owned(),
        }
    }

    #[test]
    fn reads_events_split_across_chunks() {
        let mut parser = SseParser::default();
        assert!(parser.feed(b"event: hello\ndata: {\"a\"").is_empty());
        assert_eq!(
            parser.feed(b":1}\n\nevent: qso\r\nid: 41\r\ndata: {}\r\n\r\n"),
            vec![event("hello", None, "{\"a\":1}"), event("qso", Some("41"), "{}")]
        );
    }

    #[test]
    fn skips_comments_and_events_without_data() {
        let mut parser = SseParser::default();
        assert!(parser.feed(b": keep-alive\n\nevent: ping\n\n").is_empty());
        assert_eq!(parser.feed(b"data: x\n\n"), vec![event("", None, "x")]);
    }

    #[test]
    fn joins_data_lines() {
        let mut parser = SseParser::default();
        assert_eq!(parser.feed(b"data: a\ndata:b\n\n"), vec![event("", None, "a\nb")]);
    }

    #[test]
    fn keeps_a_multibyte_character_split_between_chunks() {
        let mut parser = SseParser::default();
        let text = "data: 東京\n\n".as_bytes();
        assert!(parser.feed(&text[..8]).is_empty());
        assert_eq!(parser.feed(&text[8..]), vec![event("", None, "東京")]);
    }
}
