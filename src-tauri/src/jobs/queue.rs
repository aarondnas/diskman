//! Queue for backups that should run one after another: manually started ones and those from
//! scheduled backups. Entries are accepted immediately and processed as soon as no other
//! sync is running.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use super::runner;
use crate::state::AppState;

/// A waiting backup and what queued it (`manual`, `schedule` or `catch_up`, ends up in the history).
#[derive(Debug, Clone)]
pub struct QueueItem {
    pub backup_id: String,
    pub origin: &'static str,
}

/// Queues a backup. If it is already waiting or running, another call changes nothing.
pub fn enqueue(state: &AppState, backup_id: &str, origin: &'static str) {
    // Take the job lock first and hold it while queueing, so the worker cannot
    // take the backup out of the queue between both checks
    let job = state.job.lock().unwrap();
    let mut queue = state.queue.lock().unwrap();
    if (job.running && job.backup_id == backup_id) || queue.iter().any(|q| q.backup_id == backup_id) {
        return;
    }
    queue.push_back(QueueItem { backup_id: backup_id.to_string(), origin });
}

/// Starts the job for the first waiting backup and takes it out of the queue; None if the queue is
/// empty or another sync is running.
fn take_next(state: &AppState) -> Option<QueueItem> {
    if state.quitting.load(Ordering::SeqCst) {
        return None;
    }
    let item = state.queue.lock().unwrap().front().cloned()?;
    if !state.try_begin_job(item.origin) {
        return None; // another sync is running, try again on the next tick
    }
    // Mark as running first, then take it out of the queue: that way the backup is always either
    // waiting or running for `enqueue` and the UI, without a gap. It is looked up again: it may
    // have been cancelled since (`AppState::cancel_backups`), and then the job ends unused.
    let mut job = state.job.lock().unwrap();
    let mut queue = state.queue.lock().unwrap();
    let Some(pos) = queue.iter().position(|q| q.backup_id == item.backup_id) else {
        job.running = false;
        return None;
    };
    queue.remove(pos);
    job.backup_id = item.backup_id.clone();
    Some(item)
}

pub fn spawn(state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        loop {
            tick.tick().await;
            let Some(item) = take_next(&state) else { continue };
            let backup = state.config.lock().unwrap().backups.iter().find(|b| b.id == item.backup_id).cloned();
            let Some(backup) = backup else {
                state.end_job(); // deleted in the meantime
                continue;
            };
            state.job.lock().unwrap().label = backup.name.clone();
            let what = if item.origin == "schedule" { "Scheduled sync" } else { "Sync" };
            let st = state.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || runner::run(&st, &backup, what)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queued(state: &AppState) -> Vec<String> {
        state.queue.lock().unwrap().iter().map(|q| q.backup_id.clone()).collect()
    }

    #[test]
    fn ignores_backups_already_waiting_or_running() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::load_from(dir.path().to_path_buf()).unwrap();

        enqueue(&state, "a", "manual");
        enqueue(&state, "b", "schedule");
        enqueue(&state, "a", "schedule");
        assert_eq!(queued(&state), ["a", "b"]);
        assert_eq!(state.queue.lock().unwrap()[0].origin, "manual");

        // The worker takes "a": it is running now and no longer waiting
        assert!(state.try_begin_job("manual"));
        state.job.lock().unwrap().backup_id = "a".into();
        state.queue.lock().unwrap().pop_front();
        enqueue(&state, "a", "manual");
        assert_eq!(queued(&state), ["b"]);

        // After the run, "a" may be queued again
        state.end_job();
        enqueue(&state, "a", "manual");
        assert_eq!(queued(&state), ["b", "a"]);
    }

    #[test]
    fn the_worker_takes_the_first_waiting_backup_unless_a_sync_runs() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::load_from(dir.path().to_path_buf()).unwrap();
        enqueue(&state, "a", "manual");
        enqueue(&state, "b", "schedule");

        let item = take_next(&state).unwrap();
        assert_eq!(item.backup_id, "a");
        assert_eq!(state.job.lock().unwrap().backup_id, "a");
        assert_eq!(queued(&state), ["b"]);
        assert!(take_next(&state).is_none(), "one sync at a time");

        state.end_job();
        assert_eq!(take_next(&state).unwrap().origin, "schedule");
        assert!(queued(&state).is_empty());

        // While Diskman ends, nothing starts
        state.end_job();
        enqueue(&state, "c", "manual");
        state.quitting.store(true, Ordering::SeqCst);
        assert!(take_next(&state).is_none());
    }
}
