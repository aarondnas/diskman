//! Administrator rights on Linux: only detected. Restarting a desktop app as root is not offered;
//! permissions are fixed on the files themselves (chmod, chown, groups).

pub const CAN_ELEVATE: bool = false;

pub fn is_elevated() -> bool {
    // SAFETY: geteuid has no preconditions and cannot fail
    unsafe { libc::geteuid() == 0 }
}

pub fn restart_elevated(_args: &str) -> std::io::Result<bool> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "not available on Linux"))
}

/// Never needed: without `restart_elevated` no instance replaces another one.
pub fn wait_for_exit(_pid: u32, _timeout_ms: u32) {}
