//! History of all sync runs in a SQLite database (`diskman.db` in the data folder).
//!
//! A **run** (`runs`) is one pass of a backup; it has one entry per archive in `run_archives`
//! with durations and all numbers of the summary. The "last run …" status in the UI is the
//! latest `run_archives` entry per archive. The configuration stays in `config.json`.
//!
//! The history is secondary: if a write fails, it is logged and the sync continues.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Mutex;

use anyhow::Context;
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{params, Connection};

use serde::{Deserialize, Serialize};

use super::plan_store::PackedPlan;
use crate::model::{Config, Mode};
use crate::sync::issues::{Issue, Object, Reason};
use crate::sync::planner::{Plan, Summary};

/// This many runs are kept per backup; older ones are deleted when a run finishes.
const KEEP_RUNS_PER_BACKUP: i64 = 500;
/// This many error messages are stored per archive run.
pub const MAX_STORED_ERRORS: usize = 200;

/// Schema versions in order; index + 1 is the version (`PRAGMA user_version`).
/// Only append new versions, never change existing ones.
const MIGRATIONS: &[&str] = &[
    // 1: runs, archive results, error messages
    "CREATE TABLE runs (
        id          INTEGER PRIMARY KEY,
        backup_id   TEXT    NOT NULL,
        backup_name TEXT    NOT NULL,   -- as of the run, so the history stays readable
        source      TEXT    NOT NULL,
        origin      TEXT    NOT NULL,   -- manual | auto | schedule | preview | import
        started_at  INTEGER NOT NULL,   -- Unix time in ms (UTC)
        finished_at INTEGER,            -- NULL while running or after a crash
        outcome     TEXT,               -- ok | errors | cancelled | failed | interrupted
        scan_ms     INTEGER             -- scan of source and archives (together, in parallel)
    );
    CREATE INDEX runs_backup ON runs (backup_id, id);

    CREATE TABLE run_archives (
        id             INTEGER PRIMARY KEY,
        run_id         INTEGER NOT NULL REFERENCES runs (id) ON DELETE CASCADE,
        archive_id     TEXT    NOT NULL,
        dest           TEXT    NOT NULL,
        mode           TEXT,               -- mirror | additive
        outcome        TEXT    NOT NULL,   -- ok | errors | cancelled | skipped | failed | interrupted
        message        TEXT    NOT NULL,   -- text for the UI
        started_at     INTEGER NOT NULL,
        finished_at    INTEGER NOT NULL,
        diff_ms        INTEGER,
        copy_ms        INTEGER,            -- execution of the operations
        ops_total      INTEGER NOT NULL DEFAULT 0,
        ops_done       INTEGER NOT NULL DEFAULT 0,
        bytes_total    INTEGER NOT NULL DEFAULT 0,
        bytes_done     INTEGER NOT NULL DEFAULT 0,
        copy_files     INTEGER NOT NULL DEFAULT 0,
        copy_bytes     INTEGER NOT NULL DEFAULT 0,
        update_files   INTEGER NOT NULL DEFAULT 0,
        update_bytes   INTEGER NOT NULL DEFAULT 0,
        move_files     INTEGER NOT NULL DEFAULT 0,
        move_bytes     INTEGER NOT NULL DEFAULT 0,
        dirs_moved     INTEGER NOT NULL DEFAULT 0,
        delete_files   INTEGER NOT NULL DEFAULT 0,
        delete_bytes   INTEGER NOT NULL DEFAULT 0,
        dirs_created   INTEGER NOT NULL DEFAULT 0,
        dirs_deleted   INTEGER NOT NULL DEFAULT 0,
        unchanged      INTEGER NOT NULL DEFAULT 0,
        protected      INTEGER NOT NULL DEFAULT 0,
        error_count    INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX run_archives_run ON run_archives (run_id);
    CREATE INDEX run_archives_archive ON run_archives (archive_id, id);

    CREATE TABLE run_errors (
        run_archive_id INTEGER NOT NULL REFERENCES run_archives (id) ON DELETE CASCADE,
        seq            INTEGER NOT NULL,
        message        TEXT    NOT NULL,
        PRIMARY KEY (run_archive_id, seq)
    ) WITHOUT ROWID;",
    // 2: what failed and why, so the UI can explain it (older rows only have the message)
    "ALTER TABLE run_errors ADD COLUMN path   TEXT;
    ALTER TABLE run_errors ADD COLUMN reason TEXT;      -- sync::issues::Reason
    ALTER TABLE run_errors ADD COLUMN object TEXT;      -- file | folder | link
    ALTER TABLE run_errors ADD COLUMN size   INTEGER NOT NULL DEFAULT 0;",
    // 3: log of the schedules that catch up missed runs (jobs::catch_up); runs started for them
    // have the origin catch_up
    "CREATE TABLE schedule_log (
        backup_id  TEXT    PRIMARY KEY,
        checked_at INTEGER NOT NULL,   -- the scheduler watched the schedule until here (Unix ms, UTC)
        counted_at INTEGER NOT NULL    -- intervals count from here: last run or start of Diskman
    ) WITHOUT ROWID;",
    // 4: activities. Analyses are stored as well (kind = check; they never count as the last run of
    // an archive), each entry with its source and its plan (`storage::plan_store`)
    "ALTER TABLE runs ADD COLUMN kind TEXT NOT NULL DEFAULT 'sync';   -- sync | check
    DROP INDEX runs_backup;
    CREATE INDEX runs_backup ON runs (backup_id, kind, id);
    ALTER TABLE run_archives ADD COLUMN source  TEXT;                       -- NULL before schema 4: see runs.source
    ALTER TABLE run_archives ADD COLUMN blocked INTEGER NOT NULL DEFAULT 0; -- entries the plan could not transfer
    CREATE TABLE run_plans (
        run_archive_id INTEGER PRIMARY KEY REFERENCES run_archives (id) ON DELETE CASCADE,
        plan_id        TEXT    NOT NULL,
        size           INTEGER NOT NULL,   -- bytes of data
        data           BLOB    NOT NULL    -- the plan as Deflate-compressed JSON
    );
    CREATE INDEX run_plans_plan ON run_plans (plan_id);",
];

/// Runs that synced (`runs.kind`); only these give an archive its status.
const SYNC: &str = "sync";
/// Analyses ("Check"): stored as activities, never as the last run of an archive.
const CHECK: &str = "check";

/// Logged state of a schedule that catches up missed runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduleLog {
    /// Until here the scheduler was watching (Diskman was running)
    pub checked_at: DateTime<Utc>,
    /// Start of the interval: the last scheduled run or the start of Diskman
    pub counted_at: DateTime<Utc>,
}

/// Latest result of an archive, for the "last run …" line in the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveStatus {
    pub last_run: DateTime<Utc>,
    pub ok: bool,
    pub message: String,
}

/// Result of an archive within a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    /// Completed, but some operations failed
    Errors,
    Cancelled,
    /// Not executed (safety stop)
    Skipped,
    /// Could not start at all (e.g. scan failed)
    Failed,
    /// Diskman ended during the run (quit, crash, power cut); found at the next start
    Interrupted,
}

