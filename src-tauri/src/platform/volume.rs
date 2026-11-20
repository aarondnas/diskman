//! Limits of the file system an archive lives on, so the analysis can name files it cannot hold
//! before a run tries to copy them, and so paths are compared the way that file system does.

use std::path::Path;

use super::sys;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeLimits {
    /// Largest file the file system can store (FAT32: 4 GiB − 1 byte); `None` = no relevant limit
    pub max_file_size: Option<u64>,
    /// Can store encrypted files (Windows EFS on NTFS); otherwise copying them fails
    pub encryption: bool,
    /// Treats names that differ only in case as the same file (NTFS on Windows, FAT, exFAT)
    pub case_insensitive: bool,
    /// Only allows names as FAT and exFAT do (see `fat_name_allowed`). Relevant on Linux, where
    /// file names may contain characters those file systems refuse.
    pub restricted_names: bool,
}

impl VolumeLimits {
    /// Whether anything about a file can make it untransferable to this file system.
    pub fn restricts_files(&self) -> bool {
        self.max_file_size.is_some() || !self.encryption || self.restricted_names
    }
}

impl Default for VolumeLimits {
    /// The usual file system of the system (NTFS, ext4 …), also when it cannot be determined:
    /// then a run reports what fails.
    fn default() -> Self {
        sys::volume::DEFAULT
    }
}

/// Limits for an archive folder. It may not exist yet: then its nearest existing parent counts.
pub fn limits(path: &Path) -> VolumeLimits {
    let existing = path.ancestors().find(|p| p.exists());
    existing.and_then(sys::volume::limits).unwrap_or_default()
}

/// Identity of the volume a path lives on (its nearest existing parent counts). What competes for
/// a drive is told by `Drive::disk`, which falls back to this.
pub fn volume_id(path: &Path) -> String {
    let existing = path.ancestors().find(|p| p.exists()).unwrap_or(path);
    sys::volume::volume_id(existing).unwrap_or_else(|| existing.to_string_lossy().into_owned())
}

/// What kind of drive a volume is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveKind {
    /// Hard disk, also in a USB case: every jump of its heads costs milliseconds
    Spinning,
    /// SSD (SATA, NVMe, also in a USB case)
    Solid,
    /// USB stick, SD or other memory card: no heads, but a simple controller that gains little
    /// from many reads at once
    Flash,
    Network,
    Unknown,
}

/// How a disk is attached, as far as it matters for telling its kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bus {
    Internal,
    Usb,
    /// SD or MMC card reader
    Card,
}

/// The kind of drive from what the system reports about it: whether it has to seek (None:
/// no answer), how it is attached, and whether its medium is removable.
/// USB sticks and card readers report removable media (and often a seek penalty all the same);
/// hard disks behind USB adapters often do not answer about seeking, so an unknown USB disk counts
/// as a hard disk: reading it one thing at a time costs an SSD little, the other way round a hard
/// disk a lot.
pub fn classify(seek_penalty: Option<bool>, bus: Bus, removable: bool) -> DriveKind {
    match (bus, removable, seek_penalty) {
        (Bus::Card, _, _) | (Bus::Usb, true, _) => DriveKind::Flash,
        (_, _, Some(true)) => DriveKind::Spinning,
        (_, _, Some(false)) => DriveKind::Solid,
        (Bus::Usb, false, None) => DriveKind::Spinning,
        (Bus::Internal, _, None) => DriveKind::Unknown,
    }
}

/// The drive a path lives on (its nearest existing parent counts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drive {
    pub kind: DriveKind,
    /// The physical disk: partitions of one disk share it. The volume if that cannot be told
    /// (network shares, volumes spanning several disks).
    pub disk: String,
}

impl Drive {
    /// How many folders or files are read from it at once (scans, content samples, opening
    /// files): one on a hard disk (parallel reads would make its heads jump back and forth), two
    /// on memory sticks and cards and on drives of unknown kind, several on SSDs and network
    /// drives, where waiting dominates.
    pub fn read_threads(&self) -> usize {
        let parallel = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(2, 8);
        match self.kind {
            DriveKind::Spinning => 1,
            DriveKind::Flash | DriveKind::Unknown => 2,
            DriveKind::Solid | DriveKind::Network => parallel,
        }
    }
}

pub fn drive(path: &Path) -> Drive {
    let existing = path.ancestors().find(|p| p.exists()).unwrap_or(path);
    let (kind, disk) = sys::volume::drive(existing);
    Drive { kind, disk: disk.unwrap_or_else(|| volume_id(existing)) }
}

/// How many folders a scan of this folder reads at once (`Drive::read_threads`).
pub fn scan_threads(path: &Path) -> usize {
    drive(path).read_threads()
}

/// Whether the modification time of a folder reliably changes when an entry is added, removed or
/// renamed in it. Not on FAT and exFAT (Windows does not keep it there, and a USB drive changes
/// hands between systems); Windows uses the NTFS change journal instead (`platform::journal`).
pub fn dir_times_reliable(path: &Path) -> bool {
    sys::volume::DIR_TIMES_RELIABLE && !limits(path).restricted_names
}

/// Whether FAT and exFAT accept this file or folder name: none of `" * / : < > ? \ |`, no control
/// characters, and no dot or space at the end (they would be dropped, so two names could collide).
pub fn fat_name_allowed(name: &str) -> bool {
    const FORBIDDEN: &str = "\"*/:<>?\\|";
    !name.is_empty() && !name.chars().any(|c| c < ' ' || FORBIDDEN.contains(c)) && !name.ends_with(['.', ' '])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fat_names() {
        assert!(fat_name_allowed("Report 2026.pdf"));
        assert!(fat_name_allowed(".hidden"));
        assert!(!fat_name_allowed("12:30 meeting.txt"));
        assert!(!fat_name_allowed("what?"));
        assert!(!fat_name_allowed("trailing."));
        assert!(!fat_name_allowed("tab\there"));
    }

    #[test]
    fn drive_kinds() {
        use Bus::*;
        use DriveKind::*;
        // Internal disks answer about seeking
        assert_eq!(classify(Some(true), Internal, false), Spinning);
        assert_eq!(classify(Some(false), Internal, false), Solid);
        assert_eq!(classify(None, Internal, false), Unknown);
        // USB sticks and memory cards, whatever they say about seeking
        assert_eq!(classify(Some(true), Usb, true), Flash);
        assert_eq!(classify(None, Usb, true), Flash);
        assert_eq!(classify(Some(false), Card, false), Flash);
        // USB disks: SSD or hard disk as they answer; without an answer a hard disk
        assert_eq!(classify(Some(false), Usb, false), Solid);
        assert_eq!(classify(Some(true), Usb, false), Spinning);
        assert_eq!(classify(None, Usb, false), Spinning);
    }

    #[test]
    fn read_threads_follow_the_drive() {
        let threads = |kind| Drive { kind, disk: String::new() }.read_threads();
        assert_eq!(threads(DriveKind::Spinning), 1);
        assert_eq!(threads(DriveKind::Flash), 2);
        assert_eq!(threads(DriveKind::Unknown), 2);
        assert!(threads(DriveKind::Solid) >= 2);
        assert!(threads(DriveKind::Network) >= 2);
    }
}
