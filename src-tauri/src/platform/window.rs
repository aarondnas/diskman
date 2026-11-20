//! The main window, its frame and icon.
//!
//! The window is not created at the start (`create: false` in tauri.conf.json): a start at login
//! (`autostart::BACKGROUND_ARG`) runs without one and saves the memory of the webview. It is
//! created on the first `show_main`: at a normal start, from the tray or by starting Diskman again.
//!
//! Windows: the window has no native title bar (tauri.windows.conf.json: `decorations: false`);
//! the UI draws it (src/platform/windows/TitleBar.svelte) and the frame colors and icon sizes are
//! set here. Linux: the desktop draws the native frame, there is nothing to do.
//!
//! Theme: dark by default, light when chosen in the settings (`settings.light_mode`), whatever the
//! system's mode. The window's theme (what the webview and GTK see) and the frame follow it.

use std::sync::Arc;

use tauri::{AppHandle, Manager, Theme, WebviewWindow, WebviewWindowBuilder};

use super::sys;
use crate::state::AppState;

const MAIN: &str = "main";

/// Shows the main window, creating it first if there is none yet; brings it back if it was
/// hidden (closed to the tray) or minimized.
pub fn show_main(app: &AppHandle) {
    let window = match app.get_webview_window(MAIN) {
        Some(window) => window,
        None => match create_main(app) {
            Ok(window) => window,
            Err(e) => {
                eprintln!("Main window not created: {e}");
                return;
            }
        },
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

/// The main window as configured in tauri.conf.json (and tauri.<system>.conf.json).
fn create_main(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let config = app.config().app.windows.iter().find(|w| w.label == MAIN).cloned().unwrap_or_default();
    let light = light_mode(app);
    let webview = WebviewWindowBuilder::from_config(app, &config)?.theme(Some(theme(light))).build()?;
    let window = webview.as_ref().window();
    apply_frame(&window, light);
    apply_icon(&window);
    Ok(webview)
}

/// Light mode chosen in the settings (false = the default dark theme).
pub fn light_mode(app: &AppHandle) -> bool {
    app.try_state::<Arc<AppState>>().is_some_and(|s| s.config.lock().unwrap().settings.light_mode)
}

fn theme(light: bool) -> Theme {
    if light {
        Theme::Light
    } else {
        Theme::Dark
    }
}

/// Switches the main window (if there is one) to the light or dark theme.
pub fn apply_theme(app: &AppHandle, light: bool) {
    if let Some(webview) = app.get_webview_window(MAIN) {
        let _ = webview.set_theme(Some(theme(light)));
        apply_frame(&webview.as_ref().window(), light);
    }
}

/// Frame in the app's colors; call again when Windows may have reset them.
pub fn apply_frame(window: &tauri::Window, light: bool) {
    sys::window::apply_frame(window, light);
}

/// Sharp window icon for the current scaling; call again when the scaling changes.
pub fn apply_icon(window: &tauri::Window) {
    sys::window::apply_icon(window);
}
