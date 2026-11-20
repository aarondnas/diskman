//! File details on Linux: errno values, timestamps. There is no file system encryption flag and no
//! read-only attribute; permissions live in the mode bits and are copied along with the file.

use std::fs::{self, Metadata};
use std::io;
use std::path::Path;

use crate::platform::fs::FsError;

/// Folder ext file systems keep at their top for what a file system check recovers; only root
/// can read it, and it holds none of the user's files.
pub const SYSTEM_DIRS: [&str; 1] = ["lost+found"];
/// Linux keeps no files of its own at the top of a drive.
pub const SYSTEM_FILES: [&str; 0] = [];

pub fn classify_code(code: i32) -> Option<FsError> {
    Some(match code {
        libc::EACCES | libc::EPERM => FsError::AccessDenied,
        libc::EBUSY | libc::ETXTBSY => FsError::Locked,
        libc::ENOSPC | libc::EDQUOT => FsError::DiskFull,
        libc::EFBIG => FsError::TooLarge,
        libc::ENOENT => FsError::NotFound,
        libc::ENAMETOOLONG => FsError::NameNotAllowed,
        _ => return None,
    })
}

/// `std::fs::copy` also carries the permission bits over. File systems without Unix permissions
/// (FAT, exFAT on a USB drive) refuse that with `EPERM` unless mounted with `quiet`: then only the
/// content is copied, the drive decides the permissions.
pub fn copy_file(from: &Path, to: &Path) -> io::Result<u64> {
    match fs::copy(from, to) {
        Err(e) if e.raw_os_error() == Some(libc::EPERM) => {
            let mut reader = fs::File::open(from)?;
            let mut writer = fs::File::create(to)?;
            io::copy(&mut reader, &mut writer)
        }
        other => other,
    }
}

/// `fsync` works on a file opened for reading, so read-only copies need no detour.
pub fn sync_file(path: &Path) -> io::Result<()> {
    match fs::File::open(path)?.sync_all() {
        // The file system cannot flush (some FUSE and network file systems)
        Err(e) if matches!(e.raw_os_error(), Some(libc::EINVAL | libc::ENOTSUP | libc::EROFS)) => Ok(()),
        other => other,
    }
}

pub fn sync_dir(path: &Path) {
    if let Ok(dir) = fs::File::open(path) {
        let _ = dir.sync_all();
    }
}

pub fn clear_readonly(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// Linux cannot set the creation time (birth time); the modification time is what the comparison uses.
/// Opened read-only: the copy may carry the source's read-only mode, and `futimens` only needs
/// ownership, not write access.
pub fn copy_times(source: &Metadata, path: &Path) -> io::Result<()> {
    let mut times = fs::FileTimes::new();
    if let Ok(m) = source.modified() {
        times = times.set_modified(m);
    }
    fs::File::open(path)?.set_times(times)
}

pub fn list_dir(path: &Path) -> io::Result<Vec<crate::platform::fs::Listed>> {
    use crate::platform::fs::{EntryKind, Listed};
    use std::os::unix::fs::DirEntryExt;
    let mut out = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name().into_string().map_err(|n| n.to_string_lossy().into_owned());
        // lstat: links are reported, not followed
        let meta = fs::symlink_metadata(entry.path());
        let kind_of = |ft: fs::FileType| {
            if ft.is_symlink() {
                EntryKind::Link
            } else if ft.is_dir() {
                EntryKind::Dir
            } else if ft.is_file() {
                EntryKind::File
            } else {
                EntryKind::Special
            }
        };
        let kind = match &meta {
            Ok(m) => kind_of(m.file_type()),
            Err(_) => entry.file_type().map(kind_of).unwrap_or(EntryKind::File),
        };
        let ino = entry.ino();
        out.push(Listed { name, kind, meta: meta.map(|m| entry_meta(&m, ino)) });
    }
    Ok(out)
}

pub fn dir_meta(path: &Path) -> io::Result<crate::platform::fs::EntryMeta> {
    use std::os::unix::fs::MetadataExt;
    let m = fs::metadata(path)?;
    let ino = m.ino();
    Ok(entry_meta(&m, ino))
}

fn entry_meta(m: &Metadata, id: u64) -> crate::platform::fs::EntryMeta {
    use crate::platform::fs::millis;
    crate::platform::fs::EntryMeta {
        size: m.len(),
        mtime: m.modified().map(millis).unwrap_or(0),
        ctime: m.created().ok().map(millis),
        encrypted: false,
        id,
    }
}
