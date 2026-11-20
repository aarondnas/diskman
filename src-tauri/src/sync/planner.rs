//! Comparison between source and archive: produces a plan of individual operations.
//!
//! Steps (all O(n) via hash maps, only move candidates are read):
//! 1. Scan both trees in parallel.
//! 2. Pair files by their relative path. Same size + modification time (± tolerance)
//!    = unchanged, otherwise = update.
//! 3. Source files without a partner are "new", archive files without a partner "orphaned".
//!    New and orphaned files are pre-grouped by size, filtered by modification time,
//!    ranked by file name/creation time and confirmed with sample hashes: for whole folders
//!    that moved by a few samples, otherwise file by file (`detect_moves`). How many files are
//!    read at once follows the drive of each side (`Io`).
//!    Matches are moved inside the archive instead of being copied again and deleted.
//! 4. Moves that affect a whole folder are merged into a single folder rename.
//! 5. What cannot be transferred is listed separately (`Plan::blocked`) instead of failing during
//!    the run: unreadable entries, links, case conflicts, files the archive's file system cannot
//!    hold and, when checking, source files that cannot be opened (locked, no permission).

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::bail;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::check_stop;
use super::issues::{Issue, Object, Reason};
use super::scanner::{self, FileMeta, Snapshot};
use super::tree::{self, Role, ScanCache, ScanCount, ScanMode, Scanned};
use crate::model::{Backup, Mode};
use crate::platform::volume::{self, VolumeLimits};

/// At most this many candidates of the same size are checked by hash.
const MAX_MOVE_CANDIDATES: usize = 4;
/// Pairs compared by content per group of sure moves from one folder into another (at least 2:
/// first and last, the others spread between them).
const GROUP_SAMPLES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpKind {
    Delete,
    MoveDir,
    Move,
    DeleteDir,
    CreateDir,
    Copy,
    Update,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Op {
    pub kind: OpKind,
    /// Destination path relative to the archive (for Copy/Update also relative to the source).
    pub path: String,
    /// Original path in the archive for moves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    pub size: u64,
    pub files: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl OpKind {
    /// The name it is serialized with
    pub fn as_str(self) -> &'static str {
        match self {
            OpKind::Delete => "delete",
            OpKind::MoveDir => "move_dir",
            OpKind::Move => "move",
            OpKind::DeleteDir => "delete_dir",
            OpKind::CreateDir => "create_dir",
            OpKind::Copy => "copy",
            OpKind::Update => "update",
        }
    }
}

impl Op {
    pub(crate) fn new(kind: OpKind, path: impl Into<String>, size: u64) -> Self {
        let files = if matches!(kind, OpKind::CreateDir | OpKind::DeleteDir) { 0 } else { 1 };
        Op { kind, path: path.into(), from: None, size, files, note: None }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Summary {
    pub copy_files: u64,
    pub copy_bytes: u64,
    pub update_files: u64,
    pub update_bytes: u64,
    pub move_files: u64,
    pub move_bytes: u64,
    pub dirs_moved: u64,
    pub delete_files: u64,
    pub delete_bytes: u64,
    pub dirs_created: u64,
    pub dirs_deleted: u64,
    pub unchanged: u64,
    /// Archive files left untouched because of read errors in the source.
    pub protected: u64,
    /// Entries that cannot be transferred (`Plan::blocked`).
    pub blocked: u64,
}

impl Summary {
    pub fn describe(&self) -> String {
        let parts: Vec<String> = [
            (self.copy_files, "new"),
            (self.update_files, "updated"),
            (self.move_files, "moved"),
            (self.delete_files, "deleted"),
        ]
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, label)| format!("{n} {label}"))
        .collect();
        if parts.is_empty() {
            "Already up to date".into()
        } else {
            parts.join(", ")
        }
    }
}

/// Something about a plan worth knowing before running it. Sent as its kind and figures, the UI
/// words it (`preview.warnings` in the translations).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Warning {
    /// The archive folder does not exist yet and will be created
    ArchiveMissing,
    /// Mirror: files in the archive whose source could not be read are left untouched
    SourceUnreadable,
    /// Entries in the archive could not be read
    ArchiveUnreadable { count: usize },
    /// Mirror of an empty source: would delete the whole archive
    SourceEmpty,
    /// Mirror: more than half of the archive's files would be deleted
    ManyDeletes { count: usize, total: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub backup_id: String,
    pub archive_id: String,
    pub backup_name: String,
    pub source: String,
    pub dest: String,
    pub mode: Mode,
    pub ops: Vec<Op>,
    pub summary: Summary,
    pub warnings: Vec<Warning>,
    /// What cannot be transferred, and why.
    pub blocked: Vec<Issue>,
    /// Would (probably unintentionally) empty the whole archive.
    pub dangerous: bool,
    /// Scanning source and archives (together, in parallel)
    pub scan_ms: u64,
    pub diff_ms: u64,
    pub created_at: DateTime<Utc>,
    /// Time and kind of the scan of the source and of the archive (None: it does not exist yet)
    pub src_scan_ms: u64,
    pub src_scan: ScanMode,
    pub dst_scan_ms: u64,
    pub dst_scan: Option<ScanMode>,
    /// Files and folders (below the root) of the source and of the archive that were compared
    pub src_files: u64,
    pub src_dirs: u64,
    pub dst_files: u64,
    pub dst_dirs: u64,
    /// The source files to copy were opened to find locked and unreadable ones (`probe_plan`)
    pub probed: bool,
    pub probe_ms: Option<u64>,
    /// Temporary files of unfinished copies in the archive, deleted when the plan runs.
    #[serde(skip)]
    pub leftovers: Vec<String>,
}

/// Where the scans of `build_plans` come from and when they stop.
pub struct Scans<'a> {
    /// Kept trees: only what changed is read (`sync::tree`); None: always everything
    pub cache: Option<&'a ScanCache>,
    /// Number of the "Check all" this belongs to: its backups share scans of the same folder
    pub batch: Option<u64>,
    /// Once set, scanning ends with `Cancelled` (see `sync::is_cancelled`)
    pub stop: &'a AtomicBool,
    /// Counts the folders and files read, for the live progress of a check; at the end the totals
    /// of all trees (`ScanCount`)
    pub count: Option<&'a ScanCount>,
}

impl<'a> Scans<'a> {
    /// Full scans without a cache.
    #[cfg(test)]
    pub fn plain(stop: &'a AtomicBool) -> Self {
        Scans { cache: None, batch: None, stop, count: None }
    }

    fn scan(&self, root: &Path, role: Role) -> anyhow::Result<Scanned> {
        match self.cache {
            Some(cache) => cache.scan_counted(root, role, self.batch, self.stop, self.count),
            None => {
                let started = Instant::now();
                let tree = tree::read_tree_counted(root, volume::scan_threads(root), self.stop, self.count)?;
                let ms = started.elapsed().as_millis() as u64;
                Ok(Scanned { snapshot: Arc::new(tree.snapshot()), mode: ScanMode::Full, ms })
            }
        }
    }
}

