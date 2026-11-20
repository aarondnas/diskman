//! Autostart on Windows: the value "Diskman" in the user's Run key (HKCU). The NSIS installer writes
//! the same value when the user agrees during installation, and marks that it asked
//! (src-tauri/windows/installer-hooks.nsh).

use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD, REG_SZ, RRF_RT_REG_BINARY,
    RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
};

use super::wide;
use crate::platform::autostart::BACKGROUND_ARG;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Where Windows notes that the user switched an entry off (Task Manager, Settings → Startup apps)
const APPROVED_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const VALUE: &str = "Diskman";
/// Written once the user answered, by the installer or here, so neither asks again
const APP_KEY: &str = r"Software\Diskman";
const ASKED_VALUE: &str = "AutostartAsked";

pub fn is_enabled() -> bool {
    if read(RUN_KEY, VALUE, RRF_RT_REG_SZ).is_none_or(|data| data.len() <= 2) {
        return false;
    }
    // The first byte is odd (0x03, 0x07) when the user switched the entry off in Windows
    !read(APPROVED_KEY, VALUE, RRF_RT_REG_BINARY).is_some_and(|data| data.first().is_some_and(|b| b & 1 == 1))
}

pub fn set_enabled(on: bool, program: &Path) -> anyhow::Result<()> {
    mark_asked();
    if on {
        let command = format!("\"{}\" {BACKGROUND_ARG}", program.display());
        let data: Vec<u8> = wide(&command).iter().flat_map(|c| c.to_le_bytes()).collect();
        let key = wide(RUN_KEY);
        let name = wide(VALUE);
        let err = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                REG_SZ,
                data.as_ptr().cast(),
                data.len() as u32,
            )
        };
        if err != ERROR_SUCCESS {
            anyhow::bail!("Could not add Diskman to the startup apps (error {err}).");
        }
        // Otherwise an earlier switch-off in the Task Manager would still win
        delete(APPROVED_KEY, VALUE)
    } else {
        delete(RUN_KEY, VALUE)
    }
}

pub fn asked_by_installer() -> bool {
    read(APP_KEY, ASKED_VALUE, RRF_RT_REG_DWORD).is_some_and(|data| data.first() == Some(&1))
}

pub fn program() -> anyhow::Result<PathBuf> {
    Ok(std::env::current_exe()?)
}

/// Notes for the installer that the user answered (a failure only means it may ask once more).
fn mark_asked() {
    let key = wide(APP_KEY);
    let name = wide(ASKED_VALUE);
    let one = 1u32.to_le_bytes();
    unsafe {
        RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), REG_DWORD, one.as_ptr().cast(), 4);
    }
}

/// The data of a value of the user's registry; `None` if it does not exist or has another type.
fn read(key: &str, value: &str, kind: u32) -> Option<Vec<u8>> {
    let key = wide(key);
    let value = wide(value);
    let mut size = 0u32;
    let get = |data: *mut u8, size: &mut u32| unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), kind, std::ptr::null_mut(), data.cast(), size)
    };
    if get(std::ptr::null_mut(), &mut size) != ERROR_SUCCESS {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    (get(data.as_mut_ptr(), &mut size) == ERROR_SUCCESS).then(|| {
        data.truncate(size as usize);
        data
    })
}

/// Deletes a value; one that does not exist is no error.
fn delete(key: &str, value: &str) -> anyhow::Result<()> {
    let key = wide(key);
    let value = wide(value);
    match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) } {
        ERROR_SUCCESS | ERROR_FILE_NOT_FOUND => Ok(()),
        err => anyhow::bail!("Could not remove Diskman from the startup apps (error {err})."),
    }
}
