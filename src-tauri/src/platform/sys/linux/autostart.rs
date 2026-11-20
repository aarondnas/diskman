//! Autostart on Linux: an XDG autostart entry, `$XDG_CONFIG_HOME/autostart/diskman.desktop`
//! (usually `~/.config/autostart`), which GNOME, KDE, Xfce and the others start at login.
//! Packages (.deb, .rpm) install for all users and cannot ask, and an AppImage is not installed at
//! all, so the app asks on its first start instead (`asked_by_installer` is always false).

use std::fs;
use std::path::{Path, PathBuf};

use crate::platform::autostart::BACKGROUND_ARG;

fn entry() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("autostart").join("diskman.desktop"))
}

pub fn is_enabled() -> bool {
    let Some(text) = entry().and_then(|p| fs::read_to_string(p).ok()) else { return false };
    // Desktops switch an entry off with one of these instead of deleting it
    !text.lines().map(str::trim).any(|l| l == "Hidden=true" || l == "X-GNOME-Autostart-enabled=false")
}

pub fn set_enabled(on: bool, program: &Path) -> anyhow::Result<()> {
    let path = entry().ok_or_else(|| anyhow::anyhow!("No configuration folder for the autostart entry."))?;
    if !on {
        return match fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        };
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(&path, desktop_entry(program))?;
    Ok(())
}

pub fn asked_by_installer() -> bool {
    false
}

/// The AppImage itself when Diskman runs from one (the program inside lives in a temporary
/// mount that changes with every start), otherwise this program.
pub fn program() -> anyhow::Result<PathBuf> {
    match std::env::var_os("APPIMAGE") {
        Some(appimage) if !appimage.is_empty() => Ok(PathBuf::from(appimage)),
        _ => Ok(std::env::current_exe()?),
    }
}

fn desktop_entry(program: &Path) -> String {
    // Exec quoting of the Desktop Entry spec: in double quotes, with \ " ` $ escaped
    let mut quoted = String::from("\"");
    for c in program.to_string_lossy().chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    format!(
        "[Desktop Entry]\nType=Application\nName=Diskman\nComment=Runs scheduled backups in the background\n\
         Exec={quoted} {BACKGROUND_ARG}\nIcon=diskman\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_line_quotes_the_program() {
        let entry = desktop_entry(Path::new("/home/me/Apps/Disk man $1.AppImage"));
        assert!(entry.contains("Exec=\"/home/me/Apps/Disk man \\$1.AppImage\" --background\n"));
    }
}
