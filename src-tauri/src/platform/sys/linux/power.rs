//! Sleep and shutdown during a sync on Linux: a block inhibitor of systemd-logind.

use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};

/// The `systemd-inhibit` process holding the inhibitor. Its command is `cat` reading from a pipe
/// of ours: dropping our end (or Diskman ending in any way, even a crash) ends `cat`, then
/// `systemd-inhibit`, and the inhibitor goes with it. No leftover process can keep it.
pub struct Guard {
    held: Option<(Child, ChildStdin)>,
}

pub fn hold(reason: &str) -> Guard {
    let child = Command::new("systemd-inhibit")
        .args(["--what=sleep:shutdown", "--who=Diskman", "--mode=block"])
        .arg(format!("--why={reason}"))
        .arg("cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let held = match child {
        Ok(mut child) => child.stdin.take().map(|stdin| (child, stdin)),
        // No systemd (or not in PATH): nothing holds the system up, the sync runs as before
        Err(_) => None,
    };
    Guard { held }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some((mut child, mut stdin)) = self.held.take() {
            let _ = stdin.flush();
            drop(stdin); // EOF for cat
            let _ = child.wait();
        }
    }
}

/// systemd sends SIGTERM when the session ends; there is no message to watch for.
pub fn on_session_end(_stop: Box<dyn Fn() + Send + Sync>) {}
