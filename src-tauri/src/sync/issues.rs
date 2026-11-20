//! What cannot be transferred and why. The analysis lists these entries before a run ("not
//! transferable"); a run stores the same kind of entry for every operation that failed.
//! The reasons are codes, so the UI can explain them in its own language.

use std::io;

use serde::{Deserialize, Serialize};

use crate::platform::{elevation, fs::FsError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// Symbolic link or junction: skipped on purpose, neither the link nor its target is copied
    Link,
    /// Named pipe, socket or device file (Linux): skipped on purpose, it holds no data to copy
    Special,
    /// No read permission (Windows: often readable with administrator rights)
    AccessDenied,
    /// Opened exclusively by another program (e.g. Outlook, a running VM)
    Locked,
    /// The name cannot be represented (not valid Unicode)
    InvalidName,
    /// The archive's file system does not allow the name (FAT, exFAT: e.g. `:` or `?`)
    NameNotAllowed,
    /// A second entry whose path differs only in upper and lower case (case-sensitive folder)
    CaseConflict,
    /// Larger than the archive's file system allows (4 GiB on FAT32)
    TooLarge,
    /// EFS-encrypted, but the archive's file system cannot store encrypted files
    Encrypted,
    /// The archive drive is full
    DiskFull,
    /// Disappeared between analysis and copying
    Vanished,
    /// Any other error; the message says more
    Other,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::Link => "link",
            Reason::Special => "special",
            Reason::AccessDenied => "access_denied",
            Reason::Locked => "locked",
            Reason::InvalidName => "invalid_name",
            Reason::NameNotAllowed => "name_not_allowed",
            Reason::CaseConflict => "case_conflict",
            Reason::TooLarge => "too_large",
            Reason::Encrypted => "encrypted",
            Reason::DiskFull => "disk_full",
            Reason::Vanished => "vanished",
            Reason::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "link" => Reason::Link,
            "special" => Reason::Special,
            "access_denied" => Reason::AccessDenied,
            "locked" => Reason::Locked,
            "invalid_name" => Reason::InvalidName,
            "name_not_allowed" => Reason::NameNotAllowed,
            "case_conflict" => Reason::CaseConflict,
            "too_large" => Reason::TooLarge,
            "encrypted" => Reason::Encrypted,
            "disk_full" => Reason::DiskFull,
            "vanished" => Reason::Vanished,
            _ => Reason::Other,
        }
    }

    /// Classifies an I/O error of reading or copying: by the OS error code first (more precise),
    /// otherwise by the error kind.
    pub fn of(e: &io::Error) -> Self {
        if let Some(fs_error) = crate::platform::fs::classify(e) {
            return match fs_error {
                FsError::AccessDenied => Reason::AccessDenied,
                FsError::Locked => Reason::Locked,
                FsError::DiskFull => Reason::DiskFull,
                FsError::TooLarge => Reason::TooLarge,
                FsError::Encrypted => Reason::Encrypted,
                FsError::NotFound => Reason::Vanished,
                FsError::NameNotAllowed => Reason::NameNotAllowed,
            };
        }
        match e.kind() {
            io::ErrorKind::PermissionDenied => Reason::AccessDenied,
            io::ErrorKind::NotFound => Reason::Vanished,
            io::ErrorKind::StorageFull => Reason::DiskFull,
            io::ErrorKind::FileTooLarge => Reason::TooLarge,
            _ => Reason::Other,
        }
    }

    /// Restarting with administrator rights may solve it (Windows, while Diskman does not have them).
    pub fn admin_may_help(self) -> bool {
        self == Reason::AccessDenied && elevation::can_elevate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Object {
    File,
    Folder,
    Link,
    /// Named pipe, socket or device file
    Special,
}

impl Object {
    pub fn as_str(self) -> &'static str {
        match self {
            Object::File => "file",
            Object::Folder => "folder",
            Object::Link => "link",
            Object::Special => "special",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "folder" => Object::Folder,
            "link" => Object::Link,
            "special" => Object::Special,
            _ => Object::File,
        }
    }
}

/// One entry that is not (or was not) transferred.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Issue {
    /// Path relative to the source, with `/` ("" = the source folder itself)
    pub path: String,
    pub object: Object,
    pub reason: Reason,
    /// Details: the system's error message, or the other path of a case conflict
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub size: u64,
    /// Administrator rights may solve it
    pub admin: bool,
}

impl Issue {
    pub fn new(path: impl Into<String>, object: Object, reason: Reason) -> Self {
        Issue { path: path.into(), object, reason, detail: None, size: 0, admin: reason.admin_may_help() }
    }

    pub fn from_io(path: impl Into<String>, object: Object, e: &io::Error) -> Self {
        Issue { detail: Some(e.to_string()), ..Issue::new(path, object, Reason::of(e)) }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_size(mut self, size: u64) -> Self {
        self.size = size;
        self
    }

    /// Links and special files are skipped on purpose; everything else means data is missing in the
    /// archive.
    pub fn is_failure(&self) -> bool {
        !matches!(self.reason, Reason::Link | Reason::Special)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_error_kind() {
        assert_eq!(Reason::of(&io::Error::from(io::ErrorKind::PermissionDenied)), Reason::AccessDenied);
        assert_eq!(Reason::of(&io::Error::from(io::ErrorKind::NotFound)), Reason::Vanished);
        assert_eq!(Reason::of(&io::Error::other("x")), Reason::Other);
    }

    #[test]
    #[cfg(windows)]
    fn classifies_windows_errors() {
        assert_eq!(Reason::of(&io::Error::from_raw_os_error(32)), Reason::Locked);
        assert_eq!(Reason::of(&io::Error::from_raw_os_error(112)), Reason::DiskFull);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn classifies_linux_errors() {
        assert_eq!(Reason::of(&io::Error::from_raw_os_error(libc::EACCES)), Reason::AccessDenied);
        assert_eq!(Reason::of(&io::Error::from_raw_os_error(libc::ENOSPC)), Reason::DiskFull);
        assert_eq!(Reason::of(&io::Error::from_raw_os_error(libc::ETXTBSY)), Reason::Locked);
    }

    #[test]
    fn reason_codes_round_trip() {
        for r in [
            Reason::Link,
            Reason::Special,
            Reason::AccessDenied,
            Reason::Locked,
            Reason::NameNotAllowed,
            Reason::CaseConflict,
            Reason::Other,
        ] {
            assert_eq!(Reason::parse(r.as_str()), r);
        }
    }
}
