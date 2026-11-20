//! File system limits on Windows, from the volume's file system name and flags.

use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::null_mut;

use windows_sys::Win32::Storage::FileSystem::{GetVolumeInformationW, GetVolumePathNameW};

use crate::platform::volume::VolumeLimits;

const FILE_SUPPORTS_ENCRYPTION: u32 = 0x0002_0000;

/// NTFS: no size limit, encryption, case-insensitive; Windows itself refuses names FAT would refuse.
pub const DEFAULT: VolumeLimits =
    VolumeLimits { max_file_size: None, encryption: true, case_insensitive: true, restricted_names: false };

pub fn limits(path: &Path) -> Option<VolumeLimits> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut root = [0u16; 1024];
    let mut fs_name = [0u16; 64];
    let mut flags = 0u32;
    unsafe {
        if GetVolumePathNameW(wide.as_ptr(), root.as_mut_ptr(), root.len() as u32) == 0 {
            return None;
        }
        if GetVolumeInformationW(
            root.as_ptr(),
            null_mut(),
            0,
            null_mut(),
            null_mut(),
            &mut flags,
            fs_name.as_mut_ptr(),
            fs_name.len() as u32,
        ) == 0
        {
            return None;
        }
    }
    let len = fs_name.iter().position(|&c| c == 0).unwrap_or(fs_name.len());
    let name = String::from_utf16_lossy(&fs_name[..len]).to_ascii_uppercase();
    Some(VolumeLimits {
        max_file_size: matches!(name.as_str(), "FAT" | "FAT32").then_some(u32::MAX as u64),
        encryption: flags & FILE_SUPPORTS_ENCRYPTION != 0,
        ..DEFAULT
    })
}

/// Name of the file system of a volume ("NTFS", "EXFAT" …), upper case.
pub fn fs_name(path: &Path) -> Option<String> {
    let root = volume_root(path)?;
    let mut fs_name = [0u16; 64];
    let ok = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            null_mut(),
            0,
            null_mut(),
            null_mut(),
            null_mut(),
            fs_name.as_mut_ptr(),
            fs_name.len() as u32,
        )
    };
    if ok == 0 {
        return None;
    }
    let len = fs_name.iter().position(|&c| c == 0).unwrap_or(fs_name.len());
    Some(String::from_utf16_lossy(&fs_name[..len]).to_ascii_uppercase())
}

/// Windows keeps folder times on NTFS, but Diskman uses the NTFS change journal there instead
/// (`platform::journal`); on FAT and exFAT they are not kept.
pub const DIR_TIMES_RELIABLE: bool = false;

const DRIVE_REMOTE: u32 = 4;

/// The folder a volume is mounted at (e.g. `C:\`), as a zero-terminated wide string.
fn volume_root(path: &Path) -> Option<Vec<u16>> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut root = [0u16; 1024];
    if unsafe { GetVolumePathNameW(wide.as_ptr(), root.as_mut_ptr(), root.len() as u32) } == 0 {
        return None;
    }
    let len = root.iter().position(|&c| c == 0)?;
    Some(root[..=len].to_vec())
}

/// `\\?\Volume{GUID}\` of the volume (without the terminating zero); None for network shares.
fn volume_name(root: &[u16]) -> Option<Vec<u16>> {
    use windows_sys::Win32::Storage::FileSystem::GetVolumeNameForVolumeMountPointW;
    let mut name = [0u16; 64];
    if unsafe { GetVolumeNameForVolumeMountPointW(root.as_ptr(), name.as_mut_ptr(), name.len() as u32) } == 0 {
        return None;
    }
    let len = name.iter().position(|&c| c == 0)?;
    Some(name[..len].to_vec())
}

pub fn volume_id(path: &Path) -> Option<String> {
    let root = volume_root(path)?;
    let id = volume_name(&root).unwrap_or_else(|| root[..root.len() - 1].to_vec());
    Some(String::from_utf16_lossy(&id).to_lowercase())
}

