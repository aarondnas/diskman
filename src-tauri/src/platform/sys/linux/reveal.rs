//! The file manager on Linux. Selecting an item goes through the D-Bus interface
//! `org.freedesktop.FileManager1` (Nautilus, Dolphin, Nemo, Caja, Thunar and others offer it);
//! without it the folder is opened with `xdg-open`, which every desktop has.

use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::process::{Command, Stdio};

/// `file://` URI of a path: every byte except unreserved characters and "/" percent-encoded.
/// Commas too, since `dbus-send` splits array values at them.
fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for &b in path.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            uri.push(b as char);
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    uri
}

pub fn select(path: &Path) -> io::Result<()> {
    let shown = Command::new("dbus-send")
        .args([
            "--session",
            "--print-reply",
            "--reply-timeout=5000",
            "--dest=org.freedesktop.FileManager1",
            "--type=method_call",
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1.ShowItems",
        ])
        .arg(format!("array:string:{}", file_uri(path)))
        .arg("string:")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match shown {
        Ok(status) if status.success() => Ok(()),
        // No file manager offers it (or no dbus-send): open the folder it lies in
        _ => open_folder(path.parent().unwrap_or(path)),
    }
}

pub fn open_link(url: &str) -> io::Result<()> {
    open(std::ffi::OsStr::new(url))
}

pub fn open_folder(dir: &Path) -> io::Result<()> {
    open(dir.as_os_str())
}

/// `xdg-open`: a folder in the file manager, a web address in the browser.
fn open(what: &std::ffi::OsStr) -> io::Result<()> {
    let mut child = Command::new("xdg-open").arg(what).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
    // Collected when it ends, so it leaves no zombie process behind
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::file_uri;
    use std::path::Path;

    #[test]
    fn uris_encode_what_is_not_plain() {
        assert_eq!(file_uri(Path::new("/home/a/My Docs/x,y.txt")), "file:///home/a/My%20Docs/x%2Cy.txt");
        assert_eq!(file_uri(Path::new("/tmp/Über")), "file:///tmp/%C3%9Cber");
    }
}
