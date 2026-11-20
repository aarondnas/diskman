//! The NTFS USN journal (see `platform::journal`).
//!
//! Without administrator rights a handle to the volume is refused, but a handle to any folder on
//! it works with `FSCTL_QUERY_USN_JOURNAL` and `FSCTL_READ_UNPRIVILEGED_USN_JOURNAL`, the latter
//! with the `_V1` input that names the record versions. Only NTFS: its file ids are the 64-bit
//! numbers that folder listings report (ReFS uses 128-bit ids).

use std::collections::HashSet;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES;
use windows_sys::Win32::System::Ioctl::{
    FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_UNPRIVILEGED_USN_JOURNAL, READ_USN_JOURNAL_DATA_V1, USN_JOURNAL_DATA_V0,
};
use windows_sys::Win32::System::IO::DeviceIoControl;

use super::fs::{open_dir, Handle};
use super::volume::fs_name;
use crate::platform::journal::Position;

fn query(dir: &Handle) -> Option<USN_JOURNAL_DATA_V0> {
    let mut data: USN_JOURNAL_DATA_V0 = unsafe { std::mem::zeroed() };
    let mut got = 0u32;
    let ok = unsafe {
        DeviceIoControl(
            dir.0,
            FSCTL_QUERY_USN_JOURNAL,
            core::ptr::null(),
            0,
            &mut data as *mut _ as *mut _,
            std::mem::size_of_val(&data) as u32,
            &mut got,
            core::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(data)
}

fn open(root: &Path) -> Option<Handle> {
    if fs_name(root).as_deref() != Some("NTFS") {
        return None;
    }
    open_dir(root, FILE_READ_ATTRIBUTES).ok()
}

pub fn position(root: &Path) -> Option<Position> {
    let dir = open(root)?;
    let data = query(&dir)?;
    Some(Position { journal: data.UsnJournalID, usn: data.NextUsn })
}

pub fn changed_folders(root: &Path, since: Position) -> Option<HashSet<u64>> {
    let dir = open(root)?;
    let data = query(&dir)?;
    // A new journal, or records from `since` on were already overwritten
    if data.UsnJournalID != since.journal || since.usn < data.FirstUsn || since.usn < data.LowestValidUsn {
        return None;
    }
    let until = data.NextUsn;
    let mut folders = HashSet::new();
    let mut start = since.usn;
    // 8-byte aligned buffer: the next USN, then the records
    let mut buf = vec![0u64; 64 * 1024 / 8];
    while start < until {
        let input = READ_USN_JOURNAL_DATA_V1 {
            StartUsn: start,
            ReasonMask: u32::MAX,
            ReturnOnlyOnClose: 0,
            Timeout: 0,
            BytesToWaitFor: 0,
            UsnJournalID: since.journal,
            MinMajorVersion: 2,
            MaxMajorVersion: 2,
        };
        let mut got = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                dir.0,
                FSCTL_READ_UNPRIVILEGED_USN_JOURNAL,
                &input as *const _ as *const _,
                std::mem::size_of_val(&input) as u32,
                buf.as_mut_ptr().cast(),
                (buf.len() * 8) as u32,
                &mut got,
                core::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, got as usize) };
        if bytes.len() < 8 {
            return None;
        }
        let next = i64::from_le_bytes(bytes[..8].try_into().ok()?);
        let mut off = 8;
        // USN_RECORD_V2: RecordLength u32, MajorVersion u16, MinorVersion u16,
        // FileReferenceNumber u64 (at 8), ParentFileReferenceNumber u64 (at 16), …
        while off + 24 <= bytes.len() {
            let len = u32::from_le_bytes(bytes[off..off + 4].try_into().ok()?) as usize;
            if len == 0 || off + len > bytes.len() {
                break;
            }
            let major = u16::from_le_bytes(bytes[off + 4..off + 6].try_into().ok()?);
            if major != 2 {
                return None;
            }
            let parent = u64::from_le_bytes(bytes[off + 16..off + 24].try_into().ok()?);
            folders.insert(parent);
            off += len;
        }
        if next <= start {
            break; // nothing more to read
        }
        start = next;
    }
    Some(folders)
}
