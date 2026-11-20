//! Diskman keeps running in the background when the window is closed: the X button (or Alt+F4)
//! only hides the window, scheduled backups and automatic syncs keep running.
//! The icon in the notification area (Windows) or system tray (Linux) brings the window back and
//! quits Diskman. The same code serves both systems; they differ in what they report:
//!
//! - Windows: a left click opens the window, a right click shows the menu.
//! - Linux (AppIndicator): every click shows the menu, so "Open Diskman" is in it. Desktops
//!   without a tray (GNOME without extension) show no icon at all; starting Diskman again brings
//!   the hidden window back (single-instance plugin in lib.rs).

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::AppHandle;

use super::window;
use crate::jobs::shutdown;

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Diskman", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &PredefinedMenuItem::separator(app)?, &quit])?;

    // 32 px instead of the large window icon: the pixel rack stays sharp in the small notification area
    let icon = Image::from_bytes(include_bytes!("../../icons/32x32.png"))?;
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("Diskman")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "quit" => shutdown::request_quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Shows the main window, also after a start in the background (it is created then).
pub fn show_main_window(app: &AppHandle) {
    window::show_main(app);
}