/// Kind and physical disk of a volume. Network drives by their drive type; otherwise the disk is
/// asked, without administrator rights through a handle without access rights, for its "seek
/// penalty" (true for hard disks), its bus and removable media (`volume::classify`), and its disk
/// number (`\\.\PhysicalDriveN`, shared by all partitions of a disk; none for volumes spanning
/// several disks).
pub fn drive(path: &Path) -> (crate::platform::volume::DriveKind, Option<String>) {
    use crate::platform::volume::{classify, Bus, DriveKind};
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        BusTypeMmc, BusTypeSd, BusTypeUsb, CreateFileW, GetDriveTypeW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Ioctl::{
        PropertyStandardQuery, StorageDeviceProperty, StorageDeviceSeekPenaltyProperty, DEVICE_SEEK_PENALTY_DESCRIPTOR,
        IOCTL_STORAGE_GET_DEVICE_NUMBER, IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_DEVICE_DESCRIPTOR,
        STORAGE_DEVICE_NUMBER, STORAGE_PROPERTY_ID, STORAGE_PROPERTY_QUERY,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;

    /// Sends `code` with `input` and returns how many bytes of `out` were filled, None on failure.
    unsafe fn ioctl<T>(h: HANDLE, code: u32, input: Option<&STORAGE_PROPERTY_QUERY>, out: &mut T) -> Option<usize> {
        let mut got = 0u32;
        let (ptr, len) = match input {
            Some(q) => (q as *const STORAGE_PROPERTY_QUERY as *const _, std::mem::size_of_val(q) as u32),
            None => (std::ptr::null(), 0),
        };
        let ok = DeviceIoControl(
            h,
            code,
            ptr,
            len,
            out as *mut T as *mut _,
            std::mem::size_of::<T>() as u32,
            &mut got,
            null_mut(),
        );
        (ok != 0).then_some(got as usize)
    }
    let query = |id: STORAGE_PROPERTY_ID| STORAGE_PROPERTY_QUERY {
        PropertyId: id,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };

    let Some(root) = volume_root(path) else { return (DriveKind::Unknown, None) };
    if unsafe { GetDriveTypeW(root.as_ptr()) } == DRIVE_REMOTE {
        return (DriveKind::Network, None);
    }
    // The volume as a device: \\?\Volume{GUID} without the trailing backslash
    let Some(mut name) = volume_name(&root) else { return (DriveKind::Unknown, None) };
    if name.last() == Some(&(b'\\' as u16)) {
        name.pop();
    }
    name.push(0);
    unsafe {
        let h =
            CreateFileW(name.as_ptr(), 0, FILE_SHARE_READ | FILE_SHARE_WRITE, null_mut(), OPEN_EXISTING, 0, null_mut());
        if h == INVALID_HANDLE_VALUE {
            return (DriveKind::Unknown, None);
        }
        let mut seek: DEVICE_SEEK_PENALTY_DESCRIPTOR = std::mem::zeroed();
        let seek_penalty =
            ioctl(h, IOCTL_STORAGE_QUERY_PROPERTY, Some(&query(StorageDeviceSeekPenaltyProperty)), &mut seek)
                .map(|_| seek.IncursSeekPenalty);
        // The descriptor is followed by vendor and product strings: room for them, aligned for it
        let mut buf = [0u64; 128];
        let mut bus = Bus::Internal;
        let mut removable = false;
        if let Some(got) = ioctl(h, IOCTL_STORAGE_QUERY_PROPERTY, Some(&query(StorageDeviceProperty)), &mut buf) {
            if got >= std::mem::offset_of!(STORAGE_DEVICE_DESCRIPTOR, RawPropertiesLength) {
                let d = &*(buf.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR);
                removable = d.RemovableMedia;
                bus = if d.BusType == BusTypeUsb {
                    Bus::Usb
                } else if d.BusType == BusTypeSd || d.BusType == BusTypeMmc {
                    Bus::Card
                } else {
                    Bus::Internal
                };
            }
        }
        let mut number: STORAGE_DEVICE_NUMBER = std::mem::zeroed();
        let disk = ioctl(h, IOCTL_STORAGE_GET_DEVICE_NUMBER, None, &mut number)
            .map(|_| format!("disk:{}:{}", number.DeviceType, number.DeviceNumber));
        CloseHandle(h);
        (classify(seek_penalty, bus, removable), disk)
    }
}
