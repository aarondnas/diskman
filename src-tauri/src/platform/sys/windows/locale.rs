//! Display language of Windows (the user's first preferred UI language, e.g. "de-DE").

use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};

pub fn system_language() -> Option<String> {
    let mut count = 0u32;
    let mut len = 0u32;
    unsafe {
        if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, std::ptr::null_mut(), &mut len) == 0 || len == 0 {
            return None;
        }
        let mut buf = vec![0u16; len as usize];
        if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, buf.as_mut_ptr(), &mut len) == 0 {
            return None;
        }
        // A list of zero-terminated names; the first one counts
        let first = buf.split(|&c| c == 0).next()?;
        (!first.is_empty()).then(|| String::from_utf16_lossy(first))
    }
}
