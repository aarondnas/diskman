//! Window frame and icon on Linux: the desktop draws the native frame (decorations in
//! tauri.conf.json; the window's theme, set in platform/window.rs, asks GTK for its dark or light
//! variant), and Tauri sets the icon from the PNG icons. Nothing to do.

pub fn apply_frame(_window: &tauri::Window, _light: bool) {}

pub fn apply_icon(_window: &tauri::Window) {}
