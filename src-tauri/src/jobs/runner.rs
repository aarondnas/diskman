//! The run of one backup, started by the queue for manual starts and scheduled syncs: plans all
//! enabled archives fresh and executes them. Skips plans that would empty the archive completely.
//! Every plan is stored with its result, so the activities show what the run did.

use super::{shutdown, EntryState, LiveEntry};
use crate::model::Backup;
use crate::platform;
use crate::state::AppState;
use crate::storage::history::{ArchiveRun, Outcome};
use crate::storage::plan_store;
use crate::sync::{executor, is_cancelled, planner};

/// Syncs all enabled archives. `what` names the run in status messages.
/// The job must have been started with `try_begin_job`.
pub fn run(state: &AppState, backup: &Backup, what: &str) {
    // No sleep and a question before shutting down while it runs (dropped at the end of `run`,
    // on this thread)
    let _awake = platform::power::hold(&shutdown::busy_reason(state));
    let tol = state.config.lock().unwrap().settings.mtime_tolerance_ms;
    let origin = state.job.lock().unwrap().origin.clone();
    let run = state.start_run(&backup.id, &backup.name, &backup.sources(), &origin);
    // The UI opens this activity once the run is done
    state.job.lock().unwrap().run_id = run;
    // The kept trees: a run right after a check only reads what changed since (sync::tree)
    let scans = planner::Scans { cache: Some(&state.scans), batch: None, stop: &state.cancel, count: None };
    match planner::build_plans(backup, tol, &scans) {
        // Cancelled while scanning: nothing was changed, so the archives keep their last status
        Err(e) if is_cancelled(&e) => {
            state.finish_run(run);
            state.end_job();
        }
        Ok(plans) => {
            if let Some(p) = plans.first() {
                state.record_scan(run, p.scan_ms);
            }
            // Every entry in the order of the backup, the skipped ones too (executor::run_plans
            // carries the others forward)
            state.job.lock().unwrap().entries = plans
                .iter()
                .map(|p| LiveEntry::of(p, if p.dangerous { EntryState::Skipped } else { EntryState::Waiting }))
                .collect();
            let (safe, risky): (Vec<_>, Vec<_>) = plans.into_iter().partition(|p| !p.dangerous);
            for p in &risky {
                // Its plan stays with it: the activities show what it would have deleted
                let skipped = ArchiveRun {
                    plan: plan_store::pack(p).map_err(|e| eprintln!("Plan not stored: {e:#}")).ok(),
                    summary: p.summary.clone(),
                    ops_total: p.ops.len() as u64,
                    ..ArchiveRun::not_run(
                        &p.archive_id,
                        &p.source,
                        &p.dest,
                        Some(p.mode),
                        Outcome::Skipped,
                        format!("{what} skipped: source is empty (safety stop)"),
                    )
                };
                state.record_archive(run, &skipped);
            }
            executor::run_plans(state, run, safe);
        }
        Err(e) => {
            for a in backup.archives.iter().filter(|a| a.enabled) {
                state.record_archive(
                    run,
                    &ArchiveRun::not_run(
                        &a.id,
                        &a.source,
                        &a.path,
                        Some(a.mode),
                        Outcome::Failed,
                        format!("{what} failed: {e:#}"),
                    ),
                );
            }
            state.finish_run(run);
            state.end_job();
        }
    }
}
