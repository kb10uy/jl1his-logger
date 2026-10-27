#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod config;
mod credentials;
mod datagram;
mod events;
mod listen;
mod notify;
mod origin;
mod rig;
mod rigctld;
mod session;
mod settings;
mod tray;

use tauri::{Manager, RunEvent, WindowEvent};

use crate::{
    config::Config,
    events::Watchers,
    listen::Listeners,
    rig::RigWatcher,
    session::{Connection, Paths},
    tray::Tray,
};

fn main() {
    let builder = tauri::Builder::default();
    #[cfg(not(debug_assertions))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| tray::show_window(app)));
    builder
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let config = match config::path_argument(std::env::args().skip(1)) {
                Some(path) => path,
                None => app.path().app_config_dir()?.join(config::FILE_NAME),
            };
            let rig = RigWatcher::default();
            let listeners = Listeners::default();
            let loaded = Config::load(&config).ok();
            if let Some(loaded) = &loaded {
                rig.start(app.handle(), &loaded.rig);
                listeners.start(app.handle(), &loaded.listeners);
            }
            let close_to_tray = loaded.as_ref().is_none_or(|l| l.close_to_tray);
            app.manage(Tray::build(app.handle(), close_to_tray)?);
            app.manage(Paths { config });
            app.manage(Connection::default());
            app.manage(Watchers::default());
            app.manage(rig);
            app.manage(listeners);
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
            settings::reconnect_rig,
            settings::listener_states,
            settings::incoming,
            settings::ack_incoming,
            tray::set_tray_text,
            notify::notify
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.try_state::<Tray>().is_some_and(|tray| tray.closes_to_tray())
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("the Tauri application failed to build")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                app.state::<RigWatcher>().stop();
                app.state::<Listeners>().stop();
            }
        });
}
