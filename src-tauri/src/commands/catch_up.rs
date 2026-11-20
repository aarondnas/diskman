//! Runs missed while Diskman was off (`jobs::catch_up`): the question after the start.

use super::Shared;
use crate::jobs::catch_up::MissedRun;
use crate::jobs::queue;

/// Backups whose schedule was due while Diskman was off; empty once answered.
#[tauri::command]
pub fn get_missed_runs(state: Shared) -> Vec<MissedRun> {
    state.missed.lock().unwrap().clone()
}

/// Queues the chosen backups (origin `catch_up`, kept in the history) and closes the question.
/// Backups that no longer exist are skipped.
#[tauri::command]
pub fn run_missed(state: Shared, backup_ids: Vec<String>) {
    let known: Vec<String> = {
        let config = state.config.lock().unwrap();
        backup_ids.into_iter().filter(|id| config.backups.iter().any(|b| &b.id == id)).collect()
    };
    for id in &known {
        queue::enqueue(&state, id, "catch_up");
    }
    state.missed.lock().unwrap().clear();
}

/// Closes the question without running anything; the schedules go on as usual.
#[tauri::command]
pub fn dismiss_missed(state: Shared) {
    state.missed.lock().unwrap().clear();
}