/// Builds the plans for all enabled archives of a backup, without opening the files to copy
/// (that is `probe_plan`, afterwards). Each source folder is scanned only once, through `scans`:
/// trees on different disks at the same time, trees on one disk one after another (two scans of
/// one hard disk at once would make its heads jump between them).
pub fn build_plans(backup: &Backup, tol: i64, scans: &Scans) -> anyhow::Result<Vec<Plan>> {
    let stop = scans.stop;
    check_stop(stop)?;
    let archives: Vec<_> = backup.archives.iter().filter(|a| a.enabled).collect();
    if archives.is_empty() {
        bail!("\"{}\" has no enabled archive.", backup.name);
    }
    let mut sources: Vec<&str> = Vec::new();
    for a in &archives {
        if !sources.contains(&a.source.as_str()) {
            sources.push(&a.source);
        }
    }
    let started = Instant::now();
    // The sources first, then one tree per archive; an archive folder that does not exist yet has none
    let roots: Vec<(&Path, Role)> = sources
        .iter()
        .map(|s| (Path::new(*s), Role::Source))
        .chain(archives.iter().map(|a| (Path::new(&a.path), Role::Archive)))
        .collect();
    let drives: Vec<volume::Drive> = roots.iter().map(|(root, _)| volume::drive(root)).collect();
    let mut by_disk: Vec<(&str, Vec<usize>)> = Vec::new();
    for (i, d) in drives.iter().enumerate() {
        match by_disk.iter_mut().find(|(disk, _)| *disk == d.disk) {
            Some((_, idx)) => idx.push(i),
            None => by_disk.push((&d.disk, vec![i])),
        }
    }
    let scan = |i: usize| -> anyhow::Result<Option<Scanned>> {
        let (root, role) = roots[i];
        match role {
            Role::Archive if !root.exists() => Ok(None),
            _ => scans.scan(root, role).map(Some),
        }
    };
    let mut scanned: Vec<Option<anyhow::Result<Option<Scanned>>>> = roots.iter().map(|_| None).collect();
    std::thread::scope(|s| {
        let handles: Vec<_> =
            by_disk.iter().map(|(_, idx)| s.spawn(|| idx.iter().map(|&i| (i, scan(i))).collect::<Vec<_>>())).collect();
        for h in handles {
            for (i, r) in h.join().expect("scan thread panicked") {
                scanned[i] = Some(r);
            }
        }
    });
    check_stop(stop)?;
    let mut scanned = scanned.into_iter().map(|r| r.expect("every tree was scanned"));
    let srcs = scanned
        .by_ref()
        .take(sources.len())
        .map(|r| r.map(|s| s.expect("a source always has a tree")))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let dsts: Vec<_> = scanned.collect();
    let scan_ms = started.elapsed().as_millis() as u64;
    // What the check covers: every tree once, also those only partly read or shared
    if let Some(count) = scans.count {
        let trees = srcs
            .iter()
            .map(|s| &s.snapshot)
            .chain(dsts.iter().filter_map(|d| d.as_ref().ok().and_then(Option::as_ref)).map(|d| &d.snapshot));
        let (files, dirs) = trees.fold((0, 0), |(f, d), s| (f + s.files.len() as u64, d + s.dirs.len() as u64));
        count.set(files, dirs);
    }

    let mut plans = Vec::new();
    for (n, (archive, dst)) in archives.into_iter().zip(dsts).enumerate() {
        check_stop(stop)?;
        let mut warnings = Vec::new();
        let empty = Arc::new(Snapshot::default());
        let (dst_snap, dst_scan_ms, dst_scan) = match dst? {
            Some(d) => (d.snapshot, d.ms, Some(d.mode)),
            None => {
                warnings.push(Warning::ArchiveMissing);
                (empty, 0, None)
            }
        };
        let started = Instant::now();
        let si = sources.iter().position(|s| *s == archive.source).expect("source was collected");
        let src = &srcs[si];
        let limits = volume::limits(Path::new(&archive.path));
        let io = Io::between(&drives[si], &drives[sources.len() + n]);
        let rules = Rules { mode: archive.mode, tol, fold_case: limits.case_insensitive, io };
        let mut d = diff(&src.snapshot, &dst_snap, Path::new(&archive.source), Path::new(&archive.path), rules);
        apply_limits(&mut d, &src.snapshot, limits);
        warnings.extend(d.warnings);
        plans.push(Plan {
            id: uuid::Uuid::new_v4().to_string(),
            backup_id: backup.id.clone(),
            archive_id: archive.id.clone(),
            backup_name: backup.name.clone(),
            source: archive.source.clone(),
            dest: archive.path.clone(),
            mode: archive.mode,
            ops: d.ops,
            summary: d.summary,
            warnings,
            blocked: d.blocked,
            dangerous: d.dangerous,
            scan_ms,
            diff_ms: started.elapsed().as_millis() as u64,
            created_at: Utc::now(),
            src_scan_ms: src.ms,
            src_scan: src.mode,
            dst_scan_ms,
            dst_scan,
            src_files: src.snapshot.files.len() as u64,
            src_dirs: src.snapshot.dirs.len() as u64,
            dst_files: dst_snap.files.len() as u64,
            dst_dirs: dst_snap.dirs.len() as u64,
            probed: false,
            probe_ms: None,
            leftovers: dst_snap.leftovers.clone(),
        });
    }
    check_stop(stop)?;
    Ok(plans)
}

/// How source and archive are compared.
#[derive(Debug, Clone, Copy)]
pub struct Rules {
    pub mode: Mode,
    /// Modification times this many ms apart still count as equal
    pub tol: i64,
    /// Paths that differ only in upper and lower case are the same file. The archive's file system
    /// decides: case-insensitive on Windows, and on Linux for archives on FAT or exFAT.
    pub fold_case: bool,
    /// How file contents may be read on each side (move detection)
    pub io: Io,
}

/// How file contents are read from source and archive, after their drives (`volume::Drive`).
#[derive(Debug, Clone, Copy)]
pub struct Io {
    /// Files read from the source at once
    pub src_threads: usize,
    /// Files read from the archive at once
    pub dst_threads: usize,
    /// Both on one physical disk: never read from both sides at the same time
    pub same_disk: bool,
}

impl Io {
    pub fn between(src: &volume::Drive, dst: &volume::Drive) -> Self {
        let same_disk = src.disk == dst.disk;
        // On one disk both sides share what it can do at once
        let (s, d) = (src.read_threads(), dst.read_threads());
        let (s, d) = if same_disk { (s.min(d), s.min(d)) } else { (s, d) };
        Io { src_threads: s, dst_threads: d, same_disk }
    }
}

pub struct Diff {
    pub ops: Vec<Op>,
    pub summary: Summary,
    pub warnings: Vec<Warning>,
    pub blocked: Vec<Issue>,
    pub dangerous: bool,
}

struct Move {
    from: String,
    to: String,
    size: u64,
}

struct DirMove {
    from: String,
    to: String,
    size: u64,
    files: u64,
}

/// Comparison key of a relative path: lowercase if case does not matter (`Rules::fold_case`).
fn key(rel: &str, fold_case: bool) -> String {
    if fold_case {
        rel.to_lowercase()
    } else {
        rel.to_string()
    }
}

