//! Checking and running backups, and their progress and status.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::Shared;
use crate::jobs::{queue, JobProgress};
use crate::state::{AppState, RunningCheck};
use crate::storage::history::{ArchiveStatus, RunProblem};
use crate::sync::is_cancelled;
use crate::sync::planner::{self, PlanView, Scans};
use chrono::Utc;
use serde::Serialize;

/// Latest result per archive, for the "last run …" line of each card.
#[tauri::command]
pub fn get_status(state: Shared) -> HashMap<String, ArchiveStatus> {
    state.status()
}

/// Latest results of a backup's archives that did not end well, with what failed: the notice after
/// a run and its details.
#[tauri::command]
pub fn get_run_problems(state: Shared, backup_id: String) -> Result<Vec<RunProblem>, String> {
    let archive_ids: Vec<String> = {
        let config = state.config.lock().unwrap();
        let backup = config.backups.iter().find(|b| b.id == backup_id).ok_or("Backup not found")?;
        backup.archives.iter().map(|a| a.id.clone()).collect()
    };
    state.history.problems(&archive_ids).map_err(|e| format!("{e:#}"))
}

/// Registers a running check (its stop flag for `cancel_checks`, its count for `check_progress`)
/// and removes it again when dropped. Its own entry per check: a new check of the same backup is
/// not stopped by an older cancel.
struct CheckFlag<'a> {
    state: &'a AppState,
    backup_id: String,
    check: Arc<RunningCheck>,
}

impl<'a> CheckFlag<'a> {
    fn new(state: &'a AppState, backup_id: &str) -> Self {
        let check = Arc::new(RunningCheck::default());
        state.checks.lock().unwrap().insert(backup_id.to_string(), Arc::clone(&check));
        CheckFlag { state, backup_id: backup_id.to_string(), check }
    }
}

impl Drop for CheckFlag<'_> {
    fn drop(&mut self) {
        // Only its own entry: a newer check of the backup may have replaced it
        let mut checks = self.state.checks.lock().unwrap();
        if checks.get(&self.backup_id).is_some_and(|c| Arc::ptr_eq(c, &self.check)) {
            checks.remove(&self.backup_id);
        }
    }
}

/// Folders and files a running check has read so far.
#[derive(Serialize)]
pub struct CheckProgress {
    files: u64,
    dirs: u64,
}

/// Live progress of the running checks of these backups (those not running are left out).
#[tauri::command]
pub fn check_progress(state: Shared, backup_ids: Vec<String>) -> HashMap<String, CheckProgress> {
    let checks = state.checks.lock().unwrap();
    backup_ids
        .into_iter()
        .filter_map(|id| {
            let (files, dirs) = checks.get(&id)?.count.get();
            Some((id, CheckProgress { files, dirs }))
        })
        .collect()
}

/// "Check": plans all enabled archives of a backup without running them, and keeps the plans for
/// the analysis (`plan_tree`, `probe_plans`). Every check that is not cancelled is stored as an
/// activity (`commands::activity`). Checks of different drives run side by side, those
/// of the same drive one after another (`ScanCache::turns`). `batch`: the number of a "Check all",
/// whose backups share scans of the same folder until `end_check_batch`. None if cancelled.
#[tauri::command]
pub async fn preview(
    state: Shared<'_>,
    backup_id: String,
    batch: Option<u64>,
) -> Result<Option<Vec<PlanView>>, String> {
    let (backup, tol) = {
        let config = state.config.lock().unwrap();
        let backup = config.backups.iter().find(|t| t.id == backup_id).cloned();
        (backup.ok_or("Backup not found")?, config.settings.mtime_tolerance_ms)
    };
    let st = Arc::clone(&state);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let flag = CheckFlag::new(&st, &backup.id);
        let enabled = backup.archives.iter().filter(|a| a.enabled);
        let turns = st.scans.turns(enabled.flat_map(|a| [a.source.as_str(), a.path.as_str()]));
        // A panic in an earlier scan does not block
        let _held: Vec<_> = turns.iter().map(|t| t.lock().unwrap_or_else(|e| e.into_inner())).collect();
        let scans = Scans { cache: Some(&st.scans), batch, stop: &flag.check.stop, count: Some(&flag.check.count) };
        let started = Utc::now();
        let plans = planner::build_plans(&backup, tol, &scans);
        match &plans {
            Ok(plans) => st.record_check(&backup, started, Ok(plans)),
            Err(e) if !is_cancelled(e) => st.record_check(&backup, started, Err(e)),
            Err(_) => {}
        }
        let plans = plans?;
        let views = plans.iter().map(|p| p.view()).collect::<Vec<_>>();
        st.plans.lock().unwrap().insert(backup.id.clone(), plans);
        Ok::<_, anyhow::Error>(views)
    })
    .await;
    match result.map_err(|e| e.to_string())? {
        Ok(views) => Ok(Some(views)),
        Err(e) if is_cancelled(&e) => Ok(None),
        Err(e) => Err(format!("{e:#}")),
    }
}

/// "Check all" with this number is done: the scans its backups shared are forgotten.
#[tauri::command]
pub fn end_check_batch(state: Shared, batch: u64) {
    state.scans.end_batch(batch);
}

