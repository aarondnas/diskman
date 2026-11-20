//! Everything that depends on the operating system. Diskman supports Windows and Linux.
//!
//! The modules here are the facade the rest of the app uses; they hold the shared types, the
//! documentation and the logic that is the same everywhere. The OS-specific half of each lives in
//! `sys/<os>/` and is only compiled on that system (the same split as Rust's standard library):
//!
//! - `autostart` starting Diskman in the background at login
//! - `elevation` administrator rights (Windows: UAC restart; Linux: root is only detected)
//! - `fs`        file details: OS error codes, encryption, read-only flag, timestamps
//! - `locale`    the system's display language (for texts shown without a window)
//! - `journal`   the change journal of a volume (NTFS USN): which folders changed since a point
//! - `paths`     where Diskman keeps its data
//! - `power`     keeping the system from sleeping or shutting down while a sync runs
//! - `reveal`    showing a file or folder in the file manager (Explorer, Nautilus, Dolphin …) and
//!   opening a web page in the browser
//! - `volume`    limits of the file system an archive lives on (FAT32, exFAT …)
//! - `window`    the main window (created on demand), its frame and icon
//! - `tray`      the notification area / system tray icon (Tauri, the same code everywhere)
//! - `notification` a system notification (Tauri plugin, the same code everywhere)
//!
//! A new OS-specific function: declare it in the facade module, implement it in every
//! `sys/<os>/` module. A missing implementation is a compile error on that system, not a surprise
//! at run time. See docs/platforms.md.

pub mod autostart;
pub mod elevation;
pub mod fs;
pub mod journal;
pub mod locale;
pub mod notification;
pub mod paths;
pub mod power;
pub mod reveal;
pub mod tray;
pub mod volume;
pub mod window;

#[cfg(target_os = "linux")]
#[path = "sys/linux/mod.rs"]
mod sys;
#[cfg(windows)]
#[path = "sys/windows/mod.rs"]
mod sys;

#[cfg(not(any(windows, target_os = "linux")))]
compile_error!("Diskman supports Windows and Linux only.");
