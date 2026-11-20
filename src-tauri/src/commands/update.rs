//! Updates of Diskman itself (see `updates`).

use tauri::{AppHandle, Manager};

use crate::platform::reveal;
use crate::updates::{self, UpdateStatus, Updates};

/// What is known about updates, without asking GitHub.
#[tauri::command]
pub fn get_update_status(app: AppHandle) -> UpdateStatus {
    app.state::<Updates>().status()
}

/// Asks GitHub for a newer version now ("Check now" in the settings).
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> UpdateStatus {
    updates::check(&app).await
}

/// Installs the update found; Diskman ends and starts again. Only returns if it failed.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    updates::install(&app).await
}

/// Opens the website in the browser, to download the new version and install it over this one:
/// for copies that cannot update themselves, and when updates no longer come from GitHub with the
/// known key (`updates::Lost`).
#[tauri::command]
pub fn open_website() -> Result<(), String> {
    reveal::open_link(updates::WEBSITE_URL).map_err(|e| e.to_string())
}
