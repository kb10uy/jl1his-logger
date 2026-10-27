use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::{
    config::{Config, Listener, Rig},
    listen::{Incoming, ListenerState, Listeners},
    rig::{RigState, RigWatcher},
    session::{CommandError, Paths},
    tray::Tray,
};

const LANGUAGES: [&str; 2] = ["en", "ja"];

const CREDENTIAL_STORE: &str = if cfg!(windows) {
    "Windows Credential Manager"
} else if cfg!(target_os = "macos") {
    "macOS Keychain"
} else {
    "Secret Service"
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    path: String,
    credential_store: &'static str,
    language: Option<String>,
    close_to_tray: bool,
    rig: Rig,
    listeners: Vec<Listener>,
}

impl Settings {
    fn of(paths: &Paths, config: Config) -> Settings {
        Settings {
            path: paths.config.display().to_string(),
            credential_store: CREDENTIAL_STORE,
            language: config.language,
            close_to_tray: config.close_to_tray,
            rig: config.rig,
            listeners: config.listeners,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    language: Option<String>,
    close_to_tray: bool,
    rig: Rig,
    listeners: Vec<Listener>,
}

pub fn language(language: Option<String>) -> Option<String> {
    language.filter(|l| LANGUAGES.contains(&l.as_str()))
}

#[tauri::command]
pub fn settings(paths: State<'_, Paths>) -> Result<Settings, CommandError> {
    Ok(Settings::of(&paths, Config::load(&paths.config)?))
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    paths: State<'_, Paths>,
    watcher: State<'_, RigWatcher>,
    listening: State<'_, Listeners>,
    tray: State<'_, Tray>,
    change: Change,
) -> Result<Settings, CommandError> {
    let mut config = Config::load(&paths.config)?;
    let restart = !config.rig.same_connection(&change.rig);
    let relisten = config.listeners != change.listeners;
    config.language = self::language(change.language);
    config.close_to_tray = change.close_to_tray;
    config.rig = change.rig;
    config.listeners = change.listeners;
    config.save(&paths.config)?;
    tray.set_close_to_tray(config.close_to_tray);
    if restart {
        watcher.start(&app, &config.rig);
    }
    if relisten {
        listening.start(&app, &config.listeners);
    }
    Ok(Settings::of(&paths, config))
}

#[tauri::command]
pub fn rig_state(watcher: State<'_, RigWatcher>) -> RigState {
    watcher.state()
}

#[tauri::command]
pub fn reconnect_rig(
    app: AppHandle,
    paths: State<'_, Paths>,
    watcher: State<'_, RigWatcher>,
) -> Result<(), CommandError> {
    watcher.start(&app, &Config::load(&paths.config)?.rig);
    Ok(())
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

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(Some("ja"), Some("ja"))]
    #[case(Some("en"), Some("en"))]
    #[case(Some("fr"), None)]
    #[case(None, None)]
    fn keeps_only_known_languages(#[case] given: Option<&str>, #[case] expected: Option<&str>) {
        assert_eq!(language(given.map(str::to_owned)).as_deref(), expected);
    }
}
