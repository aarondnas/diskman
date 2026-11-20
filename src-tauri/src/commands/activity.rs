//! The activities: every analysis and every run of the backups, newest first, each with what it
//! changed (or would have changed). Opening one loads its stored plans, so the UI shows their tree
//! of changes like that of a check (`sync::plan_tree`).

use std::sync::Arc;

use serde::Serialize;

use super::Shared;
use crate::storage::history::{Activity, ActivityQuery, PlanStorage};
use crate::storage::plan_store;
use crate::sync::issues::Issue;
use crate::sync::planner::PlanView;

/// One page of the activities.
#[tauri::command]
pub fn get_activities(state: Shared, query: ActivityQuery) -> Result<Vec<Activity>, String> {
    state.history.activities(&query).map_err(|e| format!("{e:#}"))
}

/// An entry of an opened activity: its plan (None if its file list is no longer stored or was
/// never made) and the failures of its run.
#[derive(Serialize)]
pub struct EntryDetail {
    /// Id of the entry (`ActivityEntry::id`)
    pub id: i64,
    pub plan: Option<PlanView>,
    pub errors: Vec<Issue>,
}

/// Opens an activity: its stored plans are loaded and kept until `close_activity` (or until
/// another one is opened), so the UI can ask for their trees of changes.
#[tauri::command]
pub async fn open_activity(state: Shared<'_>, id: i64, entries: Vec<i64>) -> Result<Vec<EntryDetail>, String> {
    let st = Arc::clone(&state);
    tauri::async_runtime::spawn_blocking(move || {
        let stored = st.history.stored_plans(id).map_err(|e| format!("{e:#}"))?;
        let mut opened = std::collections::HashMap::new();
        let mut details = Vec::with_capacity(entries.len());
        for entry in entries {
            let plan = stored.iter().find(|(e, _)| *e == entry).and_then(|(_, data)| {
                plan_store::unpack(data).map_err(|e| eprintln!("Stored plan {entry} not readable: {e:#}")).ok()
            });
            let errors = st.history.entry_errors(entry).map_err(|e| format!("{e:#}"))?;
            let view = plan.as_ref().map(|p| p.view());
            if let Some(plan) = plan {
                opened.insert(plan.id.clone(), plan);
            }
            details.push(EntryDetail { id: entry, plan: view, errors });
        }
        *st.opened.lock().unwrap() = opened;
        Ok(details)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The open activity was closed: its plans are no longer needed.
#[tauri::command]
pub fn close_activity(state: Shared) {
    state.opened.lock().unwrap().clear();
}

/// What the stored file lists take.
#[tauri::command]
pub fn activity_storage(state: Shared) -> Result<PlanStorage, String> {
    state.history.plan_storage().map_err(|e| format!("{e:#}"))
}

/// Deletes the whole history (every analysis and run, see `History::clear_activities`).
#[tauri::command]
pub fn clear_activities(state: Shared) -> Result<usize, String> {
    state.opened.lock().unwrap().clear();
    state.history.clear_activities().map_err(|e| format!("{e:#}"))
}

/// Deletes every stored file list; the activities themselves stay.
#[tauri::command]
pub fn clear_activity_plans(state: Shared) -> Result<PlanStorage, String> {
    state.history.clear_plans().map_err(|e| format!("{e:#}"))?;
    state.history.plan_storage().map_err(|e| format!("{e:#}"))
}
