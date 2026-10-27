pub mod notify;

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{
    AppHandle, Manager, Wry,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
};

const SHOW: &str = "show";
const QUIT: &str = "quit";

pub struct Tray {
    close_to_tray: AtomicBool,
    icon: TrayIcon<Wry>,
    show: MenuItem<Wry>,
    quit: MenuItem<Wry>,
}

impl Tray {
    pub fn build(app: &AppHandle, close_to_tray: bool) -> tauri::Result<Tray> {
        let show = MenuItem::with_id(app, SHOW, "Show", true, None::<&str>)?;
        let quit = MenuItem::with_id(app, QUIT, "Quit", true, None::<&str>)?;
        let menu = Menu::with_items(app, &[&show, &quit])?;
        let mut builder = TrayIconBuilder::new()
            .menu(&menu)
            .show_menu_on_left_click(false)
            .tooltip(&app.package_info().name)
            .on_menu_event(|app, event| match event.id().as_ref() {
                SHOW => show_window(app),
                QUIT => app.exit(0),
                _ => {}
            })
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    show_window(tray.app_handle());
                }
            });
        if let Some(icon) = app.default_window_icon() {
            builder = builder.icon(icon.clone());
        }
        Ok(Tray {
            close_to_tray: AtomicBool::new(close_to_tray),
            icon: builder.build(app)?,
            show,
            quit,
        })
    }

    pub fn closes_to_tray(&self) -> bool {
        self.close_to_tray.load(Ordering::Relaxed)
    }

    pub fn set_close_to_tray(&self, close_to_tray: bool) {
        self.close_to_tray.store(close_to_tray, Ordering::Relaxed);
    }

    pub fn set_text(&self, tooltip: &str, show: &str, quit: &str) -> tauri::Result<()> {
        self.icon.set_tooltip(Some(tooltip))?;
        self.show.set_text(show)?;
        self.quit.set_text(quit)
    }
}

pub fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[tauri::command]
pub fn set_tray_text(tray: tauri::State<'_, Tray>, tooltip: String, show: String, quit: String) {
    let _ = tray.set_text(&tooltip, &show, &quit);
}
