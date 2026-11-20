//! Data folder on Windows: `%APPDATA%\Diskman` (roaming, travels with the user profile).

use std::path::PathBuf;

pub fn data_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("Diskman"))
}
