//! System dialogs and the file manager.

use std::path::Path;

use tauri_plugin_dialog::DialogExt;

use crate::platform::reveal;

/// Shows a file or folder of a tree of changes in the file manager: the first of `paths`
/// (relative to the folder `root`, with "/") that exists, selected; if none exists, the nearest
/// folder above the first one (see `platform::reveal::show`).
#[tauri::command]
pub async fn reveal_path(root: String, paths: Vec<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = Path::new(root.trim());
        if !root.is_absolute() {
            return Err(format!("\"{}\" is not an absolute path.", root.display()));
        }
        let full: Vec<_> = paths.iter().map(|p| reveal::below(root, p)).collect();
        reveal::show(&full).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The Windows folder picker, as a modal dialog of the window; None if cancelled.
#[tauri::command]
pub async fn pick_folder(window: tauri::Window) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        window
            .dialog()
            .file()
            .set_parent(&window)
            .set_title("Choose folder")
            .blocking_pick_folder()
            .map(|p| p.to_string())
    })
    .await
    .map_err(|e| e.to_string())
}
