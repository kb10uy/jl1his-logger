use std::{collections::VecDeque, convert::Infallible, sync::Arc, time::Duration};

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::{Stream, stream};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::{
    store::{
        StoreError,
        access::{self, LogbookAccess},
        logbook, qso,
    },
    web::{AppState, auth::Caller, error::ApiError, extract::Query, qso::QsoBody},
};

const POLL: Duration = Duration::from_secs(10);
const PAGE: u32 = 500;

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    after: Option<i64>,
}

#[derive(Debug, Serialize)]
struct Hello<'a> {
    cursor_generation: &'a str,
    last_updated_seq: i64,
    after: i64,
}

#[derive(Debug, Serialize)]
struct Change<'a> {
    cursor_generation: &'a str,
    qso: QsoBody,
}

struct Feed {
    state: Arc<AppState>,
    access: LogbookAccess,
    writes: watch::Receiver<u64>,
    closing: watch::Receiver<bool>,
    after: i64,
    pending: VecDeque<Event>,
    started: bool,
    ended: bool,
}

impl Feed {
    async fn next(mut self) -> Option<(Result<Event, Infallible>, Feed)> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Some((Ok(event), self));
            }
            if self.ended {
                return None;
            }
            if self.started {
                tokio::select! {
                    _ = self.writes.changed() => {}
                    _ = tokio::time::sleep(POLL) => {}
                    _ = self.closing.wait_for(|closing| *closing) => return None,
                }
            }
            self.started = true;
            if let Err(e) = self.read().await {
                if !matches!(e, StoreError::NotFound(_) | StoreError::Unauthenticated) {
                    tracing::error!("event stream of Logbook {} failed: {e}", self.access.logbook_id());
                }
                self.ended = true;
            }
        }
    }

    async fn read(&mut self) -> Result<(), StoreError> {
        let mut conn = self.state.db.acquire().await?;
        loop {
            let changes = qso::changes(&mut conn, &self.access, self.after, PAGE).await?;
            for row in changes.rows {
                self.after = row.updated_seq;
                let change = Change {
                    cursor_generation: &changes.cursor_generation,
                    qso: row.into(),
                };
                self.pending.push_back(
                    Event::default()
                        .event("qso")
                        .id(self.after.to_string())
                        .json_data(&change)
                        .expect("a change serializes"),
                );
            }
            if !changes.has_more {
                return Ok(());
            }
        }
    }
}

