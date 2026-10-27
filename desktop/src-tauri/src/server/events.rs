mod sse;

use std::{mem, sync::Mutex, time::Duration};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, State, async_runtime::JoinHandle};

use crate::{
    error::CommandError,
    server::{Api, ApiError, events::sse::SseParser, session::Connection},
};

const RETRY_MIN: Duration = Duration::from_secs(2);
const RETRY_MAX: Duration = Duration::from_secs(60);
pub const STREAM_EVENT: &str = "logbook-stream";

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
