//! Administrator rights on Windows: the elevated token and the UAC restart.

use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, HANDLE};
use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, WaitForSingleObject, PROCESS_SYNCHRONIZE,
};
use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SHELLEXECUTEINFOW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use super::wide;

pub const CAN_ELEVATE: bool = true;

pub fn is_elevated() -> bool {
    unsafe {
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

/// Starts this program again via the "runas" verb, which shows the UAC prompt.
pub fn restart_elevated(args: &str) -> std::io::Result<bool> {
    let exe = wide(std::env::current_exe()?);
    let verb = wide("runas");
    let params = wide(args);
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        info.lpVerb = verb.as_ptr();
        info.lpFile = exe.as_ptr();
        info.lpParameters = params.as_ptr();
        info.nShow = SW_SHOWNORMAL;
        if ShellExecuteExW(&mut info) != 0 {
            return Ok(true);
        }
    }
    let err = std::io::Error::last_os_error();
    if err.raw_os_error() == Some(ERROR_CANCELLED as i32) {
        Ok(false)
    } else {
        Err(err)
    }
}

pub fn wait_for_exit(pid: u32, timeout_ms: u32) {
    unsafe {
        let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if !process.is_null() {
            WaitForSingleObject(process, timeout_ms);
            CloseHandle(process);
        }
    }
}
