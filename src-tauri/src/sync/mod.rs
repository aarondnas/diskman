//! The sync engine: scans a source folder and its archive, plans what has to change and executes
//! that plan. It knows nothing about the UI or the queue; the executor reports its progress to the
//! job and its results to the history through `AppState`.

pub mod changes;
pub mod executor;
pub mod issues;
pub mod planner;
pub mod scanner;
pub mod tree;

use std::sync::atomic::{AtomicBool, Ordering};

/// Error of a scan or check that was stopped on request (a cancelled run or check), not a failure.
#[derive(Debug)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Cancelled")
    }
}

impl std::error::Error for Cancelled {}

pub fn is_cancelled(e: &anyhow::Error) -> bool {
    e.is::<Cancelled>()
}

/// Fails with `Cancelled` once `stop` is set.
pub fn check_stop(stop: &AtomicBool) -> anyhow::Result<()> {
    if stop.load(Ordering::Relaxed) {
        return Err(Cancelled.into());
    }
    Ok(())
}
