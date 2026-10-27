#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod config;
mod credentials;
mod events;
mod origin;
mod session;

use tauri::Manager;

use crate::{
    events::Watchers,
    session::{Connection, Paths},
};

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let config = match config::path_argument(std::env::args().skip(1)) {
                Some(path) => path,
                None => app.path().app_config_dir()?.join(config::FILE_NAME),
            };
            app.manage(Paths { config });
            app.manage(Connection::default());
            app.manage(Watchers::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            session::session,
            session::sign_in,
            session::sign_out,
            session::request,
            events::watch
        ])
        .run(tauri::generate_context!())
        .expect("the Tauri application failed to run");
}
