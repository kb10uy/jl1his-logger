mod datagram;
mod inbox;

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
use tauri::{AppHandle, Emitter, State};

use crate::{
    config::Listener,
    listen::{
        datagram::Datagram,
        inbox::{Inbox, Incoming},
    },
};

pub const LISTENER_EVENT: &str = "listener-state";
pub const INCOMING_EVENT: &str = "incoming-qso";
pub const ENTRY_EVENT: &str = "entry-fields";
const TICK: Duration = Duration::from_millis(500);
const RETRY: Duration = Duration::from_secs(5);
const BUFFER: usize = 65_536;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ListenerState {
    Off,
    Listening { address: String },
    Failed { address: String, error: String },
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
        lock(&self.shared).inbox.pending().to_vec()
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

#[tauri::command]
pub fn listener_states(listening: State<'_, Listeners>) -> Vec<ListenerState> {
    listening.states()
}

#[tauri::command]
pub fn incoming(listening: State<'_, Listeners>) -> Vec<Incoming> {
    listening.incoming()
}

#[tauri::command]
pub fn ack_incoming(listening: State<'_, Listeners>, ids: Vec<u64>) {
    listening.ack(&ids);
}