pub async fn stream(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path(logbook_id): Path<String>,
    Query(query): Query<EventsQuery>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let last_event_id = match headers.get("last-event-id") {
        Some(value) => Some(
            value
                .to_str()
                .ok()
                .and_then(|v| v.trim().parse::<i64>().ok())
                .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "Last-Event-ID is not an updated_seq"))?,
        ),
        None => None,
    };
    let mut conn = state.db.acquire().await?;
    let access = access::logbook(&mut conn, &caller.principal, &logbook_id).await?;
    let current = logbook::get(&mut conn, &access).await?;
    drop(conn);

    let after = last_event_id.or(query.after).unwrap_or(current.last_updated_seq);
    let hello = Hello {
        cursor_generation: &current.cursor_generation,
        last_updated_seq: current.last_updated_seq,
        after,
    };
    let hello = Event::default()
        .event("hello")
        .json_data(&hello)
        .expect("a greeting serializes");
    let feed = Feed {
        writes: state.writes.subscribe(),
        closing: state.closing.subscribe(),
        state,
        access,
        after,
        pending: VecDeque::from([hello]),
        started: false,
        ended: false,
    };
    Ok(Sse::new(stream::unfold(feed, Feed::next)).keep_alive(KeepAlive::default()))
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, header},
    };
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use super::*;
    use crate::{
        domain::qso::QsoId,
        web::testing::{Auth, Harness, authorize},
    };

    struct Events {
        body: Body,
        buffer: String,
    }

    #[derive(Debug, PartialEq)]
    struct Received {
        event: String,
        id: Option<String>,
        data: Value,
    }

    impl Events {
        async fn open(h: &Harness, auth: Auth<'_>, query: &str, last_event_id: Option<&str>) -> (StatusCode, Events) {
            let uri = format!("/api/logbooks/{}/events{query}", h.world.logbook.id);
            let mut req = authorize(Request::get(uri), auth);
            if let Some(id) = last_event_id {
                req = req.header("last-event-id", id);
            }
            let response = h.app.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
            let status = response.status();
            if status.is_success() {
                assert_eq!(response.headers()[header::CONTENT_TYPE], "text/event-stream");
            }
            let events = Events {
                body: response.into_body(),
                buffer: String::new(),
            };
            (status, events)
        }

        async fn next(&mut self) -> Option<Received> {
            loop {
                if let Some(end) = self.buffer.find("\n\n") {
                    let block: String = self.buffer.drain(..end + 2).collect();
                    let mut received = Received {
                        event: String::new(),
                        id: None,
                        data: Value::Null,
                    };
                    for line in block.lines() {
                        match line.split_once(':') {
                            Some(("event", v)) => received.event = v.trim().to_owned(),
                            Some(("id", v)) => received.id = Some(v.trim().to_owned()),
                            Some(("data", v)) => received.data = serde_json::from_str(v.trim()).unwrap(),
                            _ => {}
                        }
                    }
                    if received.event.is_empty() {
                        continue;
                    }
                    return Some(received);
                }
                let frame = tokio::time::timeout(Duration::from_secs(5), self.body.frame())
                    .await
                    .expect("no event within 5 seconds")?
                    .unwrap();
                if let Ok(data) = frame.into_data() {
                    self.buffer.push_str(std::str::from_utf8(&data).unwrap());
                }
            }
        }
    }

    async fn create(h: &Harness, cookie: &str, call: &str) -> Value {
        let body = json!({
            "id": QsoId::generate().to_string(),
            "station_id": h.station.id,
            "fields": { "CALL": call, "QSO_DATE": "20261022", "TIME_ON": "0101", "BAND": "40m", "MODE": "CW" },
        });
        let uri = format!("/api/logbooks/{}/qsos", h.world.logbook.id);
        let (status, created) = h.send("POST", &uri, Auth::Session(cookie), Some(body)).await;
        assert_eq!(status, StatusCode::CREATED);
        created["qso"].clone()
    }

    fn hello(h: &Harness, last_updated_seq: i64, after: i64) -> Received {
        Received {
            event: "hello".into(),
            id: None,
            data: json!({
                "cursor_generation": h.world.logbook.cursor_generation,
                "last_updated_seq": last_updated_seq,
                "after": after,
            }),
        }
    }

    fn change(h: &Harness, qso: &Value) -> Received {
        Received {
            event: "qso".into(),
            id: Some(qso["updated_seq"].to_string()),
            data: json!({ "cursor_generation": h.world.logbook.cursor_generation, "qso": qso }),
        }
    }

    #[tokio::test]
    async fn streams_the_backlog_then_each_write() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        let first = create(&h, &cookie, "JA1AAA").await;

        let (status, mut events) = Events::open(&h, Auth::Session(&cookie), "?after=0", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(events.next().await, Some(hello(&h, 1, 0)));
        assert_eq!(events.next().await, Some(change(&h, &first)));

        let second = create(&h, &cookie, "JA1BBB").await;
        assert_eq!(events.next().await, Some(change(&h, &second)));

        let uri = format!(
            "/api/logbooks/{}/qsos/{}",
            h.world.logbook.id,
            first["id"].as_str().unwrap()
        );
        let (status, _, deleted) = h
            .send_with("DELETE", &uri, Auth::Session(&cookie), &[("if-match", "\"1\"")], None)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(events.next().await, Some(change(&h, &deleted)));

        h.state.close_streams();
        assert_eq!(events.next().await, None);
    }

    #[tokio::test]
    async fn resumes_after_the_last_event_id_or_from_now() {
        let h = Harness::new().await;
        let cookie = h.session(&h.world.operator).await;
        create(&h, &cookie, "JA1AAA").await;
        let second = create(&h, &cookie, "JA1BBB").await;

        let (_, mut resumed) = Events::open(&h, Auth::Session(&cookie), "?after=0", Some("1")).await;
        assert_eq!(resumed.next().await, Some(hello(&h, 2, 1)));
        assert_eq!(resumed.next().await, Some(change(&h, &second)));

        let token = h
            .token(
                &h.world.operator,
                crate::store::token::TokenKind::Native,
                &[&h.world.logbook.id],
            )
            .await;
        let (status, mut live) = Events::open(&h, Auth::Bearer(&token), "", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(live.next().await, Some(hello(&h, 2, 2)));
        let third = create(&h, &cookie, "JA1CCC").await;
        assert_eq!(live.next().await, Some(change(&h, &third)));
    }

    #[tokio::test]
    async fn ends_when_the_member_loses_the_logbook() {
        let h = Harness::new().await;
        let operator = h.session(&h.world.operator).await;
        let owner = h.session(&h.world.owner).await;
        let (_, mut events) = Events::open(&h, Auth::Session(&operator), "", None).await;
        assert_eq!(events.next().await, Some(hello(&h, 0, 0)));

        let uri = format!("/api/logbooks/{}/members/JA1OPR", h.world.logbook.id);
        let (status, _) = h.send("DELETE", &uri, Auth::Session(&owner), None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(events.next().await, None);
    }

    #[tokio::test]
    async fn refuses_callers_and_cursors_it_cannot_use() {
        let h = Harness::new().await;
        let outsider = h.session(&h.world.outsider).await;
        let operator = h.session(&h.world.operator).await;
        let (status, _) = Events::open(&h, Auth::Anonymous, "", None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let (status, _) = Events::open(&h, Auth::Session(&outsider), "", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _) = Events::open(&h, Auth::Session(&operator), "", Some("later")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = Events::open(&h, Auth::Session(&operator), "?after=x", None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
