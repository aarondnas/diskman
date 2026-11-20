//! Showing a file or folder in the system's file manager (Windows: Explorer; Linux: the desktop's
//! file manager, e.g. Nautilus, Dolphin, Nemo), from the trees of changes in the UI; and opening a
//! web page (the release page of an update) in the browser.

use std::io;
use std::path::{Path, PathBuf};

use super::sys;

/// Opens the file manager with the first of `paths` that exists selected. If none exists (a file
/// not copied yet, or already deleted), the nearest folder above the first one that does is
/// opened instead. Err if not even that exists (e.g. a drive that is not plugged in).
pub fn show(paths: &[PathBuf]) -> io::Result<()> {
    if let Some(found) = paths.iter().find(|p| p.exists()) {
        return sys::reveal::select(found);
    }
    let first = paths.first().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "No path given"))?;
    match first.ancestors().skip(1).find(|p| p.is_dir()) {
        Some(dir) => sys::reveal::open_folder(dir),
        None => Err(io::Error::new(io::ErrorKind::NotFound, format!("{} is not reachable", first.display()))),
    }
}

/// Opens a web page in the default browser. Only for addresses of Diskman itself.
pub fn open_link(url: &str) -> io::Result<()> {
    if !url.starts_with("https://") {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Not a web address"));
    }
    sys::reveal::open_link(url)
}

/// `rel` (relative, with "/" as in the plans) below `root`, with the separator of the system.
/// "." and ".." are left out: the result always lies below `root`.
pub fn below(root: &Path, rel: &str) -> PathBuf {
    rel.split('/').filter(|s| !s.is_empty() && *s != "." && *s != "..").fold(root.to_path_buf(), |p, s| p.join(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_stay_below_their_root() {
        let root = Path::new("/archive");
        assert_eq!(below(root, "docs/a.txt"), root.join("docs").join("a.txt"));
        assert_eq!(below(root, ""), root);
        assert_eq!(below(root, "../../etc/passwd"), root.join("etc").join("passwd"));
    }

    #[test]
    fn nothing_to_show_is_an_error() {
        // (A path that exists would open the file manager, so only this case is tested here)
        assert!(show(&[]).is_err());
    }
}
