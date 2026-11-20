//! File details that differ between the systems: what an OS error code means, file attributes,
//! timestamps.

use std::fs::Metadata;
use std::io;
use std::path::Path;

use super::sys;

/// Meaning of an OS error code, as far as the sync engine cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    AccessDenied,
    /// Opened exclusively by another program (Windows) or busy (Linux)
    Locked,
    DiskFull,
    TooLarge,
    /// Encrypted file on a file system without encryption (Windows EFS)
    #[cfg_attr(not(windows), allow(dead_code))]
    Encrypted,
    NotFound,
    /// The name is too long or not allowed by the file system
    NameNotAllowed,
}

/// Classifies an I/O error by its OS error code; `None` for codes without special meaning.
pub fn classify(e: &io::Error) -> Option<FsError> {
    e.raw_os_error().and_then(sys::fs::classify_code)
}

/// Copies the content of a file (and what the system carries along: attributes, permissions,
/// alternate data streams on Windows). Returns the number of bytes copied.
pub fn copy_file(from: &Path, to: &Path) -> io::Result<u64> {
    sys::fs::copy_file(from, to)
}

/// Makes a file writable again so it can be replaced or deleted. Windows: clears the read-only
/// attribute. Linux: nothing to do, replacing and deleting depend on the folder's permissions.
pub fn clear_readonly(path: &Path) -> io::Result<()> {
    sys::fs::clear_readonly(path)
}

/// Carries the timestamps of the source over to the copy at `path`, so the next comparison
/// recognizes it (modification time everywhere, creation time where the system can set it).
pub fn copy_times(source: &Metadata, path: &Path) -> io::Result<()> {
    sys::fs::copy_times(source, path)
}

/// Forces the content of a file onto the drive. Without it the system may write the rename of a
/// copy before its data: after a power cut or an unplugged drive the archive would hold a file with
/// the right name, size and time but empty content, and the next comparison would not notice.
/// File systems that cannot flush (some network shares) count as done.
pub fn sync_file(path: &Path) -> io::Result<()> {
    sys::fs::sync_file(path)
}

/// Makes renames and new names in a folder durable. Linux: `fsync` of the folder. Windows: nothing
/// to do, NTFS journals its names (FAT has no journal either way). Best effort: errors are ignored.
pub fn sync_dir(path: &Path) {
    sys::fs::sync_dir(path)
}

/// Removes a folder with everything in it, read-only files included (Windows refuses to delete
/// those otherwise). A folder that is not there counts as removed.
pub fn remove_tree(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    for entry in walkdir::WalkDir::new(path).follow_links(false).contents_first(true) {
        let entry = entry.map_err(io::Error::other)?;
        let p = entry.path();
        if entry.file_type().is_dir() {
            std::fs::remove_dir(p)?;
        } else {
            clear_readonly(p)?;
            std::fs::remove_file(p)?;
        }
    }
    Ok(())
}

/// Kind of an entry in a folder listing. Links (symbolic links, junctions) are reported as links,
/// never followed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    Link,
    /// Named pipe, socket or device (Linux)
    #[cfg_attr(windows, allow(dead_code))]
    Special,
}

/// What a listing knows about an entry.
#[derive(Debug, Clone, Copy)]
pub struct EntryMeta {
    pub size: u64,
    /// Modification time in ms since the Unix epoch
    pub mtime: i64,
    /// Creation time in ms (not available on all file systems)
    pub ctime: Option<i64>,
    /// Encrypted by the file system (Windows EFS)
    pub encrypted: bool,
    /// Identity on its volume: the file reference number (NTFS) or inode (Linux). Tells a folder
    /// that was replaced or renamed apart from the one that was there before.
    pub id: u64,
}

/// One entry of a folder listing.
#[derive(Debug)]
pub struct Listed {
    /// The name, or the name made readable when it is not valid Unicode (`Err`)
    pub name: Result<String, String>,
    pub kind: EntryKind,
    /// Details; on Linux they are read per entry and may fail on their own
    pub meta: io::Result<EntryMeta>,
}

/// The entries of one folder (without `.` and `..`). Windows reads them in large blocks together
/// with their details (`GetFileInformationByHandleEx`), one system call for many entries; Linux
/// needs one `lstat` per entry.
pub fn list_dir(path: &Path) -> io::Result<Vec<Listed>> {
    sys::fs::list_dir(path)
}

/// A folder the system keeps at the top of a drive (Windows: the recycle bin, "System Volume
/// Information" and the rollback files of installers; Linux: lost+found). Not part of a backup:
/// neither copied from a source nor deleted from an archive.
pub fn is_system_dir(name: &str) -> bool {
    sys::fs::SYSTEM_DIRS.iter().any(|d| d.eq_ignore_ascii_case(name))
}

/// A file the system keeps at the top of a drive (Windows: the crash dump log, the swap and
/// hibernation files). Not part of a backup either.
pub fn is_system_file(name: &str) -> bool {
    sys::fs::SYSTEM_FILES.iter().any(|f| f.eq_ignore_ascii_case(name))
}

/// Identity and modification time of a folder itself (links followed).
pub fn dir_meta(path: &Path) -> io::Result<EntryMeta> {
    sys::fs::dir_meta(path)
}

/// A point in time as ms since the Unix epoch (negative before 1970).
pub fn millis(t: std::time::SystemTime) -> i64 {
    match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(e) => -(e.duration().as_millis() as i64),
    }
}
