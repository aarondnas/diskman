//! Where Diskman keeps its data, following the conventions of each system:
//!
//! - Windows: `%APPDATA%\Diskman`
//! - Linux:   `$XDG_DATA_HOME/diskman`, usually `~/.local/share/diskman`
//!
//! `DISKMAN_DATA_DIR` overrides this everywhere (see `storage::data_dir`).

use std::path::PathBuf;

use super::sys;

/// The default data folder; `None` if the system does not name a home for app data.
pub fn data_dir() -> Option<PathBuf> {
    sys::paths::data_dir()
}