fn file_name(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

fn parent_prefix(rel: &str) -> &str {
    rel.rfind('/').map_or("", |i| &rel[..=i])
}

fn depth(rel: &str) -> usize {
    rel.matches('/').count()
}

/// `k` is inside `prefix` or is `prefix` itself. Empty prefix = root = everything.
fn is_under(k: &str, prefix: &str) -> bool {
    prefix.is_empty() || k == prefix || (k.starts_with(prefix) && k.as_bytes().get(prefix.len()) == Some(&b'/'))
}

pub fn diff(src: &Snapshot, dst: &Snapshot, src_root: &Path, dst_root: &Path, rules: Rules) -> Diff {
    let Rules { mode, tol, fold_case, .. } = rules;
    let key = |rel: &str| key(rel, fold_case);
    let mirror = mode == Mode::Mirror;
    let mut sum = Summary::default();
    let mut warnings = Vec::new();
    // Paths below a source entry that could not be read stay untouched in the archive. Looked up
    // by the path and its parents, not by comparing with every error (that would be quadratic)
    let protected: HashSet<String> = src.errors.iter().map(|e| key(&e.path)).collect();
    let is_protected = |k: &str| {
        !protected.is_empty()
            && (protected.contains("")
                || protected.contains(k)
                || k.match_indices('/').any(|(i, _)| protected.contains(&k[..i])))
    };
    // Keys computed once per file
    let src_keys: Vec<String> = src.files.iter().map(|f| key(&f.rel)).collect();
    let dst_keys: Vec<String> = dst.files.iter().map(|f| key(&f.rel)).collect();
    if mirror && !src.errors.is_empty() {
        warnings.push(Warning::SourceUnreadable);
    }
    if !dst.errors.is_empty() {
        warnings.push(Warning::ArchiveUnreadable { count: dst.errors.len() });
    }
    let mut blocked: Vec<Issue> = src.errors.clone();
    blocked.extend(src.skipped.iter().cloned());

    // Source files whose paths differ only in case (a case-sensitive source, e.g. Linux, and a
    // case-insensitive archive) would land on the same file in the archive: the first one is
    // synced, the others are listed
    let mut first_by_key: HashMap<&str, &str> = HashMap::new();
    let mut conflicting = vec![false; src.files.len()];
    for (si, f) in src.files.iter().enumerate() {
        match first_by_key.get(src_keys[si].as_str()) {
            Some(first) => {
                conflicting[si] = true;
                blocked.push(
                    Issue::new(f.rel.clone(), Object::File, Reason::CaseConflict).with_detail(*first).with_size(f.size),
                );
            }
            None => {
                first_by_key.insert(&src_keys[si], &f.rel);
            }
        }
    }

    // 1. Pair by path
    let dst_by_key: HashMap<&str, usize> = dst_keys.iter().enumerate().map(|(i, k)| (k.as_str(), i)).collect();
    let mut dst_seen = vec![false; dst.files.len()];
    let mut new_files = Vec::new();
    let mut updates = Vec::new();
    let mut moves = Vec::new();
    for (si, f) in src.files.iter().enumerate() {
        if conflicting[si] {
            continue;
        }
        let Some(&di) = dst_by_key.get(src_keys[si].as_str()) else {
            new_files.push(si);
            continue;
        };
        dst_seen[di] = true;
        let d = &dst.files[di];
        if f.size == d.size && (f.mtime - d.mtime).abs() <= tol {
            if mirror && file_name(&d.rel) != file_name(&f.rel) {
                // Only the case of the name changed
                moves.push(Move {
                    from: d.rel.clone(),
                    to: format!("{}{}", parent_prefix(&d.rel), file_name(&f.rel)),
                    size: f.size,
                });
            } else {
                sum.unchanged += 1;
            }
        } else {
            let mut op = Op::new(OpKind::Update, f.rel.clone(), f.size);
            if d.mtime > f.mtime + tol {
                op.note = Some("Archive version is newer than the source".into());
            }
            updates.push(op);
        }
    }

    let mut orphans = Vec::new();
    for di in 0..dst.files.len() {
        if dst_seen[di] {
            continue;
        }
        if is_protected(&dst_keys[di]) {
            sum.protected += 1;
        } else {
            orphans.push(di);
        }
    }

    // 2. Detect moves (only when mirroring, "Add only" never removes anything)
    let found = if mirror {
        detect_moves(&new_files, &orphans, src, dst, src_root, dst_root, rules)
    } else {
        vec![None; new_files.len()]
    };
    let mut orphan_used = vec![false; dst.files.len()];
    let mut copies = Vec::new();
    for (&si, hit) in new_files.iter().zip(found) {
        let f = &src.files[si];
        match hit {
            Some(di) => {
                orphan_used[di] = true;
                moves.push(Move { from: dst.files[di].rel.clone(), to: f.rel.clone(), size: f.size });
            }
            None => copies.push(Op::new(OpKind::Copy, f.rel.clone(), f.size)),
        }
    }

    let deletes: Vec<Op> = if mirror {
        orphans
            .iter()
            .filter(|&&di| !orphan_used[di])
            .map(|&di| Op::new(OpKind::Delete, dst.files[di].rel.clone(), dst.files[di].size))
            .collect()
    } else {
        Vec::new()
    };

    // 3. Merge whole folder renames
    let src_dirs: HashSet<String> = src.dirs.iter().map(|d| key(d)).collect();
    let (dir_moves, moves) = if mirror {
        let deleted: HashSet<String> = deletes.iter().map(|op| key(&op.path)).collect();
        collapse_dir_moves(moves, dst, &src_dirs, &deleted, fold_case)
    } else {
        (Vec::new(), moves)
    };

    // 4. Folder structure: archive folders as they look after the folder moves
    // The folder moves by their old path; accepted ones never nest, so a folder lies below at
    // most one, found among its own path and its parents
    let moved_from: HashMap<String, &DirMove> = dir_moves.iter().map(|m| (key(&m.from), m)).collect();
    let mut virtual_dirs: HashMap<String, String> = HashMap::new();
    for d in &dst.dirs {
        let k = key(d);
        let mapped = std::iter::once(k.len())
            .chain(k.match_indices('/').map(|(i, _)| i))
            .find_map(|i| moved_from.get(&k[..i]))
            .and_then(|m| d.get(m.from.len()..).map(|rest| format!("{}{}", m.to, rest)))
            .unwrap_or_else(|| d.clone());
        virtual_dirs.insert(key(&mapped), mapped);
    }
    let mut create_dirs: Vec<&String> = src.dirs.iter().filter(|d| !virtual_dirs.contains_key(&key(d))).collect();
    create_dirs.sort_by_key(|d| depth(d));
    let mut delete_dirs: Vec<String> = if mirror {
        virtual_dirs
            .iter()
            .filter(|(k, _)| !src_dirs.contains(*k) && !is_protected(k))
            .map(|(_, d)| d.clone())
            .collect()
    } else {
        Vec::new()
    };
    delete_dirs.sort_by(|a, b| depth(b).cmp(&depth(a)).then_with(|| a.cmp(b)));

    // Safety checks
    let dangerous = mirror && src.files.is_empty() && src.dirs.is_empty() && !dst.files.is_empty();
    if dangerous {
        warnings.push(Warning::SourceEmpty);
    } else if mirror && deletes.len() > 100 && deletes.len() * 2 > dst.files.len() {
        warnings.push(Warning::ManyDeletes { count: deletes.len(), total: dst.files.len() });
    }

    // Execution order: delete → move folders → move files →
    // remove empty folders → create folders → copy → update
    let mut ops = Vec::with_capacity(
        deletes.len()
            + dir_moves.len()
            + moves.len()
            + delete_dirs.len()
            + create_dirs.len()
            + copies.len()
            + updates.len(),
    );
    for op in deletes {
        sum.delete_files += 1;
        sum.delete_bytes += op.size;
        ops.push(op);
    }
    for m in dir_moves {
        sum.dirs_moved += 1;
        sum.move_files += m.files;
        sum.move_bytes += m.size;
        ops.push(Op {
            kind: OpKind::MoveDir,
            path: m.to,
            from: Some(m.from),
            size: m.size,
            files: m.files,
            note: None,
        });
    }
    for m in moves {
        sum.move_files += 1;
        sum.move_bytes += m.size;
        ops.push(Op { kind: OpKind::Move, path: m.to, from: Some(m.from), size: m.size, files: 1, note: None });
    }
    sum.dirs_deleted = delete_dirs.len() as u64;
    ops.extend(delete_dirs.into_iter().map(|d| Op::new(OpKind::DeleteDir, d, 0)));
    sum.dirs_created = create_dirs.len() as u64;
    ops.extend(create_dirs.into_iter().map(|d| Op::new(OpKind::CreateDir, d.clone(), 0)));
    for op in copies {
        sum.copy_files += 1;
        sum.copy_bytes += op.size;
        ops.push(op);
    }
    for op in updates {
        sum.update_files += 1;
        sum.update_bytes += op.size;
        ops.push(op);
    }
    sum.blocked = blocked.len() as u64;
    Diff { ops, summary: sum, warnings, blocked, dangerous }
}

/// Takes the operations that `check` refuses out of the plan and lists them as not transferable
/// instead. Only operations that put something new into the archive are checked: copies, updates
/// and new folders. (In the archive, an outdated version stays as it is.)
fn block_ops(
    ops: &mut Vec<Op>,
    summary: &mut Summary,
    blocked: &mut Vec<Issue>,
    mut check: impl FnMut(&Op) -> Option<Issue>,
) {
    let mut kept = Vec::with_capacity(ops.len());
    for op in std::mem::take(ops) {
        let checked = matches!(op.kind, OpKind::Copy | OpKind::Update | OpKind::CreateDir);
        let Some(issue) = (if checked { check(&op) } else { None }) else {
            kept.push(op);
            continue;
        };
        match op.kind {
            OpKind::Copy => {
                summary.copy_files -= 1;
                summary.copy_bytes -= op.size;
            }
            OpKind::Update => {
                summary.update_files -= 1;
                summary.update_bytes -= op.size;
            }
            _ => summary.dirs_created -= 1,
        }
        blocked.push(issue.with_size(op.size));
    }
    *ops = kept;
    summary.blocked = blocked.len() as u64;
}

/// What the archive's file system cannot hold: names FAT and exFAT refuse (Linux sources may use
/// them), files too large for FAT32, EFS-encrypted files on a file system without encryption.
/// Copying them would fail.
fn apply_limits(d: &mut Diff, src: &Snapshot, limits: VolumeLimits) {
    if !limits.restricts_files() {
        return;
    }
    let encrypted: HashSet<&str> = src.files.iter().filter(|f| f.encrypted).map(|f| f.rel.as_str()).collect();
    block_ops(&mut d.ops, &mut d.summary, &mut d.blocked, |op| {
        let object = if op.kind == OpKind::CreateDir { Object::Folder } else { Object::File };
        let refuse = |reason| Some(Issue::new(op.path.clone(), object, reason));
        if limits.restricted_names && !op.path.split('/').all(volume::fat_name_allowed) {
            return refuse(Reason::NameNotAllowed);
        }
        if object == Object::Folder {
            return None;
        }
        if limits.max_file_size.is_some_and(|max| op.size > max) {
            return refuse(Reason::TooLarge);
        }
        if !limits.encryption && encrypted.contains(op.path.as_str()) {
            return refuse(Reason::Encrypted);
        }
        None
    });
}

/// The source files a plan would copy or update.
pub fn probe_paths(plan: &Plan) -> Vec<String> {
    plan.ops.iter().filter(|op| matches!(op.kind, OpKind::Copy | OpKind::Update)).map(|op| op.path.clone()).collect()
}

/// Opens each of these source files (without reading it), as many at once as the source drive
/// allows (`volume::Drive::read_threads`): files locked by another program or without read
/// permission are found before a run fails on them. None if `stop` was set before all were opened.
pub fn probe(src_root: &Path, paths: &[String], stop: &AtomicBool) -> Option<HashMap<String, std::io::Error>> {
    if paths.is_empty() {
        return Some(HashMap::new());
    }
    let threads = volume::drive(src_root).read_threads();
    let chunk = paths.len().div_ceil(threads);
    let failed = std::thread::scope(|s| {
        let handles: Vec<_> = paths
            .chunks(chunk)
            .map(|part| {
                s.spawn(move || {
                    part.iter()
                        .take_while(|_| !stop.load(Ordering::Relaxed))
                        .filter_map(|p| File::open(scanner::join(src_root, p)).err().map(|e| (p.clone(), e)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("probe thread panicked")).collect()
    });
    (!stop.load(Ordering::Relaxed)).then_some(failed)
}

/// Takes what `probe` found out of the plan and lists it as not transferable.
pub fn apply_probe(plan: &mut Plan, mut failed: HashMap<String, std::io::Error>, ms: u64) {
    if !failed.is_empty() {
        block_ops(&mut plan.ops, &mut plan.summary, &mut plan.blocked, |op| {
            failed.remove(&op.path).map(|e| Issue::from_io(op.path.clone(), Object::File, &e))
        });
    }
    plan.probed = true;
    plan.probe_ms = Some(ms);
}

/// A plan for the UI: everything but the lists of operations and blocked entries, which it asks
/// for page by page (`Plan::rows`), plus the counts it shows.
#[derive(Debug, Clone, Serialize)]
pub struct PlanView {
    pub id: String,
    pub backup_id: String,
    pub archive_id: String,
    pub backup_name: String,
    pub source: String,
    pub dest: String,
    pub mode: Mode,
    pub summary: Summary,
    pub warnings: Vec<Warning>,
    pub dangerous: bool,
    pub scan_ms: u64,
    pub diff_ms: u64,
    pub created_at: DateTime<Utc>,
    pub src_scan_ms: u64,
    pub src_scan: ScanMode,
    pub dst_scan_ms: u64,
    pub dst_scan: Option<ScanMode>,
    pub src_files: u64,
    pub src_dirs: u64,
    pub dst_files: u64,
    pub dst_dirs: u64,
    pub probed: bool,
    pub probe_ms: Option<u64>,
    /// Number of operations
    pub ops: usize,
    /// Rows per filter of the list (`ROW_FILTERS`)
    pub counts: HashMap<&'static str, usize>,
    /// Blocked entries that mean something is missing (links and special files are skipped on
    /// purpose), those administrator rights may make readable, and their size
    pub blocked_failures: usize,
    pub blocked_admin: usize,
    pub blocked_bytes: u64,
}

/// One row of the list in the analysis: an operation, or an entry that cannot be transferred.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(untagged)]
pub enum Row<'a> {
    Op(&'a Op),
    Blocked { kind: &'static str, path: &'a str, size: u64, issue: &'a Issue },
}

/// The filters of the list and which operations each shows ("blocked": the blocked entries).
pub const ROW_FILTERS: [&str; 7] = ["all", "copy", "update", "move", "delete", "dirs", "blocked"];

fn op_in(filter: &str, kind: OpKind) -> bool {
    use OpKind::*;
    match filter {
        "all" => true,
        "copy" => kind == Copy,
        "update" => kind == Update,
        "move" => matches!(kind, Move | MoveDir),
        "delete" => matches!(kind, Delete | DeleteDir),
        "dirs" => matches!(kind, MoveDir | DeleteDir | CreateDir),
        _ => false,
    }
}

impl Plan {
    pub fn view(&self) -> PlanView {
        let counts = ROW_FILTERS
            .iter()
            .map(|&f| {
                let blocked = if matches!(f, "all" | "blocked") { self.blocked.len() } else { 0 };
                (f, blocked + self.ops.iter().filter(|op| op_in(f, op.kind)).count())
            })
            .collect();
        PlanView {
            id: self.id.clone(),
            backup_id: self.backup_id.clone(),
            archive_id: self.archive_id.clone(),
            backup_name: self.backup_name.clone(),
            source: self.source.clone(),
            dest: self.dest.clone(),
            mode: self.mode,
            summary: self.summary.clone(),
            warnings: self.warnings.clone(),
            dangerous: self.dangerous,
            scan_ms: self.scan_ms,
            diff_ms: self.diff_ms,
            created_at: self.created_at,
            src_scan_ms: self.src_scan_ms,
            src_scan: self.src_scan,
            dst_scan_ms: self.dst_scan_ms,
            dst_scan: self.dst_scan,
            src_files: self.src_files,
            src_dirs: self.src_dirs,
            dst_files: self.dst_files,
            dst_dirs: self.dst_dirs,
            probed: self.probed,
            probe_ms: self.probe_ms,
            ops: self.ops.len(),
            counts,
            blocked_failures: self.blocked.iter().filter(|i| i.is_failure()).count(),
            blocked_admin: self.blocked.iter().filter(|i| i.admin).count(),
            blocked_bytes: self.blocked.iter().map(|i| i.size).sum(),
        }
    }

    /// The rows of `filter` whose path (or former path) contains `query` (case does not matter),
    /// blocked entries first. The analysis shows them as a tree (`changes::tree`).
    pub fn matching<'a>(&'a self, filter: &'a str, query: &str) -> impl Iterator<Item = Row<'a>> + 'a {
        let q = query.trim().to_lowercase();
        let hit = move |path: &str, from: Option<&str>| {
            q.is_empty() || path.to_lowercase().contains(&q) || from.is_some_and(|f| f.to_lowercase().contains(&q))
        };
        let blocked = self.blocked.iter().filter(move |_| matches!(filter, "all" | "blocked")).map(|i| Row::Blocked {
            kind: "blocked",
            path: &i.path,
            size: i.size,
            issue: i,
        });
        let ops = self.ops.iter().filter(move |op| op_in(filter, op.kind)).map(Row::Op);
        blocked.chain(ops).filter(move |row| hit(row.path(), row.from()))
    }
}

impl<'a> Row<'a> {
    /// Path relative to the root (for moves the new one)
    pub fn path(&self) -> &'a str {
        match self {
            Row::Op(op) => &op.path,
            Row::Blocked { path, .. } => path,
        }
    }

    /// Former path of a move
    pub fn from(&self) -> Option<&'a str> {
        match self {
            Row::Op(op) => op.from.as_deref(),
            Row::Blocked { .. } => None,
        }
    }

    pub fn size(&self) -> u64 {
        match self {
            Row::Op(op) => op.size,
            Row::Blocked { size, .. } => *size,
        }
    }

    /// The row stands for a folder (created, deleted, moved or not transferable as a whole).
    pub fn is_folder(&self) -> bool {
        match self {
            Row::Op(op) => matches!(op.kind, OpKind::CreateDir | OpKind::DeleteDir | OpKind::MoveDir),
            Row::Blocked { issue, .. } => issue.object == Object::Folder,
        }
    }

    /// Name of its kind, as the UI knows it (`OpKind` in snake case, or "blocked")
    pub fn kind_name(&self) -> &'static str {
        match self {
            Row::Op(op) => op.kind.as_str(),
            Row::Blocked { .. } => "blocked",
        }
    }
}

/// Finds which new source files (`new_files`, indexes into `src.files`) are orphaned archive
/// files that moved: per new file the archive file (index into `dst.files`), or None.
/// Candidates have the same size and modification time; content decides.
///
/// Reading content is what costs, above all on a hard disk, so it is read sparingly:
/// 1. Sure moves – exactly one candidate with the same name, wanted by no other new file – are
///    grouped by old and new folder, as a renamed or moved folder produces them. Per group only
///    `GROUP_SAMPLES` pairs, spread over it, are compared by content: if all match, the whole group
///    is taken, otherwise each of its files is checked on its own (2.).
/// 2. All others one by one: up to `MAX_MOVE_CANDIDATES` candidates, best first, in rounds of one
///    candidate per file, so no more is read than needed.
///
/// Files are read in path order, as many at once as the drive of each side allows (`rules.io`);
/// source and archive at the same time unless they share a disk.
fn detect_moves(
    new_files: &[usize],
    orphans: &[usize],
    src: &Snapshot,
    dst: &Snapshot,
    src_root: &Path,
    dst_root: &Path,
    rules: Rules,
) -> Vec<Option<usize>> {
    let key = |rel: &str| key(rel, rules.fold_case);
    let mut found: Vec<Option<usize>> = vec![None; new_files.len()];
    let mut by_size: HashMap<u64, Vec<usize>> = HashMap::new();
    for &di in orphans {
        if dst.files[di].size > 0 {
            by_size.entry(dst.files[di].size).or_default().push(di);
        }
    }
    if by_size.is_empty() {
        return found;
    }

    // Candidates of each new file, best first: same name +2, same creation time +1
    let cands: Vec<Vec<(u8, usize)>> = new_files
        .iter()
        .map(|&si| {
            let f = &src.files[si];
            let Some(same_size) = by_size.get(&f.size) else { return Vec::new() };
            let name = key(file_name(&f.rel));
            let mut scored: Vec<(u8, usize)> = same_size
                .iter()
                .copied()
                .filter(|&di| (dst.files[di].mtime - f.mtime).abs() <= rules.tol)
                .map(|di| {
                    let d = &dst.files[di];
                    let mut score = 0;
                    if key(file_name(&d.rel)) == name {
                        score += 2;
                    }
                    if matches!((f.ctime, d.ctime), (Some(a), Some(b)) if (a - b).abs() <= rules.tol) {
                        score += 1;
                    }
                    (score, di)
                })
                .collect();
            scored.sort_by_key(|c| std::cmp::Reverse(c.0));
            scored
        })
        .collect();

    let mut hashes = Hashes::new(src, dst, src_root, dst_root, rules.io);
    let mut used = vec![false; dst.files.len()];

    // 1. Sure moves, confirmed by samples per pair of old and new folder
    let sure: Vec<Option<usize>> = cands
        .iter()
        .map(|c| {
            let mut named = c.iter().filter(|(score, _)| *score >= 2);
            match (named.next(), named.next()) {
                (Some(&(_, di)), None) => Some(di),
                _ => None,
            }
        })
        .collect();
    let mut wanted: HashMap<usize, u32> = HashMap::new();
    for &di in sure.iter().flatten() {
        *wanted.entry(di).or_default() += 1;
    }
    let mut groups: HashMap<(String, String), Vec<usize>> = HashMap::new();
    for (i, s) in sure.iter().enumerate() {
        if let Some(di) = *s {
            if wanted[&di] == 1 {
                let from = key(parent_prefix(&dst.files[di].rel));
                let to = key(parent_prefix(&src.files[new_files[i]].rel));
                groups.entry((from, to)).or_default().push(i);
            }
        }
    }
    let mut groups: Vec<Vec<usize>> = groups.into_values().collect();
    for g in &mut groups {
        g.sort_by(|&a, &b| src.files[new_files[a]].rel.cmp(&src.files[new_files[b]].rel));
    }
    let samples = |g: &[usize]| -> Vec<usize> {
        if g.len() <= GROUP_SAMPLES {
            g.to_vec()
        } else {
            (0..GROUP_SAMPLES).map(|k| g[k * (g.len() - 1) / (GROUP_SAMPLES - 1)]).collect()
        }
    };
    let pairs: Vec<(usize, usize)> =
        groups.iter().flat_map(|g| samples(g)).map(|i| (new_files[i], sure[i].expect("grouped"))).collect();
    hashes.read(&pairs);
    for g in &groups {
        if samples(g).iter().all(|&i| hashes.same(new_files[i], sure[i].expect("grouped"))) {
            for &i in g {
                let di = sure[i].expect("grouped");
                found[i] = Some(di);
                used[di] = true;
            }
        }
    }

    // 2. All others one by one: per round the next free candidate of each file still looking
    let mut looking: Vec<usize> =
        (0..new_files.len()).filter(|&i| found[i].is_none() && !cands[i].is_empty()).collect();
    let mut next = vec![0usize; new_files.len()];
    for _ in 0..MAX_MOVE_CANDIDATES {
        let mut round = Vec::new();
        for &i in &looking {
            while cands[i].get(next[i]).is_some_and(|&(_, di)| used[di]) {
                next[i] += 1;
            }
            if let Some(&(_, di)) = cands[i].get(next[i]) {
                next[i] += 1;
                round.push((i, di));
            }
        }
        if round.is_empty() {
            break;
        }
        let pairs: Vec<(usize, usize)> = round.iter().map(|&(i, di)| (new_files[i], di)).collect();
        hashes.read(&pairs);
        for (i, di) in round {
            if !used[di] && hashes.same(new_files[i], di) {
                found[i] = Some(di);
                used[di] = true;
            }
        }
        looking.retain(|&i| found[i].is_none());
    }
    found
}

/// Content samples (`scanner::sample_hash`) of source and archive files, each read once; None
/// for a file that could not be read.
struct Hashes<'a> {
    src: &'a Snapshot,
    dst: &'a Snapshot,
    src_root: &'a Path,
    dst_root: &'a Path,
    io: Io,
    src_hashes: HashMap<usize, Option<u64>>,
    dst_hashes: HashMap<usize, Option<u64>>,
}

impl<'a> Hashes<'a> {
    fn new(src: &'a Snapshot, dst: &'a Snapshot, src_root: &'a Path, dst_root: &'a Path, io: Io) -> Self {
        Hashes { src, dst, src_root, dst_root, io, src_hashes: HashMap::new(), dst_hashes: HashMap::new() }
    }

    /// Reads what is still missing of these pairs (source index, archive index): both sides at
    /// the same time unless they share a disk.
    fn read(&mut self, pairs: &[(usize, usize)]) {
        let todo_src = missing(&self.src_hashes, &self.src.files, pairs.iter().map(|p| p.0));
        let todo_dst = missing(&self.dst_hashes, &self.dst.files, pairs.iter().map(|p| p.1));
        let Hashes { src, dst, src_root, dst_root, io, .. } = *self;
        let read_src = || sample_hashes(src_root, &src.files, &todo_src, io.src_threads);
        let read_dst = || sample_hashes(dst_root, &dst.files, &todo_dst, io.dst_threads);
        let (s, d) = if io.same_disk {
            (read_src(), read_dst())
        } else {
            std::thread::scope(|scope| {
                let s = scope.spawn(read_src);
                let d = read_dst();
                (s.join().expect("hash thread panicked"), d)
            })
        };
        self.src_hashes.extend(s);
        self.dst_hashes.extend(d);
    }

    /// Both read and equal.
    fn same(&self, si: usize, di: usize) -> bool {
        let s = self.src_hashes.get(&si).copied().flatten();
        s.is_some() && s == self.dst_hashes.get(&di).copied().flatten()
    }
}

/// The files among `idx` not read yet, once each, in path order.
fn missing(known: &HashMap<usize, Option<u64>>, files: &[FileMeta], idx: impl Iterator<Item = usize>) -> Vec<usize> {
    let mut todo: Vec<usize> = idx.filter(|i| !known.contains_key(i)).collect();
    todo.sort_unstable_by(|&a, &b| files[a].rel.cmp(&files[b].rel).then(a.cmp(&b)));
    todo.dedup();
    todo
}

/// Content samples of these files, in the given order, `threads` at a time: each thread reads one
/// stretch of them, so a drive is read along its folders.
fn sample_hashes(root: &Path, files: &[FileMeta], todo: &[usize], threads: usize) -> Vec<(usize, Option<u64>)> {
    let hash = |i: usize| (i, scanner::sample_hash(&scanner::join(root, &files[i].rel), files[i].size).ok());
    if threads <= 1 || todo.len() < 2 {
        return todo.iter().map(|&i| hash(i)).collect();
    }
    let chunk = todo.len().div_ceil(threads);
    std::thread::scope(|s| {
        let handles: Vec<_> =
            todo.chunks(chunk).map(|part| s.spawn(move || part.iter().map(|&i| hash(i)).collect::<Vec<_>>())).collect();
        handles.into_iter().flat_map(|h| h.join().expect("hash thread panicked")).collect()
    })
}

/// Merges file moves into folder moves when *all* files of an archive folder moved into the same
/// new folder with an identical substructure.
/// Files that are deleted anyway do not count (deleting runs before moving).
fn collapse_dir_moves(
    moves: Vec<Move>,
    dst: &Snapshot,
    src_dirs: &HashSet<String>,
    deleted: &HashSet<String>,
    fold_case: bool,
) -> (Vec<DirMove>, Vec<Move>) {
    let key = |rel: &str| key(rel, fold_case);
    if moves.is_empty() {
        return (Vec::new(), moves);
    }
    let mut subtree_files: HashMap<String, u64> = HashMap::new();
    for f in &dst.files {
        let k = key(&f.rel);
        if deleted.contains(&k) {
            continue;
        }
        for (i, _) in k.match_indices('/') {
            *subtree_files.entry(k[..i].to_string()).or_default() += 1;
        }
    }
    let dst_paths: HashSet<String> =
        dst.files.iter().map(|f| key(&f.rel)).chain(dst.dirs.iter().map(|d| key(d))).collect();

    // Every move a/b/c.txt → x/b/c.txt counts for the pairs (a/b → x/b) and (a → x).
    let mut pairs: HashMap<(String, String), (String, String, Vec<usize>)> = HashMap::new();
    for (i, m) in moves.iter().enumerate() {
        let from: Vec<&str> = m.from.split('/').collect();
        let to: Vec<&str> = m.to.split('/').collect();
        let common = from.iter().rev().zip(to.iter().rev()).take_while(|(a, b)| key(a) == key(b)).count();
        for j in 1..=common {
            if j >= from.len() || j >= to.len() {
                break;
            }
            let f = from[..from.len() - j].join("/");
            let t = to[..to.len() - j].join("/");
            pairs.entry((key(&f), key(&t))).or_insert_with(|| (f, t, Vec::new())).2.push(i);
        }
    }

    let mut valid: Vec<_> = pairs
        .into_iter()
        .filter(|((fk, tk), (_, _, idx))| {
            subtree_files.get(fk).copied() == Some(idx.len() as u64)
                && !dst_paths.contains(tk)
                && !src_dirs.contains(fk)
                && !is_under(tk, fk)
                && !is_under(fk, tk)
        })
        .collect();
    // Top-level folders first, so a → x is chosen instead of many a/* → x/*
    valid.sort_by(|a, b| depth(&a.0 .0).cmp(&depth(&b.0 .0)).then_with(|| a.0.cmp(&b.0)));

    let mut consumed = vec![false; moves.len()];
    let mut accepted: Vec<(String, String)> = Vec::new();
    let mut dir_moves = Vec::new();
    for ((fk, tk), (f, t, idx)) in valid {
        let overlaps = accepted
            .iter()
            .any(|(af, at)| is_under(&fk, af) || is_under(af, &fk) || is_under(&tk, at) || is_under(at, &tk));
        if overlaps || idx.iter().any(|&i| consumed[i]) {
            continue;
        }
        for &i in &idx {
            consumed[i] = true;
        }
        dir_moves.push(DirMove {
            from: f,
            to: t,
            size: idx.iter().map(|&i| moves[i].size).sum(),
            files: idx.len() as u64,
        });
        accepted.push((fk, tk));
    }
    let rest = moves.into_iter().zip(consumed).filter(|(_, c)| !c).map(|(m, _)| m).collect();
    (dir_moves, rest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::executor;
    use std::fs;
    use std::path::PathBuf;

    const TOL: i64 = 2000;

    fn write(root: &Path, rel: &str, content: &str) {
        let p = scanner::join(root, rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    /// Rules for an archive on the usual file system of this system (NTFS, ext4 …). The source is
    /// read like an SSD, the archive like a hard disk, so both ways of reading are exercised.
    fn rules(mode: Mode) -> Rules {
        Rules {
            mode,
            tol: TOL,
            fold_case: VolumeLimits::default().case_insensitive,
            io: Io { src_threads: 2, dst_threads: 1, same_disk: false },
        }
    }

    fn plan(src: &Path, dst: &Path, mode: Mode) -> Diff {
        let s = scanner::scan(src, &AtomicBool::new(false)).unwrap();
        let d = scanner::scan(dst, &AtomicBool::new(false)).unwrap();
        diff(&s, &d, src, dst, rules(mode))
    }

    fn sync(src: &Path, dst: &Path, mode: Mode) -> Diff {
        let d = plan(src, dst, mode);
        for op in &d.ops {
            executor::apply(op, src, dst, &executor::Apply { verify: false }).unwrap();
        }
        d
    }

    /// The probe of a plan, on a diff.
    fn probe_diff(d: &mut Diff, src: &Path) {
        let paths: Vec<String> = d
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::Copy | OpKind::Update))
            .map(|op| op.path.clone())
            .collect();
        let mut failed = probe(src, &paths, &AtomicBool::new(false)).unwrap();
        block_ops(&mut d.ops, &mut d.summary, &mut d.blocked, |op| {
            failed.remove(&op.path).map(|e| Issue::from_io(op.path.clone(), Object::File, &e))
        });
    }

    fn kinds(d: &Diff) -> Vec<OpKind> {
        d.ops.iter().map(|o| o.kind).collect()
    }

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();
        write(&src, "docs/a.txt", "alpha");
        write(&src, "docs/sub/b.txt", "bravo bravo");
        write(&src, "c.bin", "charlie charlie charlie");
        (tmp, src, dst)
    }

    #[test]
    fn each_entry_syncs_its_own_source() {
        let (tmp, src, dst) = setup();
        let src2 = tmp.path().join("src2");
        write(&src2, "other.txt", "other");
        let dst2 = tmp.path().join("dst2");
        let entry = |id: &str, source: &Path, path: &Path| crate::model::config::Archive {
            id: id.into(),
            source: source.to_string_lossy().into(),
            path: path.to_string_lossy().into(),
            mode: Mode::Mirror,
            enabled: true,
        };
        let backup = Backup {
            id: "b".into(),
            name: "B".into(),
            archives: vec![entry("a1", &src, &dst), entry("a2", &src2, &dst2)],
            schedule: None,
            color: None,
            legacy_path: String::new(),
        };
        let plans = build_plans(&backup, TOL, &Scans::plain(&AtomicBool::new(false))).unwrap();
        assert_eq!(plans[0].summary.copy_files, 3);
        assert_eq!(plans[1].summary.copy_files, 1);
        assert_eq!(plans[1].source, src2.to_string_lossy());
    }

    #[test]
    fn a_check_counts_the_files_and_folders_it_compares() {
        let (_t, src, dst) = setup();
        write(&dst, "old/x.txt", "x");
        let backup: Backup = serde_json::from_value(serde_json::json!({
            "id": "b", "name": "B",
            "archives": [{ "id": "a", "source": src, "path": dst, "mode": "mirror" }]
        }))
        .unwrap();
        let stop = AtomicBool::new(false);
        let count = ScanCount::default();
        let plans = build_plans(&backup, TOL, &Scans { count: Some(&count), ..Scans::plain(&stop) }).unwrap();
        let p = &plans[0];
        // Source: a.txt, b.txt, c.bin in docs and docs/sub; archive: x.txt in old
        assert_eq!((p.src_files, p.src_dirs, p.dst_files, p.dst_dirs), (3, 2, 1, 1));
        assert_eq!(count.get(), (4, 3), "both trees, files and folders below their roots");
    }

    #[test]
    fn initial_copy_then_nothing_to_do() {
        let (_t, src, dst) = setup();
        let d = sync(&src, &dst, Mode::Mirror);
        assert_eq!(d.summary.copy_files, 3);
        assert_eq!(d.summary.dirs_created, 2);
        let again = plan(&src, &dst, Mode::Mirror);
        assert!(again.ops.is_empty(), "{:?}", kinds(&again));
        assert_eq!(again.summary.unchanged, 3);
    }

    #[test]
    fn renamed_folder_becomes_single_dir_move() {
        let (_t, src, dst) = setup();
        sync(&src, &dst, Mode::Mirror);
        fs::rename(src.join("docs"), src.join("papers")).unwrap();
        let d = sync(&src, &dst, Mode::Mirror);
        assert_eq!(kinds(&d), vec![OpKind::MoveDir]);
        assert_eq!(d.ops[0].from.as_deref(), Some("docs"));
        assert_eq!(d.ops[0].path, "papers");
        assert_eq!(d.summary.move_files, 2);
        assert!(plan(&src, &dst, Mode::Mirror).ops.is_empty());
    }

    #[test]
    fn renamed_folder_with_deleted_file_is_still_dir_move() {
        let (_t, src, dst) = setup();
        sync(&src, &dst, Mode::Mirror);
        fs::rename(src.join("docs"), src.join("papers")).unwrap();
        fs::remove_file(src.join("papers/a.txt")).unwrap();
        let d = sync(&src, &dst, Mode::Mirror);
        assert_eq!(kinds(&d), vec![OpKind::Delete, OpKind::MoveDir]);
        assert!(plan(&src, &dst, Mode::Mirror).ops.is_empty());
    }

    #[test]
    fn moved_file_is_moved_not_copied() {
        let (_t, src, dst) = setup();
        sync(&src, &dst, Mode::Mirror);
        fs::rename(src.join("c.bin"), src.join("docs").join("c-renamed.bin")).unwrap();
        let d = sync(&src, &dst, Mode::Mirror);
        assert_eq!(kinds(&d), vec![OpKind::Move]);
        assert_eq!(d.ops[0].from.as_deref(), Some("c.bin"));
        assert!(dst.join("docs/c-renamed.bin").exists());
    }

    #[test]
    fn changed_file_is_updated() {
        let (_t, src, dst) = setup();
        sync(&src, &dst, Mode::Mirror);
        write(&src, "docs/a.txt", "alpha, now longer");
        let d = sync(&src, &dst, Mode::Mirror);
        assert_eq!(kinds(&d), vec![OpKind::Update]);
        assert_eq!(fs::read_to_string(dst.join("docs/a.txt")).unwrap(), "alpha, now longer");
    }

    #[test]
    fn deletions_only_in_mirror_mode() {
        let (_t, src, dst) = setup();
        sync(&src, &dst, Mode::Mirror);
        fs::remove_dir_all(src.join("docs/sub")).unwrap();
        let additive = plan(&src, &dst, Mode::Additive);
        assert!(additive.ops.is_empty());
        let mirror = sync(&src, &dst, Mode::Mirror);
        assert_eq!(kinds(&mirror), vec![OpKind::Delete, OpKind::DeleteDir]);
        assert!(!dst.join("docs/sub").exists());
    }

    #[test]
    fn same_size_different_content_is_not_a_move() {
        let (_t, src, dst) = setup();
        sync(&src, &dst, Mode::Mirror);
        fs::remove_file(src.join("docs/a.txt")).unwrap();
        write(&src, "docs/z.txt", "ALPHA");
        let d = plan(&src, &dst, Mode::Mirror);
        assert!(!kinds(&d).contains(&OpKind::Move));
    }

    /// A folder "album" of `n` files of equal size (f00.txt …) written together, synced.
    fn synced_album(n: usize) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let (src, dst) = (tmp.path().join("src"), tmp.path().join("dst"));
        fs::create_dir_all(&dst).unwrap();
        for k in 0..n {
            write(&src, &format!("album/f{k:02}.txt"), &format!("content {k:02}"));
        }
        sync(&src, &dst, Mode::Mirror);
        (tmp, src, dst)
    }

    /// Gives a file this modification time.
    fn set_mtime(path: &Path, time: std::time::SystemTime) {
        fs::File::options().write(true).open(path).unwrap().set_modified(time).unwrap();
    }

    /// Replaces an archive file's content by other content of the same size and keeps its time:
    /// only reading it tells the difference.
    fn tamper(dst: &Path, rel: &str) {
        let p = scanner::join(dst, rel);
        let meta = fs::metadata(&p).unwrap();
        fs::write(&p, "X".repeat(meta.len() as usize)).unwrap();
        set_mtime(&p, meta.modified().unwrap());
    }

    fn paths_of(d: &Diff, kind: OpKind) -> Vec<&str> {
        d.ops.iter().filter(|o| o.kind == kind).map(|o| o.path.as_str()).collect()
    }

    #[test]
    fn moved_folder_is_confirmed_by_samples() {
        let (_t, src, dst) = synced_album(10);
        fs::rename(src.join("album"), src.join("album2")).unwrap();
        let d = plan(&src, &dst, Mode::Mirror);
        assert_eq!(kinds(&d), vec![OpKind::MoveDir]);
        assert_eq!(d.summary.move_files, 10);
    }

    #[test]
    fn a_failed_sample_checks_the_folder_file_by_file() {
        let (_t, src, dst) = synced_album(10);
        fs::rename(src.join("album"), src.join("album2")).unwrap();
        // f00 is the first sample of the group
        tamper(&dst, "album/f00.txt");
        let d = plan(&src, &dst, Mode::Mirror);
        assert_eq!(paths_of(&d, OpKind::Copy), ["album2/f00.txt"]);
        assert_eq!(paths_of(&d, OpKind::Delete), ["album/f00.txt"]);
        assert_eq!(d.summary.move_files, 9);
    }

    #[test]
    fn files_between_the_samples_are_trusted_by_name_size_and_time() {
        // The price of reading only samples: same name, size and time, but other content, inside
        // a folder whose samples all match, counts as moved
        let (_t, src, dst) = synced_album(10);
        fs::rename(src.join("album"), src.join("album2")).unwrap();
        tamper(&dst, "album/f01.txt");
        let d = plan(&src, &dst, Mode::Mirror);
        assert_eq!(d.summary.move_files, 10);
    }

    #[test]
    fn a_name_found_twice_is_decided_by_content() {
        let tmp = tempfile::tempdir().unwrap();
        let (src, dst) = (tmp.path().join("src"), tmp.path().join("dst"));
        write(&dst, "a/x.txt", "first!");
        write(&dst, "b/x.txt", "second");
        write(&src, "c/x.txt", "second");
        let time = fs::metadata(dst.join("a/x.txt")).unwrap().modified().unwrap();
        set_mtime(&dst.join("b/x.txt"), time);
        set_mtime(&src.join("c/x.txt"), time);
        let d = plan(&src, &dst, Mode::Mirror);
        assert_eq!(d.summary.move_files, 1);
        assert_eq!(paths_of(&d, OpKind::Delete), ["a/x.txt"]);
    }

    #[test]
    fn reading_follows_both_drives() {
        use crate::platform::volume::{Drive, DriveKind};
        let drive = |kind, disk: &str| Drive { kind, disk: disk.into() };
        let ssd = drive(DriveKind::Solid, "disk:0");
        let hdd = drive(DriveKind::Spinning, "disk:1");
        let stick = drive(DriveKind::Flash, "disk:2");
        let share = drive(DriveKind::Network, r"\\nas\backup");

        // Different disks: each side as its drive allows, both at the same time
        let io = Io::between(&ssd, &hdd);
        assert!(io.src_threads >= 2 && !io.same_disk);
        assert_eq!(io.dst_threads, 1);
        let io = Io::between(&hdd, &stick);
        assert_eq!((io.src_threads, io.dst_threads, io.same_disk), (1, 2, false));
        let io = Io::between(&ssd, &share);
        assert!(io.src_threads >= 2 && io.dst_threads >= 2 && !io.same_disk);

        // Two partitions of one hard disk: one file at a time, one side after the other
        let io = Io::between(&drive(DriveKind::Spinning, "disk:1"), &hdd);
        assert_eq!((io.src_threads, io.dst_threads, io.same_disk), (1, 1, true));
        // One SSD: both sides share it, still several at once
        let io = Io::between(&ssd, &ssd.clone());
        assert!(io.same_disk && io.src_threads >= 2);
    }

    fn meta(rel: &str, size: u64) -> FileMeta {
        FileMeta { rel: rel.into(), size, mtime: 1_000_000, ctime: None, encrypted: false }
    }

    fn empty_plan(src: &Snapshot, rules: Rules) -> Diff {
        diff(src, &Snapshot::default(), Path::new("/src"), Path::new("/archive"), rules)
    }

    #[test]
    fn archive_entries_below_unreadable_source_folders_stay() {
        let src = Snapshot {
            files: vec![meta("keep/x.txt", 1)],
            errors: vec![Issue::new("locked", Object::Folder, Reason::AccessDenied)],
            ..Default::default()
        };
        let dst = Snapshot {
            files: vec![meta("locked/a.txt", 1), meta("locked/deep/b.txt", 2), meta("lockedness.txt", 3)],
            dirs: vec!["locked".into(), "locked/deep".into()],
            ..Default::default()
        };
        let d = diff(&src, &dst, Path::new("/src"), Path::new("/archive"), rules(Mode::Mirror));
        assert_eq!(d.summary.protected, 2);
        // Only the file that merely starts with the same letters is deleted
        let deleted: Vec<_> = d.ops.iter().filter(|o| o.kind == OpKind::Delete).map(|o| o.path.as_str()).collect();
        assert_eq!(deleted, ["lockedness.txt"]);
        assert!(!d.ops.iter().any(|o| o.kind == OpKind::DeleteDir));
    }

    #[test]
    fn the_analysis_filters_rows_and_counts_them() {
        let (_t, src, dst) = setup();
        let backup: Backup = serde_json::from_value(serde_json::json!({
            "id": "b", "name": "B",
            "archives": [{ "id": "a", "source": src, "path": dst, "mode": "mirror" }]
        }))
        .unwrap();
        let plans = build_plans(&backup, TOL, &Scans::plain(&AtomicBool::new(false))).unwrap();
        let view = plans[0].view();
        assert_eq!(view.counts["copy"], 3);
        assert_eq!(view.counts["dirs"], 2, "docs and docs/sub are created");
        assert_eq!(view.counts["all"], view.ops);
        assert_eq!(plans[0].matching("copy", "").count(), 3);
        let found: Vec<_> = plans[0].matching("all", "SUB").map(|r| r.path()).collect();
        assert_eq!(found, ["docs/sub", "docs/sub/b.txt"], "the folder and its file, whatever the case");
    }

    #[test]
    fn paths_differing_only_in_case_are_listed_when_the_archive_ignores_case() {
        let src = Snapshot { files: vec![meta("Docs/a.txt", 1), meta("docs/A.txt", 2)], ..Default::default() };
        let d = empty_plan(&src, Rules { fold_case: true, ..rules(Mode::Mirror) });
        assert_eq!(d.ops.iter().filter(|o| o.kind == OpKind::Copy).count(), 1);
        assert_eq!(d.blocked.len(), 1);
        assert_eq!(d.blocked[0].reason, Reason::CaseConflict);
        assert_eq!(d.blocked[0].path, "docs/A.txt");
        assert_eq!(d.blocked[0].detail.as_deref(), Some("Docs/a.txt"));
        assert_eq!(d.summary.blocked, 1);

        // A case-sensitive archive (ext4) holds both
        let d = empty_plan(&src, Rules { fold_case: false, ..rules(Mode::Mirror) });
        assert!(d.blocked.is_empty());
        assert_eq!(d.summary.copy_files, 2);
    }

    #[test]
    fn case_insensitive_archive_pairs_paths_regardless_of_case() {
        let src = Snapshot { files: vec![meta("Readme.md", 5)], ..Default::default() };
        let dst = Snapshot { files: vec![meta("README.md", 5)], ..Default::default() };
        let d = diff(
            &src,
            &dst,
            Path::new("/src"),
            Path::new("/archive"),
            Rules { fold_case: true, ..rules(Mode::Mirror) },
        );
        // Only the case of the name changed: renamed in the archive, not copied again
        assert_eq!(kinds(&d), vec![OpKind::Move]);
    }

    #[test]
    fn names_a_fat_archive_refuses_are_listed() {
        let src = Snapshot {
            files: vec![meta("ok.txt", 1), meta("12:30.txt", 2), meta("what?/a.txt", 3)],
            dirs: vec!["what?".into()],
            ..Default::default()
        };
        let mut d = empty_plan(&src, rules(Mode::Mirror));
        apply_limits(&mut d, &src, VolumeLimits { restricted_names: true, ..VolumeLimits::default() });
        assert_eq!(d.summary.copy_files, 1);
        assert_eq!(d.summary.dirs_created, 0);
        let mut listed: Vec<_> = d.blocked.iter().map(|b| (b.path.as_str(), b.object, b.reason)).collect();
        listed.sort_by_key(|b| b.0);
        assert_eq!(
            listed,
            [
                ("12:30.txt", Object::File, Reason::NameNotAllowed),
                ("what?", Object::Folder, Reason::NameNotAllowed),
                ("what?/a.txt", Object::File, Reason::NameNotAllowed),
            ]
        );
    }

    #[test]
    fn files_too_large_for_the_archive_are_listed() {
        let src = Snapshot { files: vec![meta("small.bin", 10), meta("huge.iso", 5 << 30)], ..Default::default() };
        let mut d = empty_plan(&src, rules(Mode::Mirror));
        apply_limits(&mut d, &src, VolumeLimits { max_file_size: Some(u32::MAX as u64), ..VolumeLimits::default() });
        assert_eq!(d.summary.copy_files, 1);
        assert_eq!(d.summary.copy_bytes, 10);
        assert_eq!(d.blocked.len(), 1);
        assert_eq!((d.blocked[0].reason, d.blocked[0].size), (Reason::TooLarge, 5 << 30));
    }

    #[test]
    fn encrypted_files_need_an_archive_that_supports_encryption() {
        let mut secret = meta("secret.txt", 3);
        secret.encrypted = true;
        let src = Snapshot { files: vec![secret], ..Default::default() };
        let plan = |limits| {
            let mut d = empty_plan(&src, rules(Mode::Mirror));
            apply_limits(&mut d, &src, limits);
            d
        };
        let without = VolumeLimits { encryption: false, ..VolumeLimits::default() };
        assert_eq!(plan(without).blocked[0].reason, Reason::Encrypted);
        assert!(plan(VolumeLimits { encryption: true, ..VolumeLimits::default() }).blocked.is_empty());
    }

    #[test]
    #[cfg(windows)]
    fn locked_source_file_is_found_by_the_probe() {
        use std::os::windows::fs::OpenOptionsExt;
        let (_t, src, dst) = setup();
        // Held open without sharing, like Outlook does with its .pst
        let _lock = fs::OpenOptions::new().read(true).share_mode(0).open(src.join("c.bin")).unwrap();
        let mut d = plan(&src, &dst, Mode::Mirror);
        probe_diff(&mut d, &src);
        assert_eq!(d.summary.copy_files, 2);
        assert_eq!(d.blocked.len(), 1);
        assert_eq!(d.blocked[0].path, "c.bin");
        assert_eq!(d.blocked[0].reason, Reason::Locked);
        assert!(!d.ops.iter().any(|o| o.path == "c.bin"));
    }

    #[test]
    #[cfg(unix)]
    fn links_and_special_files_are_listed_not_copied() {
        let (_t, src, dst) = setup();
        std::os::unix::fs::symlink(src.join("c.bin"), src.join("link.bin")).unwrap();
        let fifo = std::ffi::CString::new(src.join("pipe").to_str().unwrap()).unwrap();
        // SAFETY: plain libc call with a valid C string
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        let d = plan(&src, &dst, Mode::Mirror);
        assert_eq!(d.summary.copy_files, 3);
        let mut listed: Vec<_> = d.blocked.iter().map(|b| (b.path.as_str(), b.reason, b.is_failure())).collect();
        listed.sort_by_key(|b| b.0);
        assert_eq!(listed, [("link.bin", Reason::Link, false), ("pipe", Reason::Special, false)]);
    }

    #[test]
    #[cfg(unix)]
    fn unreadable_source_file_is_found_by_the_probe() {
        use std::os::unix::fs::PermissionsExt;
        // root reads everything, so there is nothing to find
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let (_t, src, dst) = setup();
        fs::set_permissions(src.join("c.bin"), fs::Permissions::from_mode(0o000)).unwrap();
        let mut d = plan(&src, &dst, Mode::Mirror);
        probe_diff(&mut d, &src);
        assert_eq!(d.blocked.len(), 1);
        assert_eq!((d.blocked[0].path.as_str(), d.blocked[0].reason), ("c.bin", Reason::AccessDenied));
        // No restart with administrator rights on Linux
        assert!(!d.blocked[0].admin);
    }

    #[test]
    fn empty_source_is_flagged_dangerous() {
        let (_t, src, dst) = setup();
        sync(&src, &dst, Mode::Mirror);
        fs::remove_dir_all(&src).unwrap();
        fs::create_dir_all(&src).unwrap();
        assert!(plan(&src, &dst, Mode::Mirror).dangerous);
    }
}
