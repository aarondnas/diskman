//! Starting Diskman at login (`platform::autostart`): the question on the first start and the
//! switch in the settings.

use serde::Serialize;

use super::Shared;
use crate::platform::autostart;

#[derive(Debug, Serialize)]
pub struct Autostart {
    /// The system starts Diskman at login
    pub enabled: bool,
    /// The user answered already (in the app or in the installer), so the app does not ask
    pub asked: bool,
}

#[tauri::command]
pub fn get_autostart(state: Shared) -> Autostart {
    let enabled = autostart::is_enabled();
    let asked = enabled || autostart::asked_by_installer() || state.config.lock().unwrap().settings.autostart_asked;
    Autostart { enabled, asked }
}

/// Switches autostart on or off and notes that the user answered.
#[tauri::command]
pub fn set_autostart(state: Shared, enabled: bool) -> Result<Autostart, String> {
    autostart::set_enabled(enabled).map_err(|e| format!("{e:#}"))?;
    let mut config = state.config.lock().unwrap().clone();
    if !config.settings.autostart_asked {
        config.settings.autostart_asked = true;
        state.save_config(config).map_err(|e| format!("{e:#}"))?;
    }
    Ok(get_autostart(state))
}