impl Outcome {
    fn as_str(self) -> &'static str {
        match self {
            Outcome::Ok => "ok",
            Outcome::Errors => "errors",
            Outcome::Cancelled => "cancelled",
            Outcome::Skipped => "skipped",
            Outcome::Failed => "failed",
            Outcome::Interrupted => "interrupted",
        }
    }
}

/// Everything stored for an archive run.
#[derive(Debug, Clone)]
pub struct ArchiveRun {
    pub archive_id: String,
    pub source: String,
    pub dest: String,
    pub mode: Option<Mode>,
    pub outcome: Outcome,
    pub message: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub diff_ms: Option<u64>,
    pub copy_ms: Option<u64>,
    pub ops_total: u64,
    pub ops_done: u64,
    pub bytes_total: u64,
    pub bytes_done: u64,
    pub summary: Summary,
    pub error_count: u64,
    pub errors: Vec<Issue>,
    /// The plan that was carried out (or would have been), for the activities; None if planning
    /// failed or it could not be packed
    pub plan: Option<PackedPlan>,
}

/// Latest result of an archive that did not end well, with what failed (for the notice after a
/// run and its details).
#[derive(Debug, Clone, Serialize)]
pub struct RunProblem {
    pub archive_id: String,
    pub dest: String,
    /// errors | cancelled | skipped | failed
    pub outcome: String,
    pub message: String,
    pub finished_at: DateTime<Utc>,
    /// All failures; `issues` holds at most `MAX_STORED_ERRORS` of them
    pub error_count: u64,
    pub issues: Vec<Issue>,
}

impl ArchiveRun {
    /// Entry for an archive that was not executed.
    pub fn not_run(
        archive_id: &str,
        source: &str,
        dest: &str,
        mode: Option<Mode>,
        outcome: Outcome,
        message: String,
    ) -> Self {
        let now = Utc::now();
        ArchiveRun {
            archive_id: archive_id.to_string(),
            source: source.to_string(),
            dest: dest.to_string(),
            mode,
            outcome,
            message,
            started_at: now,
            finished_at: now,
            diff_ms: None,
            copy_ms: None,
            ops_total: 0,
            ops_done: 0,
            bytes_total: 0,
            bytes_done: 0,
            summary: Summary::default(),
            error_count: 0,
            errors: Vec::new(),
            plan: None,
        }
    }

    /// Entry for an analysis of an archive: what its plan would do.
    pub fn checked(plan: &Plan, started_at: DateTime<Utc>) -> Self {
        let message = if plan.dangerous {
            format!("Source is empty (safety stop) · {}", plan.summary.describe())
        } else {
            plan.summary.describe()
        };
        ArchiveRun {
            archive_id: plan.archive_id.clone(),
            source: plan.source.clone(),
            dest: plan.dest.clone(),
            mode: Some(plan.mode),
            outcome: if plan.dangerous { Outcome::Skipped } else { Outcome::Ok },
            message,
            started_at,
            finished_at: plan.created_at,
            diff_ms: Some(plan.diff_ms),
            copy_ms: None,
            ops_total: plan.ops.len() as u64,
            ops_done: 0,
            bytes_total: plan.summary.copy_bytes + plan.summary.update_bytes,
            bytes_done: 0,
            summary: plan.summary.clone(),
            error_count: 0,
            errors: Vec::new(),
            plan: None,
        }
    }
}

/// One activity: an analysis or a run of a backup, newest first in the list.
#[derive(Debug, Clone, Serialize)]
pub struct Activity {
    pub id: i64,
    /// sync | check
    pub kind: String,
    /// manual | schedule | catch_up | import
    pub origin: String,
    pub backup_id: String,
    /// As of the activity
    pub backup_name: String,
    pub started_at: DateTime<Utc>,
    /// None while it runs
    pub finished_at: Option<DateTime<Utc>>,
    /// ok | errors | cancelled | failed | interrupted; None while it runs
    pub outcome: Option<String>,
    pub scan_ms: Option<u64>,
    pub entries: Vec<ActivityEntry>,
}

/// One entry (source → archive) of an activity.
#[derive(Debug, Clone, Serialize)]
pub struct ActivityEntry {
    /// Id of its result (`run_archives`), to open its plan and errors
    pub id: i64,
    pub archive_id: String,
    pub source: String,
    pub dest: String,
    pub mode: Option<Mode>,
    /// ok | errors | cancelled | skipped | failed | interrupted
    pub outcome: String,
    pub message: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub diff_ms: Option<u64>,
    pub copy_ms: Option<u64>,
    pub ops_total: u64,
    pub ops_done: u64,
    pub bytes_total: u64,
    pub bytes_done: u64,
    pub summary: Summary,
    pub error_count: u64,
    /// Its file list is still stored (`run_plans`; the oldest go when the limit is reached)
    pub has_plan: bool,
}

/// Which activities to list.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ActivityQuery {
    /// Only of this backup
    pub backup_id: Option<String>,
    /// Only sync or check
    pub kind: Option<String>,
    /// Only older than this activity (the next page)
    pub before: Option<i64>,
    pub limit: u32,
}

/// What the stored file lists take.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct PlanStorage {
    pub bytes: u64,
    pub plans: u64,
}

pub struct History {
    conn: Mutex<Connection>,
}

