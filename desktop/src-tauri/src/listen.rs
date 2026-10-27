use std::{
    io::ErrorKind,
    net::UdpSocket,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::{
    config::Listener,
    datagram::{self, Datagram, Fields},
};

pub const LISTENER_EVENT: &str = "listener-state";
pub const INCOMING_EVENT: &str = "incoming-qso";
pub const ENTRY_EVENT: &str = "entry-fields";
const TICK: Duration = Duration::from_millis(500);
const RETRY: Duration = Duration::from_secs(5);
const SAME_QSO: Duration = Duration::from_secs(120);
const BUFFER: usize = 65_536;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ListenerState {
    Off,
    Listening { address: String },
    Failed { address: String, error: String },
}

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
        self.seen.retain(|(_, at)| now.duration_since(*at) < SAME_QSO);
        if let Some(key) = same_qso(&fields) {
            if self.seen.iter().any(|(seen, _)| *seen == key) {
                return false;
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

#[derive(Default)]
struct Shared {
    generation: u64,
    states: Vec<ListenerState>,
    inbox: Inbox,
}

#[derive(Default)]
pub struct Listeners {
    running: Mutex<Vec<(Arc<AtomicBool>, JoinHandle<()>)>>,
    shared: Arc<Mutex<Shared>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().expect("the listener lock is never poisoned")
}

fn stop_all(running: &mut Vec<(Arc<AtomicBool>, JoinHandle<()>)>) {
    for (stop, _) in running.iter() {
        stop.store(true, Ordering::Relaxed);
    }
    for (_, handle) in running.drain(..) {
        let _ = handle.join();
    }
}

fn publish(app: &AppHandle, shared: &Mutex<Shared>, generation: u64, index: usize, next: ListenerState) {
    let mut shared = lock(shared);
    if shared.generation != generation || shared.states.get(index) == Some(&next) {
        return;
    }
    shared.states[index] = next;
    let _ = app.emit(LISTENER_EVENT, shared.states.clone());
}

impl Listeners {
    pub fn start(&self, app: &AppHandle, listeners: &[Listener]) {
        let mut running = lock(&self.running);
        stop_all(&mut running);
        let generation = {
            let mut shared = lock(&self.shared);
            shared.generation += 1;
            shared.states = vec![ListenerState::Off; listeners.len()];
            let _ = app.emit(LISTENER_EVENT, shared.states.clone());
            shared.generation
        };
        for (index, listener) in listeners.iter().enumerate().filter(|(_, l)| l.enabled) {
            let stop = Arc::new(AtomicBool::new(false));
            let app = app.clone();
            let shared = Arc::clone(&self.shared);
            let address = listener.address();
            let stopped = Arc::clone(&stop);
            let handle = thread::spawn(move || run(&app, &shared, generation, index, &address, &stopped));
            running.push((stop, handle));
        }
    }

    pub fn stop(&self) {
        stop_all(&mut lock(&self.running));
    }

    pub fn states(&self) -> Vec<ListenerState> {
        lock(&self.shared).states.clone()
    }

    pub fn incoming(&self) -> Vec<Incoming> {
        lock(&self.shared).inbox.pending.clone()
    }

    pub fn ack(&self, ids: &[u64]) {
        lock(&self.shared).inbox.ack(ids);
    }
}

fn run(app: &AppHandle, shared: &Mutex<Shared>, generation: u64, index: usize, address: &str, stop: &AtomicBool) {
    let set = |state| publish(app, shared, generation, index, state);
    let failed = |error: String| ListenerState::Failed {
        address: address.to_owned(),
        error,
    };
    let mut buffer = vec![0; BUFFER];
    while !stop.load(Ordering::Relaxed) {
        match bind(address) {
            Ok(socket) => {
                set(ListenerState::Listening {
                    address: address.to_owned(),
                });
                while !stop.load(Ordering::Relaxed) {
                    match socket.recv_from(&mut buffer) {
                        Ok((size, _)) => receive(app, shared, &buffer[..size]),
                        Err(e)
                            if matches!(
                                e.kind(),
                                ErrorKind::WouldBlock
                                    | ErrorKind::TimedOut
                                    | ErrorKind::Interrupted
                                    | ErrorKind::ConnectionReset
                            ) => {}
                        Err(e) => {
                            set(failed(e.to_string()));
                            break;
                        }
                    }
                }
            }
            Err(e) => set(failed(e.to_string())),
        }
        let retry = Instant::now() + RETRY;
        while !stop.load(Ordering::Relaxed) && Instant::now() < retry {
            thread::sleep(TICK);
        }
    }
}

fn bind(address: &str) -> std::io::Result<UdpSocket> {
    let socket = UdpSocket::bind(address)?;
    socket.set_read_timeout(Some(TICK))?;
    Ok(socket)
}

fn receive(app: &AppHandle, shared: &Mutex<Shared>, bytes: &[u8]) {
    match datagram::read(bytes) {
        Datagram::Qsos { program, records } => {
            let now = Instant::now();
            let mut added = false;
            let mut shared = lock(shared);
            for fields in records {
                added |= shared.inbox.push(program.clone(), fields, now);
            }
            drop(shared);
            if added {
                let _ = app.emit(INCOMING_EVENT, ());
            }
        }
        Datagram::Entry(message) => {
            let _ = app.emit(ENTRY_EVENT, message);
        }
        Datagram::Ignored => {}
    }
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
}
