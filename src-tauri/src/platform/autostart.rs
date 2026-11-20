//! Starting Diskman when the user logs in, in the background: without a window, only the tray
//! icon, the schedules and the catch-up of missed runs (see `lib.rs`, `BACKGROUND_ARG`).
//!
//! - Windows: a value in the user's Run key, as "Startup apps" in the Windows settings and the
//!   Task Manager show it (and can switch it off). The NSIS installer asks during installation
//!   and writes the same value (src-tauri/windows/installer-hooks.nsh).
//! - Linux: `~/.config/autostart/diskman.desktop` (XDG autostart, read by every desktop). Packages
//!   cannot ask per user while installing, so the app asks on its first start.
//!
//! The system's entry is the only truth: whether autostart is on is read from it every time, so a
//! change in the system settings shows in Diskman.

use std::path::PathBuf;

use super::sys;

/// Command line argument of a start at login: no window, missed runs are caught up right away.
pub const BACKGROUND_ARG: &str = "--background";

/// Does the system start Diskman at login?
pub fn is_enabled() -> bool {
    sys::autostart::is_enabled()
}

/// Adds or removes the autostart entry; it starts this program with `BACKGROUND_ARG`.
pub fn set_enabled(on: bool) -> anyhow::Result<()> {
    sys::autostart::set_enabled(on, &program()?)
}

/// Did the installer already ask (whatever the answer)? Then the app does not ask again.
pub fn asked_by_installer() -> bool {
    sys::autostart::asked_by_installer()
}

/// The program the autostart entry runs.
fn program() -> anyhow::Result<PathBuf> {
    sys::autostart::program()
}
