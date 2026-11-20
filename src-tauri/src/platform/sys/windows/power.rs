//! Sleep and shutdown during a sync on Windows.
//!
//! Sleep: `SetThreadExecutionState(ES_SYSTEM_REQUIRED)` on the thread that runs the sync.
//! Shutdown and sign-out: Windows asks every top-level window first (`WM_QUERYENDSESSION`).
//! Diskman's main window may not exist (start in the background) and belongs to Tauri, so a
//! hidden window of its own, on its own thread, answers: "not yet" while a sync runs, with the
//! reason Windows shows next to "Shut down anyway". If the session ends regardless
//! (`WM_ENDSESSION`), the registered function stops the sync before Windows ends the process.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED};
use windows_sys::Win32::System::Shutdown::{ShutdownBlockReasonCreate, ShutdownBlockReasonDestroy};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW, TranslateMessage, MSG,
    WM_ENDSESSION, WM_QUERYENDSESSION, WNDCLASSW, WS_OVERLAPPED,
};

use super::wide;

/// Number of guards held (one per running sync; at most one at a time today).
static RUNNING: AtomicUsize = AtomicUsize::new(0);
/// Reason shown by Windows, zero-terminated UTF-16.
static REASON: Mutex<Vec<u16>> = Mutex::new(Vec::new());
static ON_END: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

/// Not `Send`: the execution state belongs to the thread that set it.
pub struct Guard {
    _thread_bound: std::marker::PhantomData<*const ()>,
}

pub fn hold(reason: &str) -> Guard {
    *REASON.lock().unwrap() = wide(reason);
    RUNNING.fetch_add(1, Ordering::SeqCst);
    unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
    Guard { _thread_bound: std::marker::PhantomData }
}

impl Drop for Guard {
    fn drop(&mut self) {
        RUNNING.fetch_sub(1, Ordering::SeqCst);
        unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
    }
}

pub fn on_session_end(stop: Box<dyn Fn() + Send + Sync>) {
    if ON_END.set(stop).is_err() {
        return; // registered already
    }
    std::thread::spawn(|| unsafe {
        let class = wide("DiskmanSession");
        let module = GetModuleHandleW(core::ptr::null());
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: module,
            lpszClassName: class.as_ptr(),
            ..core::mem::zeroed()
        };
        if RegisterClassW(&wc) == 0 {
            eprintln!("Shutdown watch not available: {}", std::io::Error::last_os_error());
            return;
        }
        // A top-level window that is never shown (a message-only window would not be asked)
        let title = wide("Diskman");
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            module,
            core::ptr::null(),
        );
        if hwnd.is_null() {
            eprintln!("Shutdown watch not available: {}", std::io::Error::last_os_error());
            return;
        }
        let mut msg: MSG = core::mem::zeroed();
        while GetMessageW(&mut msg, core::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_QUERYENDSESSION => {
            if RUNNING.load(Ordering::SeqCst) > 0 {
                let reason = REASON.lock().unwrap().clone();
                ShutdownBlockReasonCreate(hwnd, reason.as_ptr());
                0 // not yet: Windows shows the reason and lets the user decide
            } else {
                ShutdownBlockReasonDestroy(hwnd);
                1
            }
        }
        WM_ENDSESSION => {
            // wparam: the session really ends (not cancelled by the user)
            if wparam != 0 {
                if let Some(stop) = ON_END.get() {
                    stop();
                }
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
