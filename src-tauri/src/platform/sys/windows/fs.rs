//! File details on Windows: Win32 error codes, file attributes, creation time.

use std::fs::{self, Metadata};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::{FileTimesExt, OpenOptionsExt};
use std::path::Path;

use crate::platform::fs::FsError;

/// Folders Windows keeps at the top of a drive: the recycle bin, the restore points, indexing and
/// shadow copies, and the rollback files of installers. Mostly unreadable, never the user's files.
pub const SYSTEM_DIRS: [&str; 3] = ["$RECYCLE.BIN", "System Volume Information", "Config.Msi"];
/// Files Windows keeps at the top of a drive: the crash dump log (the ".tmp" one is always in
/// use) and, on the system drive, the swap and hibernation files.
pub const SYSTEM_FILES: [&str; 5] =
    ["DumpStack.log", "DumpStack.log.tmp", "pagefile.sys", "hiberfil.sys", "swapfile.sys"];

const FILE_ATTRIBUTE_ENCRYPTED: u32 = 0x4000;
const FILE_WRITE_ATTRIBUTES: u32 = 0x100;

pub fn classify_code(code: i32) -> Option<FsError> {
    Some(match code {
        5 => FsError::AccessDenied,           // ERROR_ACCESS_DENIED
        32 | 33 => FsError::Locked,           // ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION
        39 | 112 => FsError::DiskFull,        // ERROR_HANDLE_DISK_FULL, ERROR_DISK_FULL
        223 => FsError::TooLarge,             // ERROR_FILE_TOO_LARGE
        6000 => FsError::Encrypted,           // ERROR_ENCRYPTION_FAILED
        2 | 3 => FsError::NotFound,           // ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND
        123 | 206 => FsError::NameNotAllowed, // ERROR_INVALID_NAME, ERROR_FILENAME_EXCED_RANGE
        _ => return None,
    })
}

/// `CopyFileExW`: content, attributes and alternate data streams.
pub fn copy_file(from: &Path, to: &Path) -> io::Result<u64> {
    fs::copy(from, to)
}

