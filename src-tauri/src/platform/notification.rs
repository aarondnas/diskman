//! A system notification: a toast in the bottom right corner on Windows, the desktop's
//! notification on Linux (libnotify; GNOME shows it at the top). The same code on both systems.

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

/// Shows a notification; if the system refuses, it is only logged.
pub fn show(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        eprintln!("Notification not shown: {e}");
    }
}
