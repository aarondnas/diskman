//! The system Diskman runs on and administrator rights.

use serde::Serialize;

use crate::jobs::shutdown;
use crate::platform::elevation;

/// What the UI needs to know about the system.
#[derive(Serialize)]
pub struct PlatformInfo {
    /// "windows" or "linux"
    os: &'static str,
    /// Runs with administrator rights (Windows: elevated; Linux: root)
    elevated: bool,
    /// Can restart itself with administrator rights (Windows only, while not elevated)
    can_elevate: bool,
}

#[tauri::command]
pub fn get_platform() -> PlatformInfo {
    PlatformInfo { os: std::env::consts::OS, elevated: elevation::is_elevated(), can_elevate: elevation::can_elevate() }
}

/// Starts Diskman again with administrator rights (Windows asks for permission) and quits this
/// instance; a running sync is cancelled and the queue emptied. Returns false if the permission was
/// declined: then everything stays as it is.
#[tauri::command]
pub fn restart_as_admin(app: tauri::AppHandle) -> Result<bool, String> {
    if elevation::is_elevated() {
        return Ok(true);
    }
    if !elevation::restart_elevated().map_err(|e| e.to_string())? {
        return Ok(false);
    }
    // The new instance waits for this one to end (platform::elevation)
    shutdown::exit_after_stop(app, shutdown::QUIT_WAIT);
    Ok(true)
}
