//! File system limits on Linux, from the file system type (`statfs`). Archives on USB drives are
//! often FAT32 or exFAT: case-insensitive, with restricted names, and FAT32 with a 4 GiB limit.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::platform::volume::VolumeLimits;

/// Magic numbers of `statfs.f_type` (linux/magic.h)
const MSDOS_SUPER_MAGIC: u64 = 0x4d44;
const EXFAT_SUPER_MAGIC: u64 = 0x2011_bab0;

/// ext4, btrfs, xfs …: no size limit that matters, case-sensitive, any name without `/`.
pub const DEFAULT: VolumeLimits =
    VolumeLimits { max_file_size: None, encryption: true, case_insensitive: false, restricted_names: false };

pub fn limits(path: &Path) -> Option<VolumeLimits> {
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: statfs only writes into the zeroed struct we pass and reads the C string
    let mut buf: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c_path.as_ptr(), &mut buf) } != 0 {
        return None;
    }
    // The field's integer type differs between architectures
    #[allow(clippy::unnecessary_cast)]
    let kind = buf.f_type as u64;
    Some(match kind {
        MSDOS_SUPER_MAGIC => VolumeLimits {
            max_file_size: Some(u32::MAX as u64),
            case_insensitive: true,
            restricted_names: true,
            ..DEFAULT
        },
        EXFAT_SUPER_MAGIC => VolumeLimits { case_insensitive: true, restricted_names: true, ..DEFAULT },
        _ => DEFAULT,
    })
}

/// ext4, btrfs, xfs … update a folder's time on every change of its entries.
pub const DIR_TIMES_RELIABLE: bool = true;

/// The device number of the file system.
pub fn volume_id(path: &Path) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|m| format!("dev:{}", m.dev()))
}

/// Kind and physical disk of a volume. Network file systems by `statfs`; otherwise the disk of the
/// block device in /sys (for a partition the folder above it): `queue/rotational`, `removable`,
/// attached through USB (in its device path) or a card reader (`mmcblk…`), see `volume::classify`.
/// Devices without a disk in /sys (btrfs subvolumes, …) stay unknown.
pub fn drive(path: &Path) -> (crate::platform::volume::DriveKind, Option<String>) {
    use crate::platform::volume::{classify, Bus, DriveKind};
    use std::os::unix::fs::MetadataExt;
    // NFS, SMB, CIFS, SMB2, FUSE (sshfs, rclone …)
    const NETWORK: [u64; 5] = [0x6969, 0x517B, 0xFF53_4D42, 0xFE53_4D42, 0x6573_5546];
    if let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) {
        // SAFETY: statfs only writes into the zeroed struct we pass and reads the C string
        let mut buf: libc::statfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statfs(c_path.as_ptr(), &mut buf) } == 0 {
            #[allow(clippy::unnecessary_cast)]
            let kind = buf.f_type as u64;
            if NETWORK.contains(&kind) {
                return (DriveKind::Network, None);
            }
        }
    }
    let Ok(meta) = std::fs::metadata(path) else { return (DriveKind::Unknown, None) };
    let dev = meta.dev();
    let major = ((dev >> 8) & 0xfff) | ((dev >> 32) & !0xfff);
    let minor = (dev & 0xff) | ((dev >> 12) & !0xff);
    // The real device folder, e.g. /sys/devices/pci…/usb2/…/block/sdb/sdb1
    let Ok(device) = std::fs::canonicalize(format!("/sys/dev/block/{major}:{minor}")) else {
        return (DriveKind::Unknown, None);
    };
    // A partition has no queue of its own; its disk is the folder above it
    let disk = if device.join("partition").exists() {
        device.parent().map(Path::to_path_buf).unwrap_or(device)
    } else {
        device
    };
    let flag = |file: &str| std::fs::read_to_string(disk.join(file)).ok().map(|v| v.trim() == "1");
    let name = disk.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let bus = if name.starts_with("mmcblk") {
        Bus::Card
    } else if disk.to_string_lossy().contains("/usb") {
        Bus::Usb
    } else {
        Bus::Internal
    };
    let kind = classify(flag("queue/rotational"), bus, flag("removable") == Some(true));
    (kind, (!name.is_empty()).then(|| format!("disk:{name}")))
}
