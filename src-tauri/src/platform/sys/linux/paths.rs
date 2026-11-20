//! Data folder on Linux: `$XDG_DATA_HOME/diskman`, usually `~/.local/share/diskman` (XDG Base
//! Directory specification; config and history stay together in one folder, as on Windows).

use std::path::PathBuf;

pub fn data_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("diskman"))
}
