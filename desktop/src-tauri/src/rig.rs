use std::{
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::{config::Rig, rigctld::Rigctld};

const TIMEOUT: Duration = Duration::from_secs(2);
const RETRY: Duration = Duration::from_secs(5);
pub const RIG_EVENT: &str = "rig-state";

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RigState {
    #[default]
    Off,
    Connecting {
        address: String,
    },
    Connected {
        address: String,
        model: Option<String>,
        frequency_hz: u64,
        mode: String,
    },
    Failed {
        address: String,
        error: String,
    },
}

#[derive(Default)]
struct Shared {
    generation: u64,
    state: RigState,
}

#[derive(Default)]
pub struct RigWatcher {
    running: Mutex<Option<(Sender<()>, JoinHandle<()>)>>,
    shared: Arc<Mutex<Shared>>,
}

fn publish(app: &AppHandle, shared: &Mutex<Shared>, generation: u64, next: RigState) {
    let mut shared = shared.lock().expect("the rig lock is never poisoned");
    if shared.generation != generation || shared.state == next {
        return;
    }
    shared.state = next.clone();
    let _ = app.emit(RIG_EVENT, next);
}

impl RigWatcher {
    pub fn start(&self, app: &AppHandle, rig: &Rig) {
        let mut running = self.running.lock().expect("the rig lock is never poisoned");
        running.take();
        let generation = {
            let mut shared = self.shared.lock().expect("the rig lock is never poisoned");
            shared.generation += 1;
            shared.generation
        };
        if !rig.enabled {
            publish(app, &self.shared, generation, RigState::Off);
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let app = app.clone();
        let shared = Arc::clone(&self.shared);
        let address = rig.address();
        let poll = Duration::from_millis(rig.poll_ms.clamp(200, 60_000));
        let handle = thread::spawn(move || run(&app, &shared, generation, &address, poll, &receiver));
        *running = Some((sender, handle));
    }

    pub fn stop(&self) {
        let running = self.running.lock().expect("the rig lock is never poisoned").take();
        if let Some((sender, handle)) = running {
            drop(sender);
            let _ = handle.join();
        }
    }

    pub fn state(&self) -> RigState {
        self.shared
            .lock()
            .expect("the rig lock is never poisoned")
            .state
            .clone()
    }
}

fn run(app: &AppHandle, shared: &Mutex<Shared>, generation: u64, address: &str, poll: Duration, stop: &Receiver<()>) {
    let wait = |duration| matches!(stop.recv_timeout(duration), Err(RecvTimeoutError::Timeout));
    let set = |state| publish(app, shared, generation, state);
    let failed = |error: String| RigState::Failed {
        address: address.to_owned(),
        error,
    };
    set(RigState::Connecting {
        address: address.to_owned(),
    });
    loop {
        match Rigctld::connect(address, TIMEOUT) {
            Ok(mut client) => {
                let model = client.model();
                loop {
                    match client.status() {
                        Ok(status) => set(RigState::Connected {
                            address: address.to_owned(),
                            model: model.clone(),
                            frequency_hz: status.frequency_hz,
                            mode: status.mode,
                        }),
                        Err(e) if e.is_fatal() => {
                            set(failed(e.to_string()));
                            break;
                        }
                        Err(e) => set(failed(e.to_string())),
                    }
                    if !wait(poll) {
                        return;
                    }
                }
            }
            Err(e) => set(failed(e.to_string())),
        }
        if !wait(RETRY) {
            return;
        }
    }
}
