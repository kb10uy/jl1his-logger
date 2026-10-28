use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State, Theme};

use crate::{
    config::{Config, Listener, Paths, Rig},
    error::CommandError,
    listen::Listeners,
    rig::RigWatcher,
    tray::Tray,
};

const LANGUAGES: [&str; 2] = ["en", "ja"];
const THEMES: [&str; 2] = ["light", "dark"];

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
    theme: Option<String>,
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
            theme: config.theme,
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
    theme: Option<String>,
    close_to_tray: bool,
    rig: Rig,
    listeners: Vec<Listener>,
}

pub fn language(language: Option<String>) -> Option<String> {
    language.filter(|l| LANGUAGES.contains(&l.as_str()))
}

pub fn theme(theme: Option<String>) -> Option<String> {
    theme.filter(|t| THEMES.contains(&t.as_str()))
}

pub fn window_theme(theme: Option<&str>) -> Option<Theme> {
    match theme {
        Some("light") => Some(Theme::Light),
        Some("dark") => Some(Theme::Dark),
        _ => None,
    }
}

pub fn apply_theme(app: &AppHandle, theme: Option<&str>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_theme(window_theme(theme));
    }
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
    config.theme = self::theme(change.theme);
    config.close_to_tray = change.close_to_tray;
    config.rig = change.rig;
    config.listeners = change.listeners;
    config.save(&paths.config)?;
    tray.set_close_to_tray(config.close_to_tray);
    apply_theme(&app, config.theme.as_deref());
    if restart {
        watcher.start(&app, &config.rig);
    }
    if relisten {
        listening.start(&app, &config.listeners);
    }
    Ok(Settings::of(&paths, config))
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

    #[rstest]
    #[case(Some("light"), Some("light"))]
    #[case(Some("dark"), Some("dark"))]
    #[case(Some("sepia"), None)]
    #[case(None, None)]
    fn keeps_only_known_themes(#[case] given: Option<&str>, #[case] expected: Option<&str>) {
        assert_eq!(theme(given.map(str::to_owned)).as_deref(), expected);
    }
}