pub fn clear_readonly(path: &Path) -> io::Result<()> {
    let mut perms = fs::metadata(path)?.permissions();
    if perms.readonly() {
        // On Windows this only clears the read-only attribute (on Unix it would open the file to everyone)
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}

/// `FlushFileBuffers` needs write access, which the read-only attribute (copied along from the
/// source) refuses: it is cleared for the flush and set again.
pub fn sync_file(path: &Path) -> io::Result<()> {
    let readonly = fs::metadata(path)?.permissions().readonly();
    if readonly {
        clear_readonly(path)?;
    }
    let result = match fs::OpenOptions::new().write(true).open(path).and_then(|f| f.sync_all()) {
        // ERROR_INVALID_FUNCTION: the file system cannot flush (some network shares)
        Err(e) if e.raw_os_error() == Some(1) => Ok(()),
        other => other,
    };
    if readonly {
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_readonly(true);
        fs::set_permissions(path, perms)?;
    }
    result
}

/// NTFS journals renames and new names; there is nothing to flush for a folder.
pub fn sync_dir(_path: &Path) {}

pub fn copy_times(source: &Metadata, path: &Path) -> io::Result<()> {
    let mut times = fs::FileTimes::new();
    if let Ok(m) = source.modified() {
        times = times.set_modified(m);
    }
    if let Ok(c) = source.created() {
        times = times.set_created(c);
    }
    // Only request attribute write access; this also works for read-only files
    fs::OpenOptions::new().access_mode(FILE_WRITE_ATTRIBUTES).open(path)?.set_times(times)
}

/// 100 ns intervals between 1601-01-01 (FILETIME) and 1970-01-01.
const UNIX_EPOCH_FILETIME: i64 = 116_444_736_000_000_000;
const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
const IO_REPARSE_TAG_SYMLINK: u32 = 0xA000_000C;

/// FILETIME as ms since the Unix epoch, rounded like `platform::fs::millis` of the same time.
fn filetime_ms(t: i64) -> i64 {
    if t >= UNIX_EPOCH_FILETIME {
        (t - UNIX_EPOCH_FILETIME) / 10_000
    } else {
        -((UNIX_EPOCH_FILETIME - t) / 10_000)
    }
}

/// The path in the `\\?\` form: no length limit, no rewriting by Windows.
pub(super) fn verbatim(path: &Path) -> Vec<u16> {
    const BACKSLASH: u16 = b'\\' as u16;
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut s: Vec<u16> = abs.as_os_str().encode_wide().map(|c| if c == b'/' as u16 { BACKSLASH } else { c }).collect();
    let prefix: Vec<u16> = r"\\?\".encode_utf16().collect();
    if !s.starts_with(&prefix) {
        if s.starts_with(&[BACKSLASH, BACKSLASH]) {
            // \\server\share → \\?\UNC\server\share
            let mut v: Vec<u16> = r"\\?\UNC".encode_utf16().collect();
            v.extend_from_slice(&s[1..]);
            s = v;
        } else {
            let mut v = prefix;
            v.extend_from_slice(&s);
            s = v;
        }
    }
    s.push(0);
    s
}

/// An open handle, closed when dropped.
pub(super) struct Handle(pub windows_sys::Win32::Foundation::HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

/// Opens a folder (also with access 0 or only attribute access) without following a link at the
/// end; `FILE_FLAG_BACKUP_SEMANTICS` is what lets CreateFileW open folders at all.
pub(super) fn open_dir(path: &Path, access: u32) -> io::Result<Handle> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    let wide = verbatim(path);
    let h = unsafe {
        CreateFileW(
            wide.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            core::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            core::ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(Handle(h))
}

pub fn list_dir(path: &Path) -> io::Result<Vec<crate::platform::fs::Listed>> {
    use crate::platform::fs::{EntryKind, EntryMeta, Listed};
    use windows_sys::Win32::Foundation::ERROR_NO_MORE_FILES;
    use windows_sys::Win32::Storage::FileSystem::{
        FileIdBothDirectoryInfo, GetFileInformationByHandleEx, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_ID_BOTH_DIR_INFO, FILE_LIST_DIRECTORY,
    };

    let dir = open_dir(path, FILE_LIST_DIRECTORY)?;
    // 64 KiB, 8-byte aligned: many entries per call
    let mut buf = vec![0u64; 8 * 1024];
    let mut out = Vec::new();
    loop {
        let ok = unsafe {
            GetFileInformationByHandleEx(
                dir.0,
                FileIdBothDirectoryInfo,
                buf.as_mut_ptr().cast(),
                (buf.len() * 8) as u32,
            )
        };
        if ok == 0 {
            let e = io::Error::last_os_error();
            if e.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
                break;
            }
            return Err(e);
        }
        let base = buf.as_ptr() as *const u8;
        let mut offset = 0usize;
        loop {
            // SAFETY: Windows filled the buffer with a chain of FILE_ID_BOTH_DIR_INFO records;
            // NextEntryOffset leads from one to the next and is 0 for the last
            let info = unsafe { &*(base.add(offset) as *const FILE_ID_BOTH_DIR_INFO) };
            let name = unsafe { std::slice::from_raw_parts(info.FileName.as_ptr(), info.FileNameLength as usize / 2) };
            let dot = name == [b'.' as u16] || name == [b'.' as u16, b'.' as u16];
            if !dot {
                let attrs = info.FileAttributes;
                // For reparse points EaSize holds the reparse tag
                let link = attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0
                    && matches!(info.EaSize, IO_REPARSE_TAG_SYMLINK | IO_REPARSE_TAG_MOUNT_POINT);
                let kind = if link {
                    EntryKind::Link
                } else if attrs & FILE_ATTRIBUTE_DIRECTORY != 0 {
                    EntryKind::Dir
                } else {
                    EntryKind::File
                };
                out.push(Listed {
                    name: String::from_utf16(name).map_err(|_| String::from_utf16_lossy(name)),
                    kind,
                    meta: Ok(EntryMeta {
                        size: info.EndOfFile.max(0) as u64,
                        mtime: filetime_ms(info.LastWriteTime),
                        ctime: Some(filetime_ms(info.CreationTime)),
                        encrypted: attrs & FILE_ATTRIBUTE_ENCRYPTED != 0,
                        id: info.FileId as u64,
                    }),
                });
            }
            if info.NextEntryOffset == 0 {
                break;
            }
            offset += info.NextEntryOffset as usize;
        }
    }
    Ok(out)
}

pub fn dir_meta(path: &Path) -> io::Result<crate::platform::fs::EntryMeta> {
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_READ_ATTRIBUTES,
    };
    let dir = open_dir(path, FILE_READ_ATTRIBUTES)?;
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(dir.0, &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let ft = |t: windows_sys::Win32::Foundation::FILETIME| ((t.dwHighDateTime as i64) << 32) | t.dwLowDateTime as i64;
    Ok(crate::platform::fs::EntryMeta {
        size: 0,
        mtime: filetime_ms(ft(info.ftLastWriteTime)),
        ctime: Some(filetime_ms(ft(info.ftCreationTime))),
        encrypted: info.dwFileAttributes & FILE_ATTRIBUTE_ENCRYPTED != 0,
        id: ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
    })
}
