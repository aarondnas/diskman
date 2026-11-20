//! Window frame and icon on Windows.
//!
//! The window has no native title bar (tauri.windows.conf.json: `decorations: false`); the UI draws
//! the title bar and the window buttons itself (src/platform/windows/TitleBar.svelte). Windows 11
//! still draws the thin border and the shadow, whose color and dark mode are set here (older
//! Windows versions ignore these settings). They follow the app's theme (dark unless light mode is
//! chosen in the settings), not the system's mode.
//!
//! Tauri only sets the largest image from `icon.ico` (256 px) as the window icon, which Windows
//! then scales down roughly to 24–32 px. Instead, the icon is loaded from the .exe resource (which
//! contains all sizes of the .ico), so Windows picks the right size for the current scaling.

/// Colors as 0x00BBGGRR (COLORREF), matching the tokens in global.css (--bg, --text, --border)
struct Colors {
    caption: u32,
    text: u32,
    border: u32,
}

const DARK: Colors =
    Colors { caption: rgb(0x14, 0x14, 0x14), text: rgb(0xe8, 0xe8, 0xe8), border: rgb(0x33, 0x33, 0x33) };
const LIGHT: Colors =
    Colors { caption: rgb(0xf4, 0xf4, 0xf4), text: rgb(0x1d, 0x1d, 0x1d), border: rgb(0xde, 0xde, 0xde) };

const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

pub fn apply_frame(window: &tauri::Window, light: bool) {
    if let Ok(hwnd) = window.hwnd() {
        set_colors(hwnd.0, if light { &LIGHT } else { &DARK }, !light);
    }
}

pub fn apply_icon(window: &tauri::Window) {
    if let Ok(hwnd) = window.hwnd() {
        set_icons(hwnd.0);
    }
}

fn set_colors(hwnd: *mut core::ffi::c_void, colors: &Colors, dark: bool) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    };

    let set = |attribute, value: u32| unsafe {
        // Errors only mean that this Windows version does not know the attribute
        DwmSetWindowAttribute(hwnd, attribute as _, &value as *const u32 as *const _, size_of::<u32>() as u32);
    };
    // Light or dark glyphs for minimize/maximize/close
    set(DWMWA_USE_IMMERSIVE_DARK_MODE, dark as u32);
    set(DWMWA_CAPTION_COLOR, colors.caption);
    set(DWMWA_TEXT_COLOR, colors.text);
    set(DWMWA_BORDER_COLOR, colors.border);
}

fn set_icons(hwnd: *mut core::ffi::c_void) {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        LoadImageW, SendMessageW, ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_DEFAULTCOLOR, SM_CXICON, SM_CXSMICON, WM_SETICON,
    };

    /// Resource id under which tauri-build embeds the app icon (`WINDOWS_APP_ICON_RESOURCE_ID`)
    const APP_ICON_ID: u16 = 32512;

    unsafe {
        let module = GetModuleHandleW(core::ptr::null());
        let dpi = match GetDpiForWindow(hwnd) {
            0 => 96,
            d => d,
        };
        for (which, metric) in [(ICON_BIG, SM_CXICON), (ICON_SMALL, SM_CXSMICON)] {
            let size = GetSystemMetricsForDpi(metric, dpi);
            let icon = LoadImageW(module, APP_ICON_ID as usize as *const u16, IMAGE_ICON, size, size, LR_DEFAULTCOLOR);
            if !icon.is_null() {
                // The previous icon may belong to Tauri, so it is not freed;
                // each DPI change leaves two small icons in memory.
                SendMessageW(hwnd, WM_SETICON, which as usize, icon as isize);
            }
        }
    }
}
