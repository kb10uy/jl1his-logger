#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod config;
mod credentials;
mod events;
mod origin;
mod rig;
mod rigctld;
mod session;
mod settings;

use tauri::Manager;

use crate::{
    config::Config,
    events::Watchers,
    rig::RigWatcher,
    session::{Connection, Paths},
};

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let config = match config::path_argument(std::env::args().skip(1)) {
                Some(path) => path,
                None => app.path().app_config_dir()?.join(config::FILE_NAME),
            };
            let rig = RigWatcher::default();
            if let Ok(loaded) = Config::load(&config) {
                rig.start(app.handle(), &loaded.rig);
            }
            app.manage(Paths { config });
            app.manage(Connection::default());
            app.manage(Watchers::default());
            app.manage(rig);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            session::session,
            session::sign_in,
            session::sign_out,
            session::request,
            events::watch,
            settings::settings,
            settings::save_settings,
            settings::rig_state,
            settings::reconnect_rig
        ])
        .run(tauri::generate_context!())
        .expect("the Tauri application failed to run");
}
