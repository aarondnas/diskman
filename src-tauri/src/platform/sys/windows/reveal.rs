//! Explorer: `/select,` opens the folder of an item with the item selected.

use std::io;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

/// Explorer only understands backslashes.
fn native(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\")
}

pub fn select(path: &Path) -> io::Result<()> {
    // Passed as written: Explorer parses this argument itself and needs the quotes inside it
    // (Windows paths cannot contain quotes). Explorer ends with exit code 1 even when it worked.
    Command::new("explorer.exe").raw_arg(format!("/select,\"{}\"", native(path))).spawn()?;
    Ok(())
}

/// Explorer hands a web address to the default browser.
pub fn open_link(url: &str) -> io::Result<()> {
    Command::new("explorer.exe").arg(url).spawn()?;
    Ok(())
}

pub fn open_folder(dir: &Path) -> io::Result<()> {
    Command::new("explorer.exe").arg(native(dir)).spawn()?;
    Ok(())
}