/// Second step of a check, after its result is shown: opens the source files the kept plans of a
/// backup would copy, to find locked and unreadable ones. Returns the plans with what it found;
/// None if cancelled (`cancel_checks`) or if a newer check replaced the plans meanwhile.
#[tauri::command]
pub async fn probe_plans(state: Shared<'_>, backup_id: String) -> Result<Option<Vec<PlanView>>, String> {
    let st = Arc::clone(&state);
    tauri::async_runtime::spawn_blocking(move || {
        let flag = CheckFlag::new(&st, &backup_id);
        // The paths to open, taken out so the plans stay readable for the list meanwhile
        let work: Vec<(String, String, Vec<String>)> = match st.plans.lock().unwrap().get(&backup_id) {
            Some(plans) => plans.iter().map(|p| (p.id.clone(), p.source.clone(), planner::probe_paths(p))).collect(),
            None => return None,
        };
        let mut found = Vec::new();
        for (id, source, paths) in work {
            let started = std::time::Instant::now();
            let failed = planner::probe(std::path::Path::new(&source), &paths, &flag.check.stop)?;
            found.push((id, failed, started.elapsed().as_millis() as u64));
        }
        let mut kept = st.plans.lock().unwrap();
        let plans = kept.get_mut(&backup_id)?;
        for (id, failed, ms) in found {
            let plan = plans.iter_mut().find(|p| p.id == id)?;
            planner::apply_probe(plan, failed, ms);
        }
        let views = plans.iter().map(|p| p.view()).collect();
        // The stored analysis takes what the probe found; written without holding the plans
        let probed = plans.clone();
        drop(kept);
        st.record_probed(&probed);
        Some(views)
    })
    .await
    .map_err(|e| e.to_string())
}

/// One level of the tree of changes of a kept plan (see `Plan::tree`): of a check, or of the
/// activity open in the UI (`commands::activity`).
#[tauri::command]
pub fn plan_tree(
    state: Shared,
    plan_id: String,
    filter: String,
    query: String,
    dir: String,
) -> Result<serde_json::Value, String> {
    let level =
        |plan: &planner::Plan| serde_json::to_value(plan.tree(&filter, &query, &dir)).map_err(|e| e.to_string());
    if let Some(plan) = state.plans.lock().unwrap().values().flatten().find(|p| p.id == plan_id) {
        return level(plan);
    }
    let opened = state.opened.lock().unwrap();
    level(opened.get(&plan_id).ok_or("Analysis no longer available")?)
}

/// One level of the tree of what the running sync has done so far in the entry with this plan
/// (see `Plan::done_tree`); it grows while the sync runs. Gone once the sync ends: its activity
/// shows the result then.
#[tauri::command]
pub fn run_tree(state: Shared, plan_id: String, dir: String) -> Result<serde_json::Value, String> {
    let plans = state.running.lock().unwrap().clone().ok_or("No sync is running")?;
    let plan = plans.iter().find(|p| p.id == plan_id).ok_or("Not part of the running sync")?;
    let done = state.job.lock().unwrap().entries.iter().find(|e| e.plan_id == plan_id).map_or(0, |e| e.done_ops);
    serde_json::to_value(plan.done_tree(done as usize, &dir)).map_err(|e| e.to_string())
}

/// The kept plans of these backups are no longer shown (outdated by a run, or the backup is gone).
#[tauri::command]
pub fn drop_plans(state: Shared, backup_ids: Vec<String>) {
    let mut kept = state.plans.lock().unwrap();
    for id in &backup_ids {
        kept.remove(id);
    }
}

/// Stops the checks of these backups (running or waiting); each of them then answers None.
#[tauri::command]
pub fn cancel_checks(state: Shared, backup_ids: Vec<String>) {
    state.cancel_checks(&backup_ids);
}

/// Queues a backup (see `jobs::queue`). If it is already waiting or running, nothing happens.
#[tauri::command]
pub fn run_backup(state: Shared, backup_id: String) -> Result<JobProgress, String> {
    {
        let config = state.config.lock().unwrap();
        let backup = config.backups.iter().find(|t| t.id == backup_id).ok_or("Backup not found")?;
        if !backup.archives.iter().any(|a| a.enabled) {
            return Err(format!("\"{}\" has no enabled archive.", backup.name));
        }
    }
    queue::enqueue(&state, &backup_id, "manual");
    Ok(get_job(state))
}

#[tauri::command]
pub fn get_job(state: Shared) -> JobProgress {
    // Hold both locks at once so a backup never shows as both waiting and running
    // (or neither) while the worker takes it over
    let job = state.job.lock().unwrap();
    let queued = state.queue.lock().unwrap().iter().map(|q| q.backup_id.clone()).collect();
    JobProgress { queued, ..job.clone() }
}

/// Cancels the running sync and clears the queue.
#[tauri::command]
pub fn cancel_job(state: Shared) {
    state.queue.lock().unwrap().clear();
    state.cancel.store(true, Ordering::Relaxed);
}

/// Cancels only these backups, running or waiting (a card, or the backups of "Run all").
#[tauri::command]
pub fn cancel_backups(state: Shared, backup_ids: Vec<String>) -> JobProgress {
    state.cancel_backups(&backup_ids);
    get_job(state)
}