impl History {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path).with_context(|| format!("cannot open {}", path.display()))?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn in_memory() -> Self {
        Self::init(Connection::open_in_memory().unwrap()).unwrap()
    }

    fn init(mut conn: Connection) -> anyhow::Result<Self> {
        // WAL: reading (status query every second) does not block writing and vice versa
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrate(&mut conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Creates a run and returns its id.
    pub fn start_run(&self, backup_id: &str, backup_name: &str, source: &str, origin: &str) -> anyhow::Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO runs (backup_id, backup_name, source, origin, started_at, kind) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![backup_id, backup_name, source, origin, ms(Utc::now()), SYNC],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Finishes the runs that never ended because Diskman ended during them (quit, crash, power
    /// cut): every enabled entry of the backup without a result gets "interrupted", so its card
    /// no longer shows the result of an earlier run as if it were current. Call only at the start,
    /// before any run begins. Returns the number of runs closed.
    pub fn close_interrupted(&self, config: &Config) -> anyhow::Result<usize> {
        let runs: Vec<(i64, String)> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare("SELECT id, backup_id FROM runs WHERE finished_at IS NULL AND kind = ?1")?;
            let rows = stmt.query_map([SYNC], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<Result<_, _>>()?
        };
        for (run, backup_id) in &runs {
            let done: Vec<String> = {
                let conn = self.conn.lock().unwrap();
                let mut stmt = conn.prepare("SELECT archive_id FROM run_archives WHERE run_id = ?1")?;
                let rows = stmt.query_map([run], |r| r.get(0))?;
                rows.collect::<Result<_, _>>()?
            };
            if let Some(backup) = config.backups.iter().find(|b| &b.id == backup_id) {
                for a in backup.archives.iter().filter(|a| a.enabled && !done.contains(&a.id)) {
                    self.record_archive(
                        *run,
                        &ArchiveRun::not_run(
                            &a.id,
                            &a.source,
                            &a.path,
                            Some(a.mode),
                            Outcome::Interrupted,
                            "Interrupted: Diskman ended during the run. The next run completes it.".into(),
                        ),
                    )?;
                }
            }
            self.finish_run(*run)?;
        }
        Ok(runs.len())
    }

    pub fn set_scan_ms(&self, run_id: i64, scan_ms: u64) -> anyhow::Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("UPDATE runs SET scan_ms = ?2 WHERE id = ?1", params![run_id, scan_ms as i64])?;
        Ok(())
    }

    pub fn record_archive(&self, run_id: i64, r: &ArchiveRun) -> anyhow::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        insert_archive(&tx, run_id, r)?;
        tx.commit()?;
        Ok(())
    }

    /// Stores a finished analysis of a backup as one activity (kind check) with the result and the
    /// plan of each entry, all at once. Returns its id.
    pub fn record_check(
        &self,
        backup_id: &str,
        backup_name: &str,
        source: &str,
        origin: &str,
        started_at: DateTime<Utc>,
        results: &[ArchiveRun],
    ) -> anyhow::Result<i64> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO runs (backup_id, backup_name, source, origin, started_at, kind) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![backup_id, backup_name, source, origin, ms(started_at), CHECK],
        )?;
        let run_id = tx.last_insert_rowid();
        for r in results {
            insert_archive(&tx, run_id, r)?;
        }
        close_run(&tx, run_id)?;
        tx.commit()?;
        Ok(run_id)
    }

    /// Replaces the stored plan `plan.plan_id` and the figures of its entry: the plan of an
    /// analysis after its probe found locked or unreadable files. Nothing happens if it is no
    /// longer stored.
    pub fn update_plan(&self, plan: &PackedPlan, summary: &Summary, message: &str) -> anyhow::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let id = match tx.query_row("SELECT run_archive_id FROM run_plans WHERE plan_id = ?1", [&plan.plan_id], |r| {
            r.get::<_, i64>(0)
        }) {
            Ok(id) => id,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        tx.execute(
            "UPDATE run_plans SET size = ?2, data = ?3 WHERE run_archive_id = ?1",
            params![id, plan.data.len() as i64, plan.data],
        )?;
        let s = summary;
        tx.execute(
            "UPDATE run_archives SET message = ?2,
                bytes_total = ?3, copy_files = ?4, copy_bytes = ?5, update_files = ?6, update_bytes = ?7,
                move_files = ?8, move_bytes = ?9, dirs_moved = ?10, delete_files = ?11, delete_bytes = ?12,
                dirs_created = ?13, dirs_deleted = ?14, unchanged = ?15, protected = ?16, blocked = ?17
             WHERE id = ?1",
            params![
                id,
                message,
                (s.copy_bytes + s.update_bytes) as i64,
                s.copy_files as i64,
                s.copy_bytes as i64,
                s.update_files as i64,
                s.update_bytes as i64,
                s.move_files as i64,
                s.move_bytes as i64,
                s.dirs_moved as i64,
                s.delete_files as i64,
                s.delete_bytes as i64,
                s.dirs_created as i64,
                s.dirs_deleted as i64,
                s.unchanged as i64,
                s.protected as i64,
                s.blocked as i64,
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Deletes the oldest stored plans until all of them together take at most `limit` bytes.
    /// The activities themselves stay, only without their file list. Returns how many went.
    pub fn prune_plans(&self, limit: u64) -> anyhow::Result<usize> {
        let conn = self.conn.lock().unwrap();
        let deleted = conn.execute(
            "DELETE FROM run_plans WHERE run_archive_id IN (
                SELECT run_archive_id FROM (
                    SELECT run_archive_id, SUM(size) OVER (ORDER BY run_archive_id DESC) AS total FROM run_plans
                ) WHERE total > ?1)",
            [limit.min(i64::MAX as u64) as i64],
        )?;
        Ok(deleted)
    }

    /// Deletes every stored plan; the activities stay.
    pub fn clear_plans(&self) -> anyhow::Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM run_plans", [])?;
        Ok(())
    }

    /// Deletes the whole history: every analysis and run with its results, errors and plans
    /// (ON DELETE CASCADE). A run still going on stays, it records its results into it; so do the
    /// logs of the schedules, which time the next runs. Returns the number of activities deleted.
    pub fn clear_activities(&self) -> anyhow::Result<usize> {
        let deleted = self.conn.lock().unwrap().execute("DELETE FROM runs WHERE finished_at IS NOT NULL", [])?;
        Ok(deleted)
    }

    /// Size and number of the stored plans.
    pub fn plan_storage(&self) -> anyhow::Result<PlanStorage> {
        let conn = self.conn.lock().unwrap();
        let (bytes, plans): (i64, i64) =
            conn.query_row("SELECT COALESCE(SUM(size), 0), COUNT(*) FROM run_plans", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        Ok(PlanStorage { bytes: bytes as u64, plans: plans as u64 })
    }

    /// The activities, newest first, each with its entries.
    pub fn activities(&self, q: &ActivityQuery) -> anyhow::Result<Vec<Activity>> {
        let conn = self.conn.lock().unwrap();
        let mut runs = conn.prepare_cached(
            "SELECT id, kind, origin, backup_id, backup_name, started_at, finished_at, outcome, scan_ms, source
             FROM runs
             WHERE (?1 IS NULL OR backup_id = ?1) AND (?2 IS NULL OR kind = ?2) AND (?3 IS NULL OR id < ?3)
             ORDER BY id DESC LIMIT ?4",
        )?;
        let mut entries = conn.prepare_cached(
            "SELECT ra.id, ra.archive_id, ra.source, ra.dest, ra.mode, ra.outcome, ra.message, ra.started_at,
                    ra.finished_at, ra.diff_ms, ra.copy_ms, ra.ops_total, ra.ops_done, ra.bytes_total, ra.bytes_done,
                    ra.copy_files, ra.copy_bytes, ra.update_files, ra.update_bytes, ra.move_files, ra.move_bytes,
                    ra.dirs_moved, ra.delete_files, ra.delete_bytes, ra.dirs_created, ra.dirs_deleted, ra.unchanged,
                    ra.protected, ra.blocked, ra.error_count, p.run_archive_id IS NOT NULL
             FROM run_archives ra LEFT JOIN run_plans p ON p.run_archive_id = ra.id
             WHERE ra.run_id = ?1 ORDER BY ra.id",
        )?;
        let limit = q.limit.clamp(1, 500) as i64;
        let found = runs
            .query_map(params![q.backup_id, q.kind, q.before, limit], |r| {
                Ok((
                    Activity {
                        id: r.get(0)?,
                        kind: r.get(1)?,
                        origin: r.get(2)?,
                        backup_id: r.get(3)?,
                        backup_name: r.get(4)?,
                        started_at: from_ms(r.get(5)?),
                        finished_at: r.get::<_, Option<i64>>(6)?.map(from_ms),
                        outcome: r.get(7)?,
                        scan_ms: r.get::<_, Option<i64>>(8)?.map(count),
                        entries: Vec::new(),
                    },
                    r.get::<_, String>(9)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut out = Vec::with_capacity(found.len());
        for (mut activity, run_source) in found {
            activity.entries = entries
                .query_map([activity.id], |r| {
                    Ok(ActivityEntry {
                        id: r.get(0)?,
                        archive_id: r.get(1)?,
                        // Before schema 4 only the run knew its sources
                        source: r.get::<_, Option<String>>(2)?.unwrap_or_else(|| run_source.clone()),
                        dest: r.get(3)?,
                        mode: r.get::<_, Option<String>>(4)?.as_deref().map(parse_mode),
                        outcome: r.get(5)?,
                        message: r.get(6)?,
                        started_at: from_ms(r.get(7)?),
                        finished_at: from_ms(r.get(8)?),
                        diff_ms: r.get::<_, Option<i64>>(9)?.map(count),
                        copy_ms: r.get::<_, Option<i64>>(10)?.map(count),
                        ops_total: count(r.get(11)?),
                        ops_done: count(r.get(12)?),
                        bytes_total: count(r.get(13)?),
                        bytes_done: count(r.get(14)?),
                        summary: Summary {
                            copy_files: count(r.get(15)?),
                            copy_bytes: count(r.get(16)?),
                            update_files: count(r.get(17)?),
                            update_bytes: count(r.get(18)?),
                            move_files: count(r.get(19)?),
                            move_bytes: count(r.get(20)?),
                            dirs_moved: count(r.get(21)?),
                            delete_files: count(r.get(22)?),
                            delete_bytes: count(r.get(23)?),
                            dirs_created: count(r.get(24)?),
                            dirs_deleted: count(r.get(25)?),
                            unchanged: count(r.get(26)?),
                            protected: count(r.get(27)?),
                            blocked: count(r.get(28)?),
                        },
                        error_count: count(r.get(29)?),
                        has_plan: r.get(30)?,
                    })
                })?
                .collect::<Result<_, _>>()?;
            out.push(activity);
        }
        Ok(out)
    }

    /// The stored plans of an activity, by the id of their entry.
    pub fn stored_plans(&self, run_id: i64) -> anyhow::Result<Vec<(i64, Vec<u8>)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT p.run_archive_id, p.data FROM run_plans p JOIN run_archives ra ON ra.id = p.run_archive_id
             WHERE ra.run_id = ?1 ORDER BY ra.id",
        )?;
        let rows = stmt.query_map([run_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The stored failures of an entry of an activity (at most `MAX_STORED_ERRORS`).
    pub fn entry_errors(&self, run_archive_id: i64) -> anyhow::Result<Vec<Issue>> {
        let conn = self.conn.lock().unwrap();
        read_errors(&conn, run_archive_id)
    }

    /// Finishes a run: overall result from its archives, then clean up old runs.
    pub fn finish_run(&self, run_id: i64) -> anyhow::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        close_run(&tx, run_id)?;
        tx.commit()?;
        Ok(())
    }

    /// Deletes the history of deleted backups (their runs with everything in them) and of deleted
    /// entries (their results in the runs of the backups that remain).
    pub fn forget(&self, backups: &[String], archives: &[String]) -> anyhow::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            // Results and error messages go along (ON DELETE CASCADE)
            let mut runs = tx.prepare("DELETE FROM runs WHERE backup_id = ?1")?;
            for id in backups {
                runs.execute([id])?;
            }
            let mut results = tx.prepare("DELETE FROM run_archives WHERE archive_id = ?1")?;
            for id in archives {
                results.execute([id])?;
            }
            let mut log = tx.prepare("DELETE FROM schedule_log WHERE backup_id = ?1")?;
            for id in backups {
                log.execute([id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The logged schedules, by backup id.
    pub fn schedule_log(&self) -> anyhow::Result<HashMap<String, ScheduleLog>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached("SELECT backup_id, checked_at, counted_at FROM schedule_log")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, ScheduleLog { checked_at: from_ms(r.get(1)?), counted_at: from_ms(r.get(2)?) }))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Replaces the log with these schedules: theirs is updated, all others are dropped (their
    /// backup no longer catches up, so an old entry must not count on the next start).
    pub fn write_schedule_log(&self, entries: &[(String, ScheduleLog)]) -> anyhow::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM schedule_log", [])?;
        {
            let mut stmt =
                tx.prepare("INSERT INTO schedule_log (backup_id, checked_at, counted_at) VALUES (?1, ?2, ?3)")?;
            for (id, log) in entries {
                stmt.execute(params![id, ms(log.checked_at), ms(log.counted_at)])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Start of the latest run of a backup (any origin, analyses do not count), if it has one.
    pub fn last_run_at(&self, backup_id: &str) -> anyhow::Result<Option<DateTime<Utc>>> {
        let conn = self.conn.lock().unwrap();
        let at: Option<i64> = conn.query_row(
            "SELECT MAX(started_at) FROM runs WHERE backup_id = ?1 AND kind = ?2",
            params![backup_id, SYNC],
            |r| r.get(0),
        )?;
        Ok(at.map(from_ms))
    }

    /// Latest result per archive, for the status display and the timing of automatic syncs.
    pub fn latest_status(&self) -> anyhow::Result<HashMap<String, ArchiveStatus>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT archive_id, finished_at, outcome, message FROM run_archives
             WHERE id IN (SELECT MAX(ra.id) FROM run_archives ra JOIN runs r ON r.id = ra.run_id
                          WHERE r.kind = ?1 GROUP BY ra.archive_id)",
        )?;
        let rows = stmt.query_map([SYNC], |row| {
            let outcome: String = row.get(2)?;
            Ok((
                row.get::<_, String>(0)?,
                ArchiveStatus { last_run: from_ms(row.get(1)?), ok: outcome == "ok", message: row.get(3)? },
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Latest result of each of these archives, if it was not ok, with its failures.
    pub fn problems(&self, archive_ids: &[String]) -> anyhow::Result<Vec<RunProblem>> {
        let conn = self.conn.lock().unwrap();
        let mut latest = conn.prepare_cached(
            "SELECT ra.id, ra.dest, ra.outcome, ra.message, ra.finished_at, ra.error_count
             FROM run_archives ra JOIN runs r ON r.id = ra.run_id
             WHERE ra.archive_id = ?1 AND r.kind = ?2 ORDER BY ra.id DESC LIMIT 1",
        )?;
        let mut out = Vec::new();
        for archive_id in archive_ids {
            let row = latest.query_row(params![archive_id, SYNC], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                ))
            });
            let (id, dest, outcome, message, finished_at, error_count) = match row {
                Ok(row) => row,
                Err(rusqlite::Error::QueryReturnedNoRows) => continue,
                Err(e) => return Err(e.into()),
            };
            if outcome == "ok" {
                continue;
            }
            let issues = read_errors(&conn, id)?;
            out.push(RunProblem {
                archive_id: archive_id.clone(),
                dest,
                outcome,
                message,
                finished_at: from_ms(finished_at),
                error_count: error_count as u64,
                issues,
            });
        }
        Ok(out)
    }

    /// Imports a `status.json` from versions before the history, once, and renames it afterwards.
    /// Entries whose archive no longer exists in the config are dropped.
    pub fn import_legacy_status(&self, path: &Path, config: &Config) -> anyhow::Result<()> {
        if !path.exists() {
            return Ok(());
        }
        let status: HashMap<String, ArchiveStatus> =
            serde_json::from_str(&fs::read_to_string(path)?).unwrap_or_default();
        {
            let mut conn = self.conn.lock().unwrap();
            let tx = conn.transaction()?;
            let already: bool =
                tx.query_row("SELECT EXISTS (SELECT 1 FROM runs WHERE origin = 'import')", [], |r| r.get(0))?;
            if !already {
                for backup in &config.backups {
                    for archive in &backup.archives {
                        let Some(st) = status.get(&archive.id) else { continue };
                        let at = ms(st.last_run);
                        let outcome = if st.ok { "ok" } else { "failed" };
                        tx.execute(
                            "INSERT INTO runs (backup_id, backup_name, source, origin, started_at, finished_at, outcome)
                             VALUES (?1, ?2, ?3, 'import', ?4, ?4, ?5)",
                            params![backup.id, backup.name, archive.source, at, outcome],
                        )?;
                        let run_id = tx.last_insert_rowid();
                        tx.execute(
                            "INSERT INTO run_archives (run_id, archive_id, dest, mode, outcome, message, started_at, finished_at)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                            params![run_id, archive.id, archive.path, mode_str(archive.mode), outcome, st.message, at],
                        )?;
                    }
                }
            }
            tx.commit()?;
        }
        fs::rename(path, path.with_extension("json.migrated"))?;
        Ok(())
    }
}

fn migrate(conn: &mut Connection) -> anyhow::Result<()> {
    let version: usize = conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))? as usize;
    if version > MIGRATIONS.len() {
        anyhow::bail!(
            "The history database comes from a newer Diskman version (schema {version}, supported up to {}).",
            MIGRATIONS.len()
        );
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql).with_context(|| format!("migration to schema {} failed", i + 1))?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

/// Inserts the result of an entry, its failures and its plan; returns its id.
fn insert_archive(conn: &Connection, run_id: i64, r: &ArchiveRun) -> anyhow::Result<i64> {
    let s = &r.summary;
    conn.execute(
        "INSERT INTO run_archives (
            run_id, archive_id, source, dest, mode, outcome, message, started_at, finished_at, diff_ms, copy_ms,
            ops_total, ops_done, bytes_total, bytes_done,
            copy_files, copy_bytes, update_files, update_bytes, move_files, move_bytes, dirs_moved,
            delete_files, delete_bytes, dirs_created, dirs_deleted, unchanged, protected, blocked, error_count
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19,
                  ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30)",
        params![
            run_id,
            r.archive_id,
            r.source,
            r.dest,
            r.mode.map(mode_str),
            r.outcome.as_str(),
            r.message,
            ms(r.started_at),
            ms(r.finished_at),
            r.diff_ms.map(|v| v as i64),
            r.copy_ms.map(|v| v as i64),
            r.ops_total as i64,
            r.ops_done as i64,
            r.bytes_total as i64,
            r.bytes_done as i64,
            s.copy_files as i64,
            s.copy_bytes as i64,
            s.update_files as i64,
            s.update_bytes as i64,
            s.move_files as i64,
            s.move_bytes as i64,
            s.dirs_moved as i64,
            s.delete_files as i64,
            s.delete_bytes as i64,
            s.dirs_created as i64,
            s.dirs_deleted as i64,
            s.unchanged as i64,
            s.protected as i64,
            s.blocked as i64,
            r.error_count as i64,
        ],
    )?;
    let id = conn.last_insert_rowid();
    let mut stmt = conn.prepare_cached(
        "INSERT INTO run_errors (run_archive_id, seq, message, path, reason, object, size)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for (i, issue) in r.errors.iter().take(MAX_STORED_ERRORS).enumerate() {
        stmt.execute(params![
            id,
            i as i64,
            issue.detail.as_deref().unwrap_or_default(),
            issue.path,
            issue.reason.as_str(),
            issue.object.as_str(),
            issue.size as i64,
        ])?;
    }
    if let Some(plan) = &r.plan {
        conn.execute(
            "INSERT INTO run_plans (run_archive_id, plan_id, size, data) VALUES (?1, ?2, ?3, ?4)",
            params![id, plan.plan_id, plan.data.len() as i64, plan.data],
        )?;
    }
    Ok(id)
}

/// Ends a run (or analysis): its result is the worst of its entries; without entries (e.g.
/// cancelled right away) it counts as cancelled. Then the oldest runs of each kind of its backup
/// beyond `KEEP_RUNS_PER_BACKUP` go.
fn close_run(conn: &Connection, run_id: i64) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE runs SET finished_at = ?2, outcome = COALESCE((
            SELECT CASE
                WHEN SUM(outcome = 'failed')    > 0 THEN 'failed'
                WHEN SUM(outcome = 'interrupted') > 0 THEN 'interrupted'
                WHEN SUM(outcome = 'cancelled') > 0 THEN 'cancelled'
                WHEN SUM(outcome IN ('errors', 'skipped')) > 0 THEN 'errors'
                ELSE 'ok' END
            FROM run_archives WHERE run_id = ?1 HAVING COUNT(*) > 0), 'cancelled')
         WHERE id = ?1",
        params![run_id, ms(Utc::now())],
    )?;
    conn.execute(
        "DELETE FROM runs WHERE id IN (
            SELECT id FROM (
                SELECT id, ROW_NUMBER() OVER (PARTITION BY backup_id, kind ORDER BY id DESC) AS rn FROM runs
            ) WHERE rn > ?1)",
        params![KEEP_RUNS_PER_BACKUP],
    )?;
    Ok(())
}

/// The stored failures of an entry, in their order.
fn read_errors(conn: &Connection, run_archive_id: i64) -> anyhow::Result<Vec<Issue>> {
    let mut stmt = conn.prepare_cached(
        "SELECT message, path, reason, object, size FROM run_errors WHERE run_archive_id = ?1 ORDER BY seq",
    )?;
    let rows = stmt.query_map([run_archive_id], |r| {
        let message: String = r.get(0)?;
        let path: Option<String> = r.get(1)?;
        let reason: Option<String> = r.get(2)?;
        let object: Option<String> = r.get(3)?;
        let size: i64 = r.get(4)?;
        Ok(stored_issue(message, path, reason, object, size))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// A count or size from the database (stored as INTEGER).
fn count(v: i64) -> u64 {
    v.max(0) as u64
}

/// A stored failure. Rows from before schema 2 only have "path: error message".
fn stored_issue(
    message: String,
    path: Option<String>,
    reason: Option<String>,
    object: Option<String>,
    size: i64,
) -> Issue {
    let (path, reason, detail) = match (path, reason) {
        (Some(path), Some(reason)) => (path, Reason::parse(&reason), message),
        _ => match message.split_once(": ") {
            Some((path, msg)) => (path.to_string(), Reason::Other, msg.to_string()),
            None => (String::new(), Reason::Other, message),
        },
    };
    let issue = Issue::new(path, object.as_deref().map_or(Object::File, Object::parse), reason).with_size(size as u64);
    if detail.is_empty() {
        issue
    } else {
        issue.with_detail(detail)
    }
}

fn parse_mode(s: &str) -> Mode {
    match s {
        "additive" => Mode::Additive,
        _ => Mode::Mirror,
    }
}

fn mode_str(mode: Mode) -> &'static str {
    match mode {
        Mode::Mirror => "mirror",
        Mode::Additive => "additive",
    }
}

fn ms(t: DateTime<Utc>) -> i64 {
    t.timestamp_millis()
}

fn from_ms(v: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(v).single().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::config::Archive;
    use crate::model::Backup;

    fn archive_run(archive_id: &str, outcome: Outcome, message: &str) -> ArchiveRun {
        ArchiveRun::not_run(archive_id, "C:/f", "D:/a", Some(Mode::Mirror), outcome, message.into())
    }

    #[test]
    fn runs_that_never_ended_are_closed_as_interrupted() {
        let h = History::in_memory();
        let config = Config::from_json(
            r#"{"backups": [{"id": "b1", "name": "Photos", "archives": [
                {"id": "a1", "source": "C:/f", "path": "D:/a1", "mode": "mirror"},
                {"id": "a2", "source": "C:/f", "path": "D:/a2", "mode": "mirror"},
                {"id": "a3", "source": "C:/f", "path": "D:/a3", "mode": "mirror", "enabled": false}
            ]}]}"#,
        )
        .unwrap();
        // a1 was done when Diskman ended, a2 was not reached
        let r = h.start_run("b1", "Photos", "C:/f", "manual").unwrap();
        h.record_archive(r, &archive_run("a1", Outcome::Ok, "3 new")).unwrap();
        // A run that ended normally stays as it is
        let done = h.start_run("b1", "Photos", "C:/f", "manual").unwrap();
        h.finish_run(done).unwrap();

        assert_eq!(h.close_interrupted(&config).unwrap(), 1);
        let status = h.latest_status().unwrap();
        assert!(status["a1"].ok);
        assert!(!status["a2"].ok);
        assert!(status["a2"].message.starts_with("Interrupted"));
        assert!(!status.contains_key("a3"), "disabled entries were not part of the run");
        assert_eq!(h.close_interrupted(&config).unwrap(), 0, "closed once");
    }

    #[test]
    fn latest_status_is_newest_entry_per_archive() {
        let h = History::in_memory();
        let r1 = h.start_run("b1", "Photos", "C:/f", "manual").unwrap();
        h.record_archive(r1, &archive_run("a1", Outcome::Failed, "broken")).unwrap();
        h.record_archive(r1, &archive_run("a2", Outcome::Ok, "3 new")).unwrap();
        h.finish_run(r1).unwrap();
        let r2 = h.start_run("b1", "Photos", "C:/f", "auto").unwrap();
        h.record_archive(r2, &archive_run("a1", Outcome::Ok, "Already up to date")).unwrap();
        h.finish_run(r2).unwrap();

        let st = h.latest_status().unwrap();
        assert_eq!(st.len(), 2);
        assert!(st["a1"].ok);
        assert_eq!(st["a1"].message, "Already up to date");
        assert_eq!(st["a2"].message, "3 new");
    }

    #[test]
    fn run_outcome_is_worst_archive_outcome() {
        let h = History::in_memory();
        let outcome = |run: i64| -> String {
            h.conn.lock().unwrap().query_row("SELECT outcome FROM runs WHERE id = ?1", [run], |r| r.get(0)).unwrap()
        };
        let r = h.start_run("b", "B", "C:/b", "manual").unwrap();
        h.record_archive(r, &archive_run("a1", Outcome::Ok, "")).unwrap();
        h.record_archive(r, &archive_run("a2", Outcome::Skipped, "")).unwrap();
        h.finish_run(r).unwrap();
        assert_eq!(outcome(r), "errors");

        let empty = h.start_run("b", "B", "C:/b", "manual").unwrap();
        h.finish_run(empty).unwrap();
        assert_eq!(outcome(empty), "cancelled");
    }

    #[test]
    fn errors_are_stored_and_removed_with_their_run() {
        let h = History::in_memory();
        let r = h.start_run("b", "B", "C:/b", "manual").unwrap();
        let mut ar = archive_run("a", Outcome::Errors, "1 error");
        ar.errors = vec![Issue::new("x.txt", Object::File, Reason::AccessDenied)];
        ar.error_count = 1;
        h.record_archive(r, &ar).unwrap();
        h.finish_run(r).unwrap();
        let count = |table: &str| -> i64 {
            h.conn.lock().unwrap().query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
        };
        assert_eq!(count("run_errors"), 1);
        h.conn.lock().unwrap().execute("DELETE FROM runs", []).unwrap();
        assert_eq!(count("run_archives"), 0);
        assert_eq!(count("run_errors"), 0);
    }

    #[test]
    fn forgets_deleted_backups_and_entries() {
        let h = History::in_memory();
        let gone = h.start_run("gone", "Old", "C:/o", "manual").unwrap();
        let mut ar = archive_run("g1", Outcome::Errors, "1 error");
        ar.errors = vec![Issue::new("x.txt", Object::File, Reason::AccessDenied)];
        h.record_archive(gone, &ar).unwrap();
        h.finish_run(gone).unwrap();
        let kept = h.start_run("kept", "Photos", "C:/f", "manual").unwrap();
        h.record_archive(kept, &archive_run("k1", Outcome::Ok, "ok")).unwrap();
        h.record_archive(kept, &archive_run("k2", Outcome::Ok, "ok")).unwrap();
        h.finish_run(kept).unwrap();

        // Backup "gone" deleted (with its entry g1), entry k2 removed from "kept"
        h.forget(&["gone".into()], &["g1".into(), "k2".into()]).unwrap();

        let count = |sql: &str| -> i64 { h.conn.lock().unwrap().query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(count("SELECT COUNT(*) FROM runs WHERE backup_id = 'gone'"), 0);
        assert_eq!(count("SELECT COUNT(*) FROM run_errors"), 0);
        assert_eq!(count("SELECT COUNT(*) FROM runs WHERE backup_id = 'kept'"), 1);
        let st = h.latest_status().unwrap();
        assert_eq!(st.keys().collect::<Vec<_>>(), ["k1"]);
    }

    #[test]
    fn problems_are_the_latest_results_that_were_not_ok() {
        let h = History::in_memory();
        let r = h.start_run("b", "B", "C:/b", "manual").unwrap();
        let mut ar = archive_run("a1", Outcome::Errors, "2 errors");
        ar.errors = vec![
            Issue::new("x.txt", Object::File, Reason::Locked).with_size(42),
            Issue::new("sub", Object::Folder, Reason::AccessDenied).with_detail("Access is denied."),
        ];
        ar.error_count = 2;
        h.record_archive(r, &ar).unwrap();
        h.record_archive(r, &archive_run("a2", Outcome::Ok, "ok")).unwrap();
        h.finish_run(r).unwrap();

        let p = h.problems(&["a1".into(), "a2".into(), "unknown".into()]).unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].outcome, "errors");
        assert_eq!(p[0].error_count, 2);
        assert_eq!(p[0].issues[0].reason, Reason::Locked);
        assert_eq!(p[0].issues[0].size, 42);
        assert_eq!(p[0].issues[1].object, Object::Folder);
        assert_eq!(p[0].issues[1].detail.as_deref(), Some("Access is denied."));

        // A later good run clears it
        let r = h.start_run("b", "B", "C:/b", "manual").unwrap();
        h.record_archive(r, &archive_run("a1", Outcome::Ok, "ok")).unwrap();
        h.finish_run(r).unwrap();
        assert!(h.problems(&["a1".into()]).unwrap().is_empty());
    }

    #[test]
    fn errors_from_before_schema_2_are_still_readable() {
        let issue = stored_issue("docs/x.txt: Access is denied.".into(), None, None, None, 0);
        assert_eq!(issue.path, "docs/x.txt");
        assert_eq!(issue.reason, Reason::Other);
        assert_eq!(issue.detail.as_deref(), Some("Access is denied."));
    }

    #[test]
    fn keeps_only_newest_runs_per_backup() {
        let h = History::in_memory();
        for _ in 0..KEEP_RUNS_PER_BACKUP + 3 {
            let r = h.start_run("b1", "B", "C:/b", "auto").unwrap();
            h.finish_run(r).unwrap();
        }
        let r = h.start_run("b2", "C", "C:/c", "auto").unwrap();
        h.finish_run(r).unwrap();
        let conn = h.conn.lock().unwrap();
        let count = |b: &str| -> i64 {
            conn.query_row("SELECT COUNT(*) FROM runs WHERE backup_id = ?1", [b], |r| r.get(0)).unwrap()
        };
        assert_eq!(count("b1"), KEEP_RUNS_PER_BACKUP);
        assert_eq!(count("b2"), 1);
    }

    #[test]
    fn imports_legacy_status_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("status.json");
        fs::write(
            &path,
            r#"{"a1": {"last_run": "2026-11-10T18:46:37Z", "ok": true, "message": "3 new"},
                "gone": {"last_run": "2026-11-10T18:46:37Z", "ok": false, "message": "x"}}"#,
        )
        .unwrap();
        let config = Config {
            backups: vec![Backup {
                id: "b1".into(),
                name: "Photos".into(),
                archives: vec![Archive {
                    id: "a1".into(),
                    source: "C:/f".into(),
                    path: "D:/f".into(),
                    mode: Mode::Additive,
                    enabled: true,
                }],
                schedule: None,
                color: None,
                legacy_path: String::new(),
            }],
            ..Default::default()
        };
        let h = History::open(&dir.path().join("diskman.db")).unwrap();
        h.import_legacy_status(&path, &config).unwrap();

        let st = h.latest_status().unwrap();
        assert_eq!(st.len(), 1);
        assert_eq!(st["a1"].message, "3 new");
        assert_eq!(st["a1"].last_run.to_rfc3339(), "2026-11-10T18:46:37+00:00");
        assert!(!path.exists() && dir.path().join("status.json.migrated").exists());

        // If the file shows up again, it is not imported twice
        fs::copy(dir.path().join("status.json.migrated"), &path).unwrap();
        h.import_legacy_status(&path, &config).unwrap();
        let runs: i64 = h.conn.lock().unwrap().query_row("SELECT COUNT(*) FROM runs", [], |r| r.get(0)).unwrap();
        assert_eq!(runs, 1);
    }

    #[test]
    fn reopening_keeps_data_and_schema() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("diskman.db");
        {
            let h = History::open(&db).unwrap();
            let r = h.start_run("b", "B", "C:/b", "manual").unwrap();
            h.record_archive(r, &archive_run("a", Outcome::Ok, "ok")).unwrap();
            h.finish_run(r).unwrap();
        }
        let h = History::open(&db).unwrap();
        assert_eq!(h.latest_status().unwrap().len(), 1);
    }

    #[test]
    fn schedule_log_keeps_only_the_written_schedules() {
        let h = History::in_memory();
        let t = |s: &str| DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
        let log = |c: &str, n: &str| ScheduleLog { checked_at: t(c), counted_at: t(n) };
        h.write_schedule_log(&[
            ("a".into(), log("2026-11-09T10:00:00Z", "2026-11-09T09:00:00Z")),
            ("b".into(), log("2026-11-09T10:00:00Z", "2026-11-09T10:00:00Z")),
        ])
        .unwrap();
        assert_eq!(h.schedule_log().unwrap()["a"], log("2026-11-09T10:00:00Z", "2026-11-09T09:00:00Z"));

        // "b" no longer catches up: its entry goes, so an old one cannot count on the next start
        h.write_schedule_log(&[("a".into(), log("2026-11-09T10:01:00Z", "2026-11-09T09:00:00Z"))]).unwrap();
        let read = h.schedule_log().unwrap();
        assert_eq!(read.keys().collect::<Vec<_>>(), ["a"]);
        assert_eq!(read["a"].checked_at, t("2026-11-09T10:01:00Z"));

        // Deleting the backup removes its entry as well
        h.forget(&["a".into()], &[]).unwrap();
        assert!(h.schedule_log().unwrap().is_empty());
    }

    #[test]
    fn last_run_of_a_backup() {
        let h = History::in_memory();
        assert_eq!(h.last_run_at("b").unwrap(), None);
        let r = h.start_run("b", "B", "C:/b", "schedule").unwrap();
        h.finish_run(r).unwrap();
        assert!(h.last_run_at("b").unwrap().is_some());
        assert_eq!(h.last_run_at("other").unwrap(), None);
    }

    fn packed(plan_id: &str, size: usize) -> PackedPlan {
        PackedPlan { plan_id: plan_id.into(), data: vec![7; size] }
    }

    fn with_plan(archive_id: &str, outcome: Outcome, plan: PackedPlan) -> ArchiveRun {
        ArchiveRun { plan: Some(plan), ..archive_run(archive_id, outcome, "") }
    }

    fn check(h: &History, backup_id: &str, results: &[ArchiveRun]) -> i64 {
        h.record_check(backup_id, "B", "C:/f", "manual", Utc::now(), results).unwrap()
    }

    #[test]
    fn checks_never_count_as_the_last_run() {
        let h = History::in_memory();
        let r = h.start_run("b", "B", "C:/f", "manual").unwrap();
        h.record_archive(r, &archive_run("a", Outcome::Errors, "1 error")).unwrap();
        h.finish_run(r).unwrap();
        // A later check that found nothing to do changes neither status nor problems
        check(&h, "b", &[archive_run("a", Outcome::Ok, "Already up to date")]);
        check(&h, "never-run", &[archive_run("x", Outcome::Ok, "3 new")]);

        let st = h.latest_status().unwrap();
        assert_eq!(st["a"].message, "1 error");
        assert!(!st.contains_key("x"));
        assert_eq!(h.problems(&["a".into()]).unwrap().len(), 1);
        assert_eq!(h.last_run_at("never-run").unwrap(), None);
    }

    #[test]
    fn activities_are_listed_newest_first() {
        let h = History::in_memory();
        let r = h.start_run("b1", "Photos", "C:/f", "schedule").unwrap();
        let mut ar = with_plan("a1", Outcome::Errors, packed("p1", 10));
        ar.errors = vec![Issue::new("x.txt", Object::File, Reason::Locked)];
        ar.error_count = 1;
        ar.summary.copy_files = 3;
        h.record_archive(r, &ar).unwrap();
        h.record_archive(r, &archive_run("a2", Outcome::Ok, "ok")).unwrap();
        h.finish_run(r).unwrap();
        let c = check(&h, "b1", &[with_plan("a1", Outcome::Ok, packed("p2", 10))]);
        let other = check(&h, "b2", &[archive_run("a3", Outcome::Ok, "ok")]);

        let all = h.activities(&ActivityQuery { limit: 50, ..Default::default() }).unwrap();
        assert_eq!(all.iter().map(|a| a.id).collect::<Vec<_>>(), [other, c, r]);
        let run = &all[2];
        assert_eq!(
            (run.kind.as_str(), run.origin.as_str(), run.outcome.as_deref()),
            ("sync", "schedule", Some("errors"))
        );
        assert_eq!(run.entries.len(), 2);
        let entry = &run.entries[0];
        assert_eq!((entry.source.as_str(), entry.summary.copy_files, entry.error_count), ("C:/f", 3, 1));
        assert!(entry.has_plan && !run.entries[1].has_plan);
        assert_eq!(h.entry_errors(entry.id).unwrap()[0].reason, Reason::Locked);
        let plans = h.stored_plans(r).unwrap();
        assert_eq!(plans, [(entry.id, vec![7; 10])]);

        // Filters and the next page
        let of = |backup_id: Option<&str>, kind: Option<&str>, before: Option<i64>| {
            let q =
                ActivityQuery { backup_id: backup_id.map(Into::into), kind: kind.map(Into::into), before, limit: 50 };
            h.activities(&q).unwrap().iter().map(|a| a.id).collect::<Vec<_>>()
        };
        assert_eq!(of(Some("b1"), None, None), [c, r]);
        assert_eq!(of(None, Some("check"), None), [other, c]);
        assert_eq!(of(None, Some("sync"), None), [r]);
        assert_eq!(of(None, None, Some(c)), [r]);
        let page = h.activities(&ActivityQuery { limit: 1, ..Default::default() }).unwrap();
        assert_eq!(page.len(), 1);
    }

    #[test]
    fn the_oldest_plans_go_first_when_the_limit_is_reached() {
        let h = History::in_memory();
        for i in 0..4 {
            check(&h, "b", &[with_plan("a", Outcome::Ok, packed(&format!("p{i}"), 100))]);
        }
        assert_eq!(h.plan_storage().unwrap().bytes, 400);
        assert_eq!(h.prune_plans(250).unwrap(), 2);
        let storage = h.plan_storage().unwrap();
        assert_eq!((storage.bytes, storage.plans), (200, 2));
        // The activities stay, only the oldest lost their file list
        let all = h.activities(&ActivityQuery { limit: 50, ..Default::default() }).unwrap();
        let kept: Vec<bool> = all.iter().map(|a| a.entries[0].has_plan).collect();
        assert_eq!(kept, [true, true, false, false]);

        h.clear_plans().unwrap();
        assert_eq!(h.plan_storage().unwrap().plans, 0);
    }

    #[test]
    fn a_probed_plan_replaces_the_stored_one() {
        let h = History::in_memory();
        check(&h, "b", &[with_plan("a", Outcome::Ok, packed("p", 10))]);
        let summary = Summary { copy_files: 4, blocked: 1, ..Default::default() };
        h.update_plan(&packed("p", 20), &summary, "4 new").unwrap();
        // Not stored (any more): nothing happens
        h.update_plan(&packed("gone", 5), &summary, "x").unwrap();

        let entry =
            h.activities(&ActivityQuery { limit: 1, ..Default::default() }).unwrap().remove(0).entries.remove(0);
        assert_eq!((entry.summary.copy_files, entry.summary.blocked, entry.message.as_str()), (4, 1, "4 new"));
        assert_eq!(h.plan_storage().unwrap().bytes, 20);
    }

    #[test]
    fn checks_do_not_push_runs_out_of_the_history() {
        let h = History::in_memory();
        let r = h.start_run("b", "B", "C:/f", "manual").unwrap();
        h.finish_run(r).unwrap();
        for _ in 0..KEEP_RUNS_PER_BACKUP + 2 {
            check(&h, "b", &[]);
        }
        let count = |kind: &str| -> i64 {
            h.conn.lock().unwrap().query_row("SELECT COUNT(*) FROM runs WHERE kind = ?1", [kind], |r| r.get(0)).unwrap()
        };
        assert_eq!(count(SYNC), 1);
        assert_eq!(count(CHECK), KEEP_RUNS_PER_BACKUP);
    }

    #[test]
    fn clearing_the_history_keeps_only_a_running_sync() {
        let h = History::in_memory();
        check(&h, "b", &[with_plan("a", Outcome::Ok, packed("p", 10))]);
        let done = h.start_run("b", "B", "C:/f", "manual").unwrap();
        h.record_archive(done, &ArchiveRun::not_run("a", "C:/f", "D:/f", None, Outcome::Ok, "ok".into())).unwrap();
        h.finish_run(done).unwrap();
        let running = h.start_run("b", "B", "C:/f", "manual").unwrap();

        assert_eq!(h.clear_activities().unwrap(), 2);
        let left = h.activities(&ActivityQuery { backup_id: None, kind: None, before: None, limit: 10 }).unwrap();
        assert_eq!(left.iter().map(|a| a.id).collect::<Vec<_>>(), [running]);
        assert!(h.latest_status().unwrap().is_empty(), "no last run any more");
        assert_eq!(h.plan_storage().unwrap().plans, 0, "the plans went along");
        // The running sync can still record its result
        h.record_archive(running, &ArchiveRun::not_run("a", "C:/f", "D:/f", None, Outcome::Ok, "ok".into())).unwrap();
        h.finish_run(running).unwrap();
    }

    #[test]
    fn deleting_a_backup_removes_its_stored_plans() {
        let h = History::in_memory();
        check(&h, "gone", &[with_plan("g", Outcome::Ok, packed("p", 10))]);
        h.forget(&["gone".into()], &[]).unwrap();
        assert_eq!(h.plan_storage().unwrap().plans, 0);
    }

    #[test]
    fn runs_from_before_activities_are_listed_with_the_sources_of_their_run() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("diskman.db");
        // A database of schema 3 with one run
        {
            let mut conn = Connection::open(&db).unwrap();
            for sql in &MIGRATIONS[..3] {
                conn.execute_batch(sql).unwrap();
            }
            conn.pragma_update(None, "user_version", 3).unwrap();
            let tx = conn.transaction().unwrap();
            tx.execute(
                "INSERT INTO runs (backup_id, backup_name, source, origin, started_at, finished_at, outcome)
                 VALUES ('b', 'Photos', 'C:/old', 'manual', 1, 2, 'ok')",
                [],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO run_archives (run_id, archive_id, dest, outcome, message, started_at, finished_at)
                 VALUES (1, 'a', 'D:/old', 'ok', '3 new', 1, 2)",
                [],
            )
            .unwrap();
            tx.commit().unwrap();
        }
        let h = History::open(&db).unwrap();
        let all = h.activities(&ActivityQuery { limit: 10, ..Default::default() }).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].kind, "sync");
        assert_eq!(all[0].entries[0].source, "C:/old");
        assert!(!all[0].entries[0].has_plan);
        assert!(h.latest_status().unwrap()["a"].ok, "an old run still gives its archive the status");
    }
}
