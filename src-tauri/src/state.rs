//! Shared state of the running app: the config, the history, the running job and its queue.
//! Commands, the queue and the schedules all work on this one `AppState`.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};

use crate::jobs::catch_up::MissedRun;
use crate::jobs::queue::QueueItem;
use crate::jobs::JobProgress;
use crate::model::{Backup, Config};
use crate::storage::history::{ArchiveRun, ArchiveStatus, History, Outcome};
use crate::storage::{config_file, data_dir, plan_store};
use crate::sync::planner::Plan;
use crate::sync::tree::{ScanCache, ScanCount};

/// A running check of one backup.
#[derive(Debug, Default)]
pub struct RunningCheck {
    /// Once set, the check ends with `Cancelled`
    pub stop: AtomicBool,
    /// Folders and files read so far
    pub count: ScanCount,
}

pub struct AppState {
    pub data_dir: PathBuf,
    pub config: Mutex<Config>,
    /// History of all runs (SQLite); also provides the status per archive.
    pub history: History,
    pub job: Mutex<JobProgress>,
    /// Backups waiting for their run (manual or from schedules, see `jobs::queue`).
    /// Lock order: always `job` first, then `queue`.
    pub queue: Mutex<VecDeque<QueueItem>>,
    pub cancel: AtomicBool,
    /// Diskman is ending (`jobs::shutdown`): no further run starts.
    pub quitting: AtomicBool,
    /// Kept folder trees, so scans only read what changed (`sync::tree`), and the turns checks of
    /// the same drive take.
    pub scans: ScanCache,
    /// The plans of the latest check per backup id, for the analysis to page through and probe.
    pub plans: Mutex<HashMap<String, Vec<Plan>>>,
    /// The stored plans of the activity open in the UI, by plan id (`commands::activity`).
    pub opened: Mutex<HashMap<String, Plan>>,
    /// The plans of the running sync while it carries them out, for its live tree
    /// (`commands::sync::run_tree`); how far each got is in `job.entries`.
    pub running: Mutex<Option<Arc<Vec<Plan>>>>,
    /// Each check that is waiting, scanning or probing, per backup id: its stop flag
    /// (`cancel_checks`) and what it has read so far (`check_progress`).
    pub checks: Mutex<HashMap<String, Arc<RunningCheck>>>,
    /// Runs missed while Diskman was off, found at the start (`jobs::catch_up`); kept until the
    /// user runs or dismisses them, so a reload of the window asks again.
    pub missed: Mutex<Vec<MissedRun>>,
}

impl AppState {
    /// Opens the data folder (see `storage::data_dir`).
    pub fn load() -> anyhow::Result<Self> {
        Self::load_from(data_dir::locate()?)
    }

