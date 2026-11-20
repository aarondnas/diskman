//! Administrator rights: whether Diskman has them, and restarting it with them.
//!
//! Windows: unreadable files can often be read after a restart with administrator rights (UAC).
//! The restart starts a second, elevated Diskman and then quits this one; the new one first waits
//! for the old one to exit (`--wait-pid`), otherwise the single-instance check would hand it over
//! to the old one, which is just quitting.
//!
//! Linux: a desktop app is not restarted as root; missing permissions are fixed on the files
//! themselves. Only whether Diskman already runs as root is detected.

use std::sync::OnceLock;

use super::sys;

/// Argument of the elevated instance: the id of the process it replaces.
const WAIT_ARG: &str = "--wait-pid";

/// Diskman runs with full administrator rights (Windows: elevated token; Linux: root).
pub fn is_elevated() -> bool {
    static ELEVATED: OnceLock<bool> = OnceLock::new();
    *ELEVATED.get_or_init(sys::elevation::is_elevated)
}

/// This system can restart Diskman with administrator rights (`restart_elevated`).
pub fn can_elevate() -> bool {
    sys::elevation::CAN_ELEVATE && !is_elevated()
}

/// Starts Diskman again with administrator rights. `Ok(false)`: the permission was declined.
/// The caller quits this instance afterwards.
pub fn restart_elevated() -> std::io::Result<bool> {
    if !sys::elevation::CAN_ELEVATE {
        return Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "not available on this system"));
    }
    sys::elevation::restart_elevated(&format!("{WAIT_ARG} {}", std::process::id()))
}

/// Called first thing at start: if this instance replaces another one, wait until it has exited
/// (at most a few seconds).
pub fn wait_for_replaced_instance() {
    let mut args = std::env::args().skip_while(|a| a != WAIT_ARG).skip(1);
    if let Some(pid) = args.next().and_then(|p| p.parse::<u32>().ok()) {
        sys::elevation::wait_for_exit(pid, 15_000);
    }
}
