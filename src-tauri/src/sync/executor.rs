//! Executes plans (copy, move, delete in the archive) and reports the progress in the job.
//!
//! An interrupted run (cancelled, crashed, power cut, drive unplugged) must not harm the archive:
//! - Copies are written to a temporary file, flushed to the drive and only then renamed over the
//!   target: the archive holds the old or the new version of a file, never half of one or one
//!   whose data never reached the drive. Leftover temporary files are deleted by the next run.
//! - Deleting moves the file into `TRASH_DIR` of the archive; the trash is emptied when the run
//!   ends. Until then every file is still somewhere in the archive, also a renamed one whose old
//!   name is gone and whose new name is not copied yet. An interrupted run leaves the trash to
//!   the next one that ends.
//! - Moving is a rename on the same drive and therefore all or nothing.
//! - The source is only read, never written.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;

use super::issues::{Issue, Object};
use super::planner::{Op, OpKind, Plan};
use super::scanner::{full_hash, join, TMP_SUFFIX, TRASH_DIR};
use crate::jobs::{EntryState, LiveEntry};
use crate::platform;
use crate::state::AppState;
use crate::storage::history::{self, ArchiveRun, Outcome};
use crate::storage::plan_store;

/// Executes the plans one after another and writes each result into the history run `run`.
/// The job must have been started with `try_begin_job`; job and run are finished here.
pub fn run_plans(state: &AppState, run: Option<i64>, plans: Vec<Plan>) {
    // Shared with the live tree of the UI while they run (`commands::sync::run_tree`)
    let plans = Arc::new(plans);
    {
        let mut job = state.job.lock().unwrap();
        job.total_ops = plans.iter().map(|p| p.ops.len() as u64).sum();
        job.total_bytes = plans.iter().map(|p| p.summary.copy_bytes + p.summary.update_bytes).sum();
        job.ops_started_at = Some(Utc::now());
        for p in plans.iter() {
            if !job.entries.iter().any(|e| e.plan_id == p.id) {
                job.entries.push(LiveEntry::of(p, EntryState::Waiting));
            }
        }
    }
    *state.running.lock().unwrap() = Some(Arc::clone(&plans));
    let verify = state.config.lock().unwrap().settings.verify_copies;
    for plan in plans.iter() {
        let entry = {
            let mut job = state.job.lock().unwrap();
            job.backup_id = plan.backup_id.clone();
            job.label = format!("{} → {}", plan.backup_name, plan.dest);
            let entry = job.entries.iter().position(|e| e.plan_id == plan.id).expect("entry added above");
            job.entries[entry].state = EntryState::Running;
            entry
        };
        let mut result = execute_plan(state, plan, verify, entry);
        // The plan goes into the history with its result (the activities show it). Packed after
        // the run, so a large plan does not delay its start.
        result.plan = plan_store::pack(plan).map_err(|e| eprintln!("Plan not stored: {e:#}")).ok();
        let cancelled = result.outcome == Outcome::Cancelled;
        state.job.lock().unwrap().entries[entry].state =
            if cancelled { EntryState::Cancelled } else { EntryState::Done };
        state.record_archive(run, &result);
        if cancelled {
            break;
        }
    }
    {
        let mut job = state.job.lock().unwrap();
        job.current.clear();
        // Not reached after a cancel: those stay waiting in the history too, so they are cancelled
        for e in job.entries.iter_mut().filter(|e| e.state == EntryState::Waiting) {
            e.state = EntryState::Cancelled;
        }
    }
    *state.running.lock().unwrap() = None;
    state.finish_run(run);
    state.end_job();
}

/// How the operations of a plan are carried out.
pub struct Apply {
    /// Compare every copy with its source before it replaces anything (`Settings::verify_copies`)
    pub verify: bool,
}

/// Folders whose names changed (renamed into or out of), flushed when the plan is done.
type Touched = HashSet<PathBuf>;

