#[cfg(windows)]
mod balloon;

#[cfg(windows)]
#[tauri::command]
pub async fn notify(title: String, body: String) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        if balloon::show(&title, &body) {
            return;
        }
        use tauri_winrt_notification::Toast;
        let (first, rest) = body.split_once('\n').unwrap_or((&body, ""));
        let mut toast = Toast::new(Toast::POWERSHELL_APP_ID).title(&title).text1(first);
        if !rest.is_empty() {
            toast = toast.text2(rest);
        }
        let _ = toast.show();
    })
    .await;
}

#[cfg(not(windows))]
#[tauri::command]
pub async fn notify(app: tauri::AppHandle, title: String, body: String) {
    use tauri_plugin_notification::NotificationExt;

    let _ = app.notification().builder().title(title).body(body).show();
}
