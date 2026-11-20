//! Running backups: the queue that runs them one after another, the timer of the schedules, the
//! search for runs missed while Diskman was off (`catch_up`) and the run of one backup. At most
//! one sync runs at a time; its progress is the `JobProgress`.

pub mod catch_up;
pub mod queue;
pub mod runner;
pub mod schedules;
pub mod shutdown;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::model::Mode;
use crate::sync::planner::{Plan, Summary};

/// Progress of the running sync, polled by the UI. Stays as it ended until the next sync starts.
#[derive(Debug, Clone, Default, Serialize)]
pub struct JobProgress {
    pub running: bool,
    /// What started it: "manual", "schedule" or "catch_up" (also stored in the history)
    pub origin: String,
    /// Backup currently running (to show it on the right card)
    pub backup_id: String,
    /// Waiting backups in order, only filled in when queried
    pub queued: Vec<String>,
    /// "Backup → archive" being synced
    pub label: String,
    pub total_ops: u64,
    pub done_ops: u64,
    /// Bytes to copy and copied so far (new and updated files)
    pub total_bytes: u64,
    pub done_bytes: u64,
    /// What could not be transferred so far (failed operations and entries the plan already knew
    /// it cannot transfer)
    pub error_count: u64,
    /// The activity of this run in the history, to show it once it is done (None: not stored)
    pub run_id: Option<i64>,
    pub started_at: Option<DateTime<Utc>>,
    /// When the scans were done and the operations began (speed and time left count from here)
    pub ops_started_at: Option<DateTime<Utc>>,
    /// Path of the operation being carried out, relative to its entry
    pub current: String,
    /// The entries of the run in their order, with their own progress; empty while scanning
    pub entries: Vec<LiveEntry>,
}

/// Where an entry of the running sync stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryState {
    #[default]
    Waiting,
    Running,
    Done,
    Cancelled,
    /// Not run: it would have emptied the archive (safety stop, see `jobs::runner`)
    Skipped,
}

/// One entry (source → archive) of the running sync and how far it got. Its operations run in the
/// order of its plan, so the first `done_ops` of them are done (`commands::sync::run_tree`).
#[derive(Debug, Clone, Serialize)]
pub struct LiveEntry {
    pub plan_id: String,
    pub archive_id: String,
    pub source: String,
    pub dest: String,
    pub mode: Mode,
    pub state: EntryState,
    /// What the plan does (new, updated, moved, deleted …)
    pub summary: Summary,
    pub total_ops: u64,
    pub done_ops: u64,
    pub total_bytes: u64,
    pub done_bytes: u64,
    pub error_count: u64,
}

impl LiveEntry {
    pub fn of(plan: &Plan, state: EntryState) -> Self {
        LiveEntry {
            plan_id: plan.id.clone(),
            archive_id: plan.archive_id.clone(),
            source: plan.source.clone(),
            dest: plan.dest.clone(),
            mode: plan.mode,
            state,
            summary: plan.summary.clone(),
            total_ops: plan.ops.len() as u64,
            done_ops: 0,
            total_bytes: plan.summary.copy_bytes + plan.summary.update_bytes,
            done_bytes: 0,
            error_count: 0,
        }
    }
}
