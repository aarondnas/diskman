//! Reading and saving the configuration.

use tauri::AppHandle;

use super::Shared;
use crate::model::Config;
use crate::platform::window;

#[tauri::command]
pub fn get_config(state: Shared) -> Config {
    state.config.lock().unwrap().clone()
}

/// Validates and saves the whole config; returns it as stored. The window takes over its theme.
#[tauri::command]
pub fn save_config(app: AppHandle, state: Shared, config: Config) -> Result<Config, String> {
    config.validate()?;
    state.save_config(config.clone()).map_err(|e| format!("{e:#}"))?;
    window::apply_theme(&app, config.settings.light_mode);
    Ok(config)
}

/// Folder with config.json and diskman.db, shown on the info page.
#[tauri::command]
pub fn get_data_dir(state: Shared) -> String {
    state.data_dir.display().to_string()
}
