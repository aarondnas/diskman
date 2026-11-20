//! Keeping the system up while a sync runs, and stopping cleanly when the session ends anyway.
//!
//! - While a `RunGuard` is held the system does not go to sleep on its own (a USB drive that
//!   sleeps mid-copy is a real risk) and shutting down or signing out asks first, naming the
//!   reason. The user can still go ahead.
//! - Windows: `SetThreadExecutionState` against sleep; a hidden window answers the shutdown query
//!   with "not yet" plus `ShutdownBlockReasonCreate`, and calls `on_session_end`'s function
//!   when the session ends regardless.
//! - Linux: a block inhibitor of systemd-logind (`systemd-inhibit`) for sleep and shutdown;
//!   without systemd there is none. The process is told to end with SIGTERM like any other.
//!
//! Interrupting is never harmful to the archive (copies go through a temporary file, see
//! `sync::executor`); this only spares a run that has to be redone.

use super::sys;

/// Held while a sync runs; dropping it lets the system sleep and shut down again. Must be dropped
/// on the thread that created it (Windows keeps the "stay awake" request per thread).
pub struct RunGuard {
    _inner: sys::power::Guard,
}

/// `reason` is shown where the system asks whether to shut down anyway.
pub fn hold(reason: &str) -> RunGuard {
    RunGuard { _inner: sys::power::hold(reason) }
}

/// Calls `stop` when the session ends (shutdown, sign-out) while Diskman runs, so a running sync
/// can be stopped and recorded in the few seconds the system gives. Call once at the start.
pub fn on_session_end(stop: impl Fn() + Send + Sync + 'static) {
    sys::power::on_session_end(Box::new(stop));
}