    pub fn load_from(dir: PathBuf) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&dir)?;
        let config = config_file::load(&dir);
        let history = History::open(&data_dir::database_path(&dir))?;
        if let Err(e) = history.import_legacy_status(&dir.join(data_dir::LEGACY_STATUS_FILE), &config) {
            eprintln!("Could not import {}: {e:#}", data_dir::LEGACY_STATUS_FILE);
        }
        let scan_cache = dir.join(data_dir::SCAN_CACHE_DIR);
        Ok(Self {
            data_dir: dir,
            config: Mutex::new(config),
            history,
            job: Mutex::new(JobProgress::default()),
            queue: Mutex::new(VecDeque::new()),
            cancel: AtomicBool::new(false),
            quitting: AtomicBool::new(false),
            scans: ScanCache::new(Some(scan_cache)),
            plans: Mutex::new(HashMap::new()),
            opened: Mutex::new(HashMap::new()),
            running: Mutex::new(None),
            checks: Mutex::new(HashMap::new()),
            missed: Mutex::new(Vec::new()),
        })
    }

    /// Saves the config. What belonged to backups or entries that are no longer in it goes along:
    /// see `forget`. (Their schedules and entries are part of the backup in the config itself.)
    pub fn save_config(&self, config: Config) -> anyhow::Result<()> {
        config_file::save(&self.data_dir, &config)?;
        let (removed, smaller_limit) = {
            let mut current = self.config.lock().unwrap();
            let removed = Removed::between(&current, &config);
            let smaller_limit = config.settings.activity_limit_mb < current.settings.activity_limit_mb;
            *current = config;
            (removed, smaller_limit)
        };
        self.forget(&removed);
        // Less room for the file lists of the activities: the oldest go right away
        if smaller_limit {
            self.prune_plans();
        }
        Ok(())
    }

    /// Removes everything outside the config that belongs to deleted backups and entries: a
    /// running sync of the backup is stopped (like "Cancel"), waiting runs leave the queue, and
    /// their history is deleted.
    fn forget(&self, removed: &Removed) {
        if removed.backups.is_empty() && removed.archives.is_empty() {
            return;
        }
        {
            // Lock order: job, then queue
            let job = self.job.lock().unwrap();
            if job.running && removed.backups.contains(&job.backup_id) {
                self.cancel.store(true, Ordering::Relaxed);
            }
            self.queue.lock().unwrap().retain(|q| !removed.backups.contains(&q.backup_id));
        }
        if let Err(e) = self.history.forget(&removed.backups, &removed.archives) {
            eprintln!("History of deleted backups not removed: {e:#}");
        }
    }

    /// Latest result per archive id. If the database cannot be read, there is simply no status.
    pub fn status(&self) -> HashMap<String, ArchiveStatus> {
        self.history.latest_status().unwrap_or_else(|e| {
            eprintln!("Status not readable: {e:#}");
            HashMap::new()
        })
    }

    // The history is secondary: write errors are only logged, the sync continues.

    /// Creates a run in the history; `None` if that failed.
    pub fn start_run(&self, backup_id: &str, backup_name: &str, source: &str, origin: &str) -> Option<i64> {
        self.history
            .start_run(backup_id, backup_name, source, origin)
            .map_err(|e| eprintln!("Run not stored in history: {e:#}"))
            .ok()
    }

    pub fn record_scan(&self, run: Option<i64>, scan_ms: u64) {
        if let Some(run) = run {
            if let Err(e) = self.history.set_scan_ms(run, scan_ms) {
                eprintln!("Scan duration not stored: {e:#}");
            }
        }
    }

    pub fn record_archive(&self, run: Option<i64>, result: &ArchiveRun) {
        if let Some(run) = run {
            if let Err(e) = self.history.record_archive(run, result) {
                eprintln!("Result for archive {} not stored: {e:#}", result.archive_id);
            }
            if result.plan.is_some() {
                self.prune_plans();
            }
        }
    }

    /// Stores an analysis ("Check") of a backup as an activity: the plans it made, or, if it
    /// failed, why. A cancelled analysis is not stored.
    pub fn record_check(&self, backup: &Backup, started_at: DateTime<Utc>, result: Result<&[Plan], &anyhow::Error>) {
        let results: Vec<ArchiveRun> = match result {
            Ok(plans) => plans
                .iter()
                .map(|p| ArchiveRun {
                    plan: plan_store::pack(p).map_err(|e| eprintln!("Plan not stored: {e:#}")).ok(),
                    ..ArchiveRun::checked(p, started_at)
                })
                .collect(),
            Err(e) => backup
                .archives
                .iter()
                .filter(|a| a.enabled)
                .map(|a| {
                    let message = format!("Check failed: {e:#}");
                    ArchiveRun::not_run(&a.id, &a.source, &a.path, Some(a.mode), Outcome::Failed, message)
                })
                .collect(),
        };
        let stored =
            self.history.record_check(&backup.id, &backup.name, &backup.sources(), "manual", started_at, &results);
        match stored {
            Ok(_) => self.prune_plans(),
            Err(e) => eprintln!("Check not stored in history: {e:#}"),
        }
    }

    /// The stored plans of an analysis take what its probe found (locked or unreadable files).
    pub fn record_probed(&self, plans: &[Plan]) {
        for p in plans {
            let stored = plan_store::pack(p)
                .and_then(|packed| self.history.update_plan(&packed, &p.summary, &p.summary.describe()));
            if let Err(e) = stored {
                eprintln!("Probed plan not stored: {e:#}");
            }
        }
        self.prune_plans();
    }

    /// Keeps the stored plans within `Settings::activity_limit_mb`, the oldest go first.
    fn prune_plans(&self) {
        let limit = self.config.lock().unwrap().settings.activity_limit_mb.saturating_mul(1024 * 1024);
        if let Err(e) = self.history.prune_plans(limit) {
            eprintln!("Old plans not removed: {e:#}");
        }
    }

    pub fn finish_run(&self, run: Option<i64>) {
        if let Some(run) = run {
            if let Err(e) = self.history.finish_run(run) {
                eprintln!("Run not finished: {e:#}");
            }
        }
    }

    /// Starts a job if none is running. At most one sync runs at a time.
    pub fn try_begin_job(&self, origin: &str) -> bool {
        let mut job = self.job.lock().unwrap();
        if job.running {
            return false;
        }
        *job = JobProgress {
            running: true,
            origin: origin.to_string(),
            started_at: Some(Utc::now()),
            ..Default::default()
        };
        self.cancel.store(false, Ordering::Relaxed);
        true
    }

    /// Cancels these backups: waiting ones leave the queue, a running one stops (like "Cancel"
    /// in the job bar, but only for these). Other backups go on.
    pub fn cancel_backups(&self, backup_ids: &[String]) {
        // Lock order: job, then queue
        let job = self.job.lock().unwrap();
        self.queue.lock().unwrap().retain(|q| !backup_ids.contains(&q.backup_id));
        if job.running && backup_ids.contains(&job.backup_id) {
            self.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Stops the checks of these backups, whether they are scanning or still waiting for their turn.
    pub fn cancel_checks(&self, backup_ids: &[String]) {
        let checks = self.checks.lock().unwrap();
        for id in backup_ids {
            if let Some(check) = checks.get(id) {
                check.stop.store(true, Ordering::Relaxed);
            }
        }
    }

    pub fn end_job(&self) {
        self.job.lock().unwrap().running = false;
    }
}

/// Ids of the backups and entries that a new config no longer has.
#[derive(Debug, Default, PartialEq)]
struct Removed {
    backups: Vec<String>,
    /// Entries of deleted backups as well as entries removed from a backup that remains
    archives: Vec<String>,
}

impl Removed {
    fn between(old: &Config, new: &Config) -> Self {
        let has_backup = |id: &str| new.backups.iter().any(|b| b.id == id);
        let has_archive = |id: &str| new.backups.iter().flat_map(|b| &b.archives).any(|a| a.id == id);
        Self {
            backups: old.backups.iter().filter(|b| !has_backup(&b.id)).map(|b| b.id.clone()).collect(),
            archives: old
                .backups
                .iter()
                .flat_map(|b| &b.archives)
                .filter(|a| !has_archive(&a.id))
                .map(|a| a.id.clone())
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::queue;
    use crate::model::Backup;
    use crate::storage::history::Outcome;

    fn backup(id: &str, archives: &[&str]) -> Backup {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "name": id,
            "archives": archives.iter().map(|a| serde_json::json!({
                "id": a, "source": format!("C:/{a}"), "path": format!("D:/{a}"), "mode": "mirror"
            })).collect::<Vec<_>>(),
            "schedule": { "when": { "kind": "interval", "minutes": 60 } }
        }))
        .unwrap()
    }

    #[test]
    fn deleting_a_backup_removes_what_belongs_to_it() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::load_from(dir.path().to_path_buf()).unwrap();
        let mut config =
            Config { backups: vec![backup("gone", &["g1"]), backup("kept", &["k1", "k2"])], ..Default::default() };
        state.save_config(config.clone()).unwrap();

        // History, a waiting run and a running sync of "gone"
        let run = state.start_run("gone", "gone", "C:/g1", "manual");
        state.record_archive(run, &ArchiveRun::not_run("g1", "C:/g1", "D:/g1", None, Outcome::Ok, "ok".into()));
        state.finish_run(run);
        let run = state.start_run("kept", "kept", "C:/k", "manual");
        for a in ["k1", "k2"] {
            state.record_archive(run, &ArchiveRun::not_run(a, "C:/k", "D:/k", None, Outcome::Ok, "ok".into()));
        }
        state.finish_run(run);
        queue::enqueue(&state, "kept", "manual");
        queue::enqueue(&state, "gone", "schedule");
        assert!(state.try_begin_job("manual"));
        state.job.lock().unwrap().backup_id = "gone".into();

        // Delete "gone" and the entry k2 of "kept"
        config.backups = vec![backup("kept", &["k1"])];
        state.save_config(config).unwrap();

        assert!(state.cancel.load(Ordering::Relaxed), "the running sync of the deleted backup stops");
        let queued: Vec<_> = state.queue.lock().unwrap().iter().map(|q| q.backup_id.clone()).collect();
        assert_eq!(queued, ["kept"]);
        let status = state.status();
        assert_eq!(status.keys().collect::<Vec<_>>(), ["k1"]);
        // On disk as well: the schedule and entries were part of the backup
        let saved = config_file::load(dir.path());
        assert_eq!(saved.backups.len(), 1);
        assert_eq!(saved.backups[0].archives.len(), 1);
    }

    #[test]
    fn cancelling_backups_stops_only_those() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::load_from(dir.path().to_path_buf()).unwrap();
        for id in ["a", "b", "c"] {
            queue::enqueue(&state, id, "manual");
        }
        assert!(state.try_begin_job("manual"));
        state.job.lock().unwrap().backup_id = "running".into();

        // Waiting ones leave the queue, the running one goes on
        state.cancel_backups(&["a".into(), "c".into()]);
        let queued: Vec<_> = state.queue.lock().unwrap().iter().map(|q| q.backup_id.clone()).collect();
        assert_eq!(queued, ["b"]);
        assert!(!state.cancel.load(Ordering::Relaxed));

        // The running one stops, the waiting one stays
        state.cancel_backups(&["running".into()]);
        assert!(state.cancel.load(Ordering::Relaxed));
        assert_eq!(state.queue.lock().unwrap().len(), 1);
    }

    #[test]
    fn saving_without_deletions_stops_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::load_from(dir.path().to_path_buf()).unwrap();
        let mut config = Config { backups: vec![backup("a", &["a1"])], ..Default::default() };
        state.save_config(config.clone()).unwrap();
        assert!(state.try_begin_job("manual"));
        state.job.lock().unwrap().backup_id = "a".into();
        config.backups[0].name = "renamed".into();
        state.save_config(config).unwrap();
        assert!(!state.cancel.load(Ordering::Relaxed));
    }
}