/// `entry`: its place in `job.entries`, where its progress goes.
fn execute_plan(state: &AppState, plan: &Plan, verify: bool, entry: usize) -> ArchiveRun {
    let src_root = Path::new(&plan.source);
    let dst_root = Path::new(&plan.dest);
    let started_at = Utc::now();
    let started = Instant::now();
    let mut errors: Vec<Issue> = Vec::new();
    let mut error_count = 0;
    let mut record_error = |issue: Issue| {
        error_count += 1;
        if errors.len() < history::MAX_STORED_ERRORS {
            errors.push(issue);
        }
    };
    // What the plan already knows cannot be transferred counts as failed, except links (skipped on purpose)
    let mut failures = 0;
    for issue in plan.blocked.iter().filter(|i| i.is_failure()) {
        record_error(issue.clone());
        failures += 1;
    }
    {
        let mut job = state.job.lock().unwrap();
        job.error_count += failures;
        job.entries[entry].error_count = failures;
    }
    let (mut ops_done, mut bytes_done) = (0, 0);
    let mut cancelled = false;
    let how = Apply { verify };
    let mut touched = Touched::new();
    if let Err(e) = fs::create_dir_all(dst_root) {
        record_error(Issue::from_io("", Object::Folder, &e).with_detail(format!("Archive {}: {e}", plan.dest)));
        let mut job = state.job.lock().unwrap();
        job.error_count += 1;
        job.entries[entry].error_count += 1;
    } else {
        // Half copies of an earlier run that was interrupted
        for rel in &plan.leftovers {
            let _ = remove_file_force(&join(dst_root, rel));
        }
        for op in &plan.ops {
            if state.cancel.load(Ordering::Relaxed) {
                cancelled = true;
                break;
            }
            // Shown while it is carried out: a large file may take a while
            state.job.lock().unwrap().current.clone_from(&op.path);
            let result = apply_tracked(op, src_root, dst_root, &how, &mut touched);
            ops_done += 1;
            let bytes = if matches!(op.kind, OpKind::Copy | OpKind::Update) { op.size } else { 0 };
            bytes_done += bytes;
            {
                let mut job = state.job.lock().unwrap();
                let failed = u64::from(result.is_err());
                job.done_ops += 1;
                job.done_bytes += bytes;
                job.error_count += failed;
                let e = &mut job.entries[entry];
                e.done_ops += 1;
                e.done_bytes += bytes;
                e.error_count += failed;
            }
            if let Err(e) = result {
                let folder = matches!(op.kind, OpKind::MoveDir | OpKind::DeleteDir | OpKind::CreateDir);
                let object = if folder { Object::Folder } else { Object::File };
                record_error(Issue::from_io(op.path.clone(), object, &e).with_size(op.size));
            }
        }
        // New names durable before the run counts as done (Linux; NTFS journals them anyway)
        for dir in &touched {
            platform::fs::sync_dir(dir);
        }
        // Ended (with or without errors): what it deleted can go. Cancelled: the trash stays for
        // the next run that ends, the archive may still need what is in it.
        if !cancelled {
            if let Err(e) = platform::fs::remove_tree(&dst_root.join(TRASH_DIR)) {
                eprintln!("Trash of {} not emptied: {e}", plan.dest);
            }
        }
    }
    let (outcome, message) = if cancelled {
        (Outcome::Cancelled, "Cancelled".to_string())
    } else if error_count > 0 {
        (
            Outcome::Errors,
            format!(
                "{error_count} {} · {}",
                if error_count == 1 { "error" } else { "errors" },
                plan.summary.describe()
            ),
        )
    } else {
        (Outcome::Ok, plan.summary.describe())
    };
    ArchiveRun {
        archive_id: plan.archive_id.clone(),
        source: plan.source.clone(),
        dest: plan.dest.clone(),
        mode: Some(plan.mode),
        outcome,
        message,
        started_at,
        finished_at: Utc::now(),
        diff_ms: Some(plan.diff_ms),
        copy_ms: Some(started.elapsed().as_millis() as u64),
        ops_total: plan.ops.len() as u64,
        ops_done,
        bytes_total: plan.summary.copy_bytes + plan.summary.update_bytes,
        bytes_done,
        summary: plan.summary.clone(),
        error_count,
        errors,
        plan: None,
    }
}

