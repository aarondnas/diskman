//! Windows implementation of the platform facade (see `platform`). Only compiled on Windows.

pub mod autostart;
pub mod elevation;
pub mod fs;
pub mod journal;
pub mod locale;
pub mod paths;
pub mod power;
pub mod reveal;
pub mod volume;
pub mod window;

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

/// Zero-terminated UTF-16 string for the Win32 API.
fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(Some(0)).collect()
}
