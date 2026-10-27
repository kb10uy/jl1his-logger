use tauri::AppHandle;

#[cfg(windows)]
const ICON: &[u8] = include_bytes!("../icons/128x128.png");

#[cfg(windows)]
pub fn register(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use std::fs;

    use tauri::Manager;
    use winreg::{RegKey, enums::HKEY_CURRENT_USER};

    let icon = app.path().app_local_data_dir()?.join("notification.png");
    if fs::read(&icon).ok().as_deref() != Some(ICON) {
        if let Some(dir) = icon.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&icon, ICON)?;
    }
    let path = format!(r"Software\Classes\AppUserModelId\{}", app.config().identifier);
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(path)?;
    key.set_value("DisplayName", &app.package_info().name)?;
    key.set_value("IconUri", &icon.display().to_string())?;
    Ok(())
}

#[cfg(windows)]
#[tauri::command]
pub async fn notify(app: AppHandle, title: String, body: String) {
    let id = app.config().identifier.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        let (first, rest) = body.split_once('\n').unwrap_or((&body, ""));
        let mut toast = tauri_winrt_notification::Toast::new(&id).title(&title).text1(first);
        if !rest.is_empty() {
            toast = toast.text2(rest);
        }
        let _ = toast.show();
    })
    .await;
}

#[cfg(not(windows))]
#[tauri::command]
pub async fn notify(app: AppHandle, title: String, body: String) {
    use tauri_plugin_notification::NotificationExt;

    let _ = app.notification().builder().title(title).body(body).show();
}