/// Carries out one operation in the archive below `dst_root` (for tests of the planner).
#[cfg(test)]
pub fn apply(op: &Op, src_root: &Path, dst_root: &Path, how: &Apply) -> io::Result<()> {
    apply_tracked(op, src_root, dst_root, how, &mut Touched::new())
}

fn apply_tracked(op: &Op, src_root: &Path, dst_root: &Path, how: &Apply, touched: &mut Touched) -> io::Result<()> {
    let target = join(dst_root, &op.path);
    let mut renamed = |p: &Path| {
        if let Some(parent) = p.parent() {
            touched.insert(parent.to_path_buf());
        }
    };
    match op.kind {
        OpKind::Delete => {
            let trash = join(&dst_root.join(TRASH_DIR), &op.path);
            renamed(&target);
            renamed(&trash);
            to_trash(&target, &trash)
        }
        OpKind::Move | OpKind::MoveDir => {
            let from = join(dst_root, op.from.as_deref().unwrap_or_default());
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            renamed(&from);
            renamed(&target);
            fs::rename(from, target)
        }
        OpKind::DeleteDir => match fs::remove_dir(&target) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        },
        OpKind::CreateDir => fs::create_dir_all(target),
        OpKind::Copy | OpKind::Update => {
            renamed(&target);
            copy_file(&join(src_root, &op.path), &target, how.verify)
        }
    }
}

/// Moves a file into the trash of its archive instead of deleting it (see the module notes).
/// Where that is not possible (e.g. a folder of the same name is in the trash) it is deleted.
fn to_trash(path: &Path, trash: &Path) -> io::Result<()> {
    let moved = (|| {
        if let Some(parent) = trash.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(path, trash)
    })();
    match moved {
        Ok(()) => Ok(()),
        Err(_) if !path.exists() => Ok(()),
        Err(_) => remove_file_force(path),
    }
}

