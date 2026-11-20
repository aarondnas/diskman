//! The change journal of a volume: which folders had something change in them since a point in
//! time, without reading the folders. Windows: the NTFS USN journal, read without administrator
//! rights (`FSCTL_READ_UNPRIVILEGED_USN_JOURNAL`, through a handle to a folder on the volume).
//! Linux has nothing comparable; there is no journal and scans read everything (or rely on folder
//! times for archives, see `sync::tree`).
//!
//! Every change of a file or folder (created, deleted, renamed, written, attributes) is recorded
//! with the id of the folder it is in. A folder whose id shows up must be read again; all others
//! are as they were.

use std::collections::HashSet;
use std::path::Path;

use super::sys;

/// A point in the journal of a volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// Identity of the journal; a new one (deleted and created again) does not continue the old
    pub journal: u64,
    /// The next record that will be written
    pub usn: i64,
}

/// The current position of the journal of the volume `root` lives on; None without a journal
/// (other file systems, Linux) or when it cannot be read.
pub fn position(root: &Path) -> Option<Position> {
    sys::journal::position(root)
}

/// Ids of the folders (`platform::fs::EntryMeta::id`) in which something changed between `since`
/// and now. None when that cannot be said for sure: the journal was replaced, or it no longer
/// reaches back to `since` (it is a ring of limited size). Then everything has to be read.
pub fn changed_folders(root: &Path, since: Position) -> Option<HashSet<u64>> {
    sys::journal::changed_folders(root, since)
}
