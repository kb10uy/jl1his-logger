mod datagram;
mod entry;
mod inbox;
mod subscribers;

use std::{
    io::ErrorKind,
    net::{SocketAddr, UdpSocket},
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
        datagram::{Datagram, Fields},
        entry::EntryForm,
        inbox::{Inbox, Incoming},
        subscribers::Subscribers,
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
    Listening { address: String, subscribers: usize },
    Failed { address: String, error: String },
}

type Targets = Vec<(Arc<UdpSocket>, SocketAddr)>;

#[derive(Default)]
struct Shared {
    generation: u64,
    states: Vec<ListenerState>,
    sockets: Vec<Option<Arc<UdpSocket>>>,
    inbox: Inbox,
    entry: EntryForm,
    subscribers: Subscribers,
}

impl Shared {
    fn targets(&self) -> Targets {
        self.subscribers
            .all()
            .iter()
            .filter_map(|s| Some((Arc::clone(self.sockets.get(s.listener)?.as_ref()?), s.address)))
            .collect()
    }

    fn part(&mut self) -> Targets {
        let targets = self.targets();
        self.subscribers.take();
        targets
    }

    fn recount(&mut self, app: &AppHandle) {
        let mut changed = false;
        for (index, state) in self.states.iter_mut().enumerate() {
            if let ListenerState::Listening { subscribers, .. } = state {
                let count = self.subscribers.count(index);
                changed |= *subscribers != count;
                *subscribers = count;
            }
        }
        if changed {
            let _ = app.emit(LISTENER_EVENT, self.states.clone());
        }
    }
}

#[derive(Default)]
pub struct Listeners {
    running: Mutex<Vec<(Arc<AtomicBool>, JoinHandle<()>)>>,
    shared: Arc<Mutex<Shared>>,
}

struct Slot {
    generation: u64,
    index: usize,
    address: String,
    subscribe: bool,
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

fn send(targets: &Targets, bytes: &[u8]) {
    for (socket, address) in targets {
        let _ = socket.send_to(bytes, address);
    }
}

fn publish(app: &AppHandle, shared: &Mutex<Shared>, slot: &Slot, next: ListenerState) {
    let mut shared = lock(shared);
    if shared.generation != slot.generation || shared.states.get(slot.index) == Some(&next) {
        return;
    }
    shared.states[slot.index] = next;
    let _ = app.emit(LISTENER_EVENT, shared.states.clone());
}

fn hold_socket(shared: &Mutex<Shared>, slot: &Slot, socket: Option<&Arc<UdpSocket>>) {
    let mut shared = lock(shared);
    if shared.generation != slot.generation {
        return;
    }
    if let Some(held) = shared.sockets.get_mut(slot.index) {
        *held = socket.cloned();
    }
    if socket.is_none() {
        shared.subscribers.forget_listener(slot.index);
    }
}

impl Listeners {
    pub fn start(&self, app: &AppHandle, listeners: &[Listener]) {
        let mut running = lock(&self.running);
        stop_all(&mut running);
        let (generation, targets) = {
            let mut shared = lock(&self.shared);
            let targets = shared.part();
            shared.generation += 1;
            shared.states = vec![ListenerState::Off; listeners.len()];
            shared.sockets = vec![None; listeners.len()];
            let _ = app.emit(LISTENER_EVENT, shared.states.clone());
            (shared.generation, targets)
        };
        send(&targets, &datagram::bye());
        for (index, listener) in listeners.iter().enumerate().filter(|(_, l)| l.enabled) {
            let stop = Arc::new(AtomicBool::new(false));
            let app = app.clone();
            let shared = Arc::clone(&self.shared);
            let slot = Slot {
                generation,
                index,
                address: listener.address(),
                subscribe: listener.subscribe,
            };
            let stopped = Arc::clone(&stop);
            let handle = thread::spawn(move || run(&app, &shared, &slot, &stopped));
            running.push((stop, handle));
        }
    }

    pub fn stop(&self) {
        stop_all(&mut lock(&self.running));
        let targets = lock(&self.shared).part();
        send(&targets, &datagram::bye());
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

    pub fn attach_entry(&self) -> Fields {
        lock(&self.shared).entry.attach()
    }

    pub fn detach_entry(&self) {
        lock(&self.shared).entry.detach();
    }

    pub fn share_form(&self, fields: &Fields) {
        let (targets, bytes) = {
            let mut shared = lock(&self.shared);
            let Some(form) = shared.entry.update(fields) else {
                return;
            };
            let bytes = datagram::form(form);
            (shared.targets(), bytes)
        };
        send(&targets, &bytes);
    }

    pub fn forget_form(&self, app: &AppHandle) {
        let targets = {
            let mut shared = lock(&self.shared);
            shared.entry.forget();
            let targets = shared.part();
            shared.recount(app);
            targets
        };
        send(&targets, &datagram::bye());
    }
}

fn run(app: &AppHandle, shared: &Mutex<Shared>, slot: &Slot, stop: &AtomicBool) {
    let set = |state| publish(app, shared, slot, state);
    let failed = |error: String| ListenerState::Failed {
        address: slot.address.clone(),
        error,
    };
    let mut buffer = vec![0; BUFFER];
    while !stop.load(Ordering::Relaxed) {
        match bind(&slot.address) {
            Ok(socket) => {
                let socket = Arc::new(socket);
                hold_socket(shared, slot, Some(&socket));
                set(ListenerState::Listening {
                    address: slot.address.clone(),
                    subscribers: 0,
                });
                while !stop.load(Ordering::Relaxed) {
                    match socket.recv_from(&mut buffer) {
                        Ok((size, from)) => receive(app, shared, slot, &socket, from, &buffer[..size]),
                        Err(e)
                            if matches!(
                                e.kind(),
                                ErrorKind::WouldBlock
                                    | ErrorKind::TimedOut
                                    | ErrorKind::Interrupted
                                    | ErrorKind::ConnectionReset
                            ) => {}
                        Err(e) => {
                            hold_socket(shared, slot, None);
                            set(failed(e.to_string()));
                            break;
                        }
                    }
                    if slot.subscribe {
                        let mut shared = lock(shared);
                        shared.subscribers.expire(Instant::now());
                        shared.recount(app);
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

fn receive(app: &AppHandle, shared: &Mutex<Shared>, slot: &Slot, socket: &UdpSocket, from: SocketAddr, bytes: &[u8]) {
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
            let shown = lock(shared).entry.receive(message);
            if let Some(message) = shown {
                let _ = app.emit(ENTRY_EVENT, message);
            }
        }
        Datagram::Subscribe if slot.subscribe => {
            let mut shared = lock(shared);
            if !shared.subscribers.subscribe(from, slot.index, Instant::now()) {
                return;
            }
            let form = shared.entry.form().map(datagram::form);
            shared.recount(app);
            drop(shared);
            if let Some(bytes) = form {
                let _ = socket.send_to(&bytes, from);
            }
        }
        Datagram::Unsubscribe if slot.subscribe => {
            let mut shared = lock(shared);
            shared.subscribers.unsubscribe(from);
            shared.recount(app);
        }
        Datagram::Subscribe | Datagram::Unsubscribe | Datagram::Ignored => {}
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

#[tauri::command]
pub fn attach_entry(listening: State<'_, Listeners>) -> Fields {
    listening.attach_entry()
}

#[tauri::command]
pub fn detach_entry(listening: State<'_, Listeners>) {
    listening.detach_entry();
}

#[tauri::command]
pub fn share_form(listening: State<'_, Listeners>, fields: Fields) {
    listening.share_form(&fields);
}