/// Copies via a temporary file that is flushed to the drive before it replaces the target, so
/// neither a cancel nor a crash or power cut leaves a half or empty file under the real name.
/// Modification and creation times are carried over so the next comparison recognizes the file.
/// `verify`: the copy is read back and compared with the source before it replaces anything.
fn copy_file(from: &Path, to: &Path, verify: bool) -> io::Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    let name = to.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = to.with_file_name(format!(".{name}{TMP_SUFFIX}"));
    let result = (|| {
        platform::fs::copy_file(from, &tmp)?;
        if verify && full_hash(from)? != full_hash(&tmp)? {
            return Err(io::Error::other("Verification failed: the copy differs from the source"));
        }
        platform::fs::copy_times(&fs::metadata(from)?, &tmp)?;
        platform::fs::sync_file(&tmp)?;
        if to.exists() {
            platform::fs::clear_readonly(to)?;
        }
        fs::rename(&tmp, to)
    })();
    if result.is_err() {
        let _ = platform::fs::clear_readonly(&tmp);
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn remove_file_force(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            platform::fs::clear_readonly(path)?;
            fs::remove_file(path)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Backup;
    use crate::sync::planner::{build_plans, Scans};
    use std::sync::atomic::AtomicBool;

    fn backup(src: &Path, dst: &Path) -> Backup {
        serde_json::from_value(serde_json::json!({
            "id": "b", "name": "B",
            "archives": [{ "id": "a", "source": src, "path": dst, "mode": "mirror" }]
        }))
        .unwrap()
    }

    fn setup() -> (tempfile::TempDir, AppState, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let state = AppState::load_from(tmp.path().join("data")).unwrap();
        let (src, dst) = (tmp.path().join("src"), tmp.path().join("dst"));
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();
        (tmp, state, src, dst)
    }

    fn run(state: &AppState, src: &Path, dst: &Path) {
        let plans = build_plans(&backup(src, dst), 2000, &Scans::plain(&AtomicBool::new(false))).unwrap();
        assert!(state.try_begin_job("manual"));
        run_plans(state, None, plans);
    }

    #[test]
    fn a_finished_run_leaves_no_trash_and_no_half_copies() {
        let (_tmp, state, src, dst) = setup();
        fs::write(src.join("keep.txt"), "keep").unwrap();
        fs::write(dst.join("gone.txt"), "gone").unwrap();
        // Half copy of an earlier run and the trash of an interrupted one
        fs::write(dst.join(format!(".keep.txt{TMP_SUFFIX}")), "ke").unwrap();
        fs::create_dir_all(dst.join(TRASH_DIR)).unwrap();
        fs::write(dst.join(TRASH_DIR).join("old.txt"), "old").unwrap();

        run(&state, &src, &dst);

        let mut names: Vec<_> = fs::read_dir(&dst).unwrap().map(|e| e.unwrap().file_name()).collect();
        names.sort();
        assert_eq!(names, ["keep.txt"]);
        assert_eq!(fs::read_to_string(dst.join("keep.txt")).unwrap(), "keep");
    }

    #[test]
    fn a_cancelled_run_keeps_what_it_deleted_in_the_trash() {
        let (_tmp, state, src, dst) = setup();
        fs::write(src.join("new.txt"), "new").unwrap();
        fs::write(dst.join("old.txt"), "old").unwrap();
        let plans = build_plans(&backup(&src, &dst), 2000, &Scans::plain(&AtomicBool::new(false))).unwrap();
        // The delete runs first; cancel right after it, before the copy
        let delete = plans[0].ops.iter().find(|op| op.kind == OpKind::Delete).unwrap();
        apply(delete, &src, &dst, &Apply { verify: false }).unwrap();
        assert!(!dst.join("old.txt").exists());
        assert_eq!(fs::read_to_string(dst.join(TRASH_DIR).join("old.txt")).unwrap(), "old");

        // A cancelled run does not empty the trash
        assert!(state.try_begin_job("manual"));
        state.cancel.store(true, Ordering::Relaxed);
        run_plans(&state, None, plans);
        assert!(dst.join(TRASH_DIR).join("old.txt").exists());
        assert!(!dst.join("new.txt").exists());
    }

    #[test]
    fn the_job_shows_the_progress_of_every_entry() {
        let (_tmp, state, src, dst) = setup();
        fs::write(src.join("a.txt"), "abc").unwrap();
        fs::write(src.join("b.txt"), "de").unwrap();
        run(&state, &src, &dst);
        let job = state.job.lock().unwrap();
        assert_eq!(job.entries.len(), 1);
        let e = &job.entries[0];
        assert_eq!(e.state, EntryState::Done);
        assert_eq!((e.done_ops, e.total_ops), (2, 2));
        assert_eq!((e.done_bytes, e.total_bytes), (5, 5));
        assert_eq!((job.done_bytes, job.total_bytes, job.error_count), (5, 5, 0));
        assert!(job.current.is_empty());
        assert!(state.running.lock().unwrap().is_none(), "the live tree ends with the run");
    }

    #[test]
    fn verified_copies_arrive_intact() {
        let (_tmp, state, src, dst) = setup();
        fs::write(src.join("a.bin"), vec![7u8; 300_000]).unwrap();
        let mut config = state.config.lock().unwrap().clone();
        config.settings.verify_copies = true;
        state.save_config(config).unwrap();
        run(&state, &src, &dst);
        assert_eq!(fs::read(dst.join("a.bin")).unwrap(), vec![7u8; 300_000]);
    }
}
