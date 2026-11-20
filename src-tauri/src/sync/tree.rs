//! Folder trees as scans read them, and keeping them between scans so the next scan only reads
//! the folders in which something changed.
//!
//! A scan reads folder by folder (`platform::fs::list_dir`), several at once where the drive
//! benefits (`platform::volume::scan_threads`). The result is kept per root folder in the data
//! folder (`scan-cache/`). The next scan of that root asks what changed since:
//!
//! - **Change journal** (Windows, NTFS; sources and archives): the journal names every folder in
//!   which something was created, deleted, renamed or written since the last scan
//!   (`platform::journal`). Only those folders are read again. This is exact: it also sees a file
//!   that was changed in place.
//! - **Folder times** (Linux, archives only, not on FAT/exFAT): a folder's modification time
//!   changes when an entry in it is added, removed or renamed, so only folders with a new time are
//!   read again. A file changed in place does not change its folder's time; that is acceptable for
//!   an archive, which only Diskman writes, but not for a source. Sources on Linux are always read
//!   in full.
//! - Otherwise there is no cache: every scan reads everything.
//!
//! Whatever the journal or the times cannot vouch for (a new journal, a root that was replaced,
//! folders that could not be read) is read in full, and once a week every root is read in full
//! regardless. A broken or missing cache file only means a full scan.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::check_stop;
use super::issues::{Issue, Object, Reason};
use super::scanner::{is_leftover, FileMeta, Snapshot, TRASH_DIR};
use crate::platform;
use crate::platform::fs::{EntryKind, EntryMeta};
use crate::platform::journal::{self, Position};

/// After this long a root is read in full again, whatever the journal or the times say.
const FULL_SCAN_EVERY: Duration = Duration::from_secs(7 * 24 * 3600);
/// Folder times this close to the last scan may hide a change made in the same moment.
const RACY_MS: i64 = 2000;
const CACHE_MAGIC: &[u8; 4] = b"DMSC";
// 3: system folders and files at the top of a root are no longer part of a tree
const CACHE_VERSION: u32 = 3;

/// A file directly in a folder.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub size: u64,
    pub mtime: i64,
    pub ctime: Option<i64>,
    pub encrypted: bool,
}

/// One folder of a tree: what is directly in it.
#[derive(Debug, Clone, Default)]
pub struct DirNode {
    /// Identity on its volume (`EntryMeta::id`) and modification time, as its parent listed it
    pub id: u64,
    pub mtime: i64,
    pub files: Vec<FileEntry>,
    /// Names of the folders in it (each has a node of its own)
    pub subdirs: Vec<String>,
    /// Temporary files of unfinished copies
    pub leftovers: Vec<String>,
    /// Entries that could not be read or are skipped on purpose, with paths relative to the root.
    /// Not kept in the cache file: a folder with issues is read again by the next scan.
    pub issues: Vec<Issue>,
    /// The folder itself could not be read; read again by every scan
    pub unreadable: bool,
    /// Loaded from a cache file while it had issues, which the file does not keep: read again
    pub stale: bool,
}

/// A tree below a root folder: the folders by their path relative to it ("" = the root).
#[derive(Debug, Clone, Default)]
pub struct Tree {
    pub dirs: HashMap<String, DirNode>,
}

fn join_rel(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

fn abs(root: &Path, rel: &str) -> PathBuf {
    super::scanner::join(root, rel)
}

impl Tree {
    /// The flat view the comparison works on.
    pub fn snapshot(&self) -> Snapshot {
        let mut snap = Snapshot::default();
        let mut rels: Vec<&String> = self.dirs.keys().collect();
        rels.sort();
        for rel in rels {
            let node = &self.dirs[rel];
            if !rel.is_empty() {
                snap.dirs.push(rel.clone());
            }
            for f in &node.files {
                snap.files.push(FileMeta {
                    rel: join_rel(rel, &f.name),
                    size: f.size,
                    mtime: f.mtime,
                    ctime: f.ctime,
                    encrypted: f.encrypted,
                });
            }
            snap.leftovers.extend(node.leftovers.iter().map(|n| join_rel(rel, n)));
            for issue in &node.issues {
                if matches!(issue.reason, Reason::Link | Reason::Special) {
                    snap.skipped.push(issue.clone());
                } else {
                    snap.errors.push(issue.clone());
                }
            }
        }
        snap
    }

    /// Removes a folder and everything below it.
    fn remove_subtree(&mut self, rel: &str) {
        if let Some(node) = self.dirs.remove(rel) {
            for name in node.subdirs {
                self.remove_subtree(&join_rel(rel, &name));
            }
        }
    }
}

/// Folders and files read so far by the scans of one check, for its live progress (polled by the
/// UI). Below the root folders; at the end of the scans it is set to the totals of the trees
/// (`set`), so a scan that only read what changed still ends with everything the check covers.
#[derive(Debug, Default)]
pub struct ScanCount {
    files: AtomicU64,
    dirs: AtomicU64,
}

impl ScanCount {
    fn add(&self, node: &DirNode, is_root: bool) {
        self.files.fetch_add(node.files.len() as u64, Ordering::Relaxed);
        if !is_root {
            self.dirs.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn set(&self, files: u64, dirs: u64) {
        self.files.store(files, Ordering::Relaxed);
        self.dirs.store(dirs, Ordering::Relaxed);
    }

    /// Files and folders.
    pub fn get(&self) -> (u64, u64) {
        (self.files.load(Ordering::Relaxed), self.dirs.load(Ordering::Relaxed))
    }
}

/// A folder still to read: its path, identity and time as its parent listed them.
type Job = (String, u64, i64);

/// Reads one folder: its node and the folders in it, still to be read.
fn read_node(root: &Path, rel: &str, id: u64, mtime: i64) -> (DirNode, Vec<Job>) {
    let mut node = DirNode { id, mtime, ..Default::default() };
    let mut children = Vec::new();
    let listed = match platform::fs::list_dir(&abs(root, rel)) {
        Ok(l) => l,
        Err(e) => {
            node.unreadable = true;
            node.issues.push(Issue::from_io(rel, Object::Folder, &e));
            return (node, children);
        }
    };
    for entry in listed {
        let name = match entry.name {
            Ok(n) => n,
            Err(shown) => {
                let object = if entry.kind == EntryKind::Dir { Object::Folder } else { Object::File };
                node.issues.push(Issue::new(join_rel(rel, &shown), object, Reason::InvalidName));
                continue;
            }
        };
        let path = join_rel(rel, &name);
        match entry.kind {
            // The trash of an archive (sync::executor) is not part of it, nor are the folders and
            // files the system keeps at the top of a drive (also when the root is not a drive's
            // top: an entry of that name directly in it is one of them as good as always)
            EntryKind::Dir if rel.is_empty() && (name == TRASH_DIR || platform::fs::is_system_dir(&name)) => {}
            EntryKind::Dir => {
                let (cid, cmtime) = entry.meta.as_ref().map(|m| (m.id, m.mtime)).unwrap_or((0, 0));
                node.subdirs.push(name);
                children.push((path, cid, cmtime));
            }
            EntryKind::Link => node.issues.push(Issue::new(path, Object::Link, Reason::Link)),
            EntryKind::Special => node.issues.push(Issue::new(path, Object::Special, Reason::Special)),
            EntryKind::File if rel.is_empty() && platform::fs::is_system_file(&name) => {}
            EntryKind::File if is_leftover(&name) => node.leftovers.push(name),
            EntryKind::File => match entry.meta {
                Ok(EntryMeta { size, mtime, ctime, encrypted, .. }) => {
                    node.files.push(FileEntry { name, size, mtime, ctime, encrypted })
                }
                Err(e) => node.issues.push(Issue::from_io(path, Object::File, &e)),
            },
        }
    }
    (node, children)
}

/// Reads the folders `starts` and everything below them, `threads` folders at a time; counts what
/// it read in `count`.
fn walk(
    root: &Path,
    starts: Vec<Job>,
    threads: usize,
    stop: &AtomicBool,
    count: Option<&ScanCount>,
) -> anyhow::Result<HashMap<String, DirNode>> {
    struct Queue {
        todo: Vec<Job>,
        busy: usize,
    }
    let queue = Mutex::new(Queue { todo: starts, busy: 0 });
    let wake = Condvar::new();
    let out = Mutex::new(HashMap::new());
    std::thread::scope(|s| {
        for _ in 0..threads.max(1) {
            s.spawn(|| loop {
                let job = {
                    let mut q = queue.lock().unwrap();
                    loop {
                        if stop.load(Ordering::Relaxed) {
                            wake.notify_all();
                            return;
                        }
                        // Last in, first out: one thread goes depth first, which suits spinning disks
                        if let Some(job) = q.todo.pop() {
                            q.busy += 1;
                            break job;
                        }
                        if q.busy == 0 {
                            wake.notify_all();
                            return;
                        }
                        q = wake.wait_timeout(q, Duration::from_millis(100)).unwrap().0;
                    }
                };
                let (node, children) = read_node(root, &job.0, job.1, job.2);
                if let Some(c) = count {
                    c.add(&node, job.0.is_empty());
                }
                out.lock().unwrap().insert(job.0, node);
                let mut q = queue.lock().unwrap();
                q.todo.extend(children);
                q.busy -= 1;
                wake.notify_all();
            });
        }
    });
    check_stop(stop)?;
    Ok(out.into_inner().unwrap())
}

/// Reads a whole tree (without counting).
#[cfg(test)]
pub fn read_tree(root: &Path, threads: usize, stop: &AtomicBool) -> anyhow::Result<Tree> {
    read_tree_counted(root, threads, stop, None)
}

/// Reads a whole tree and counts what it read in `count`.
pub fn read_tree_counted(
    root: &Path,
    threads: usize,
    stop: &AtomicBool,
    count: Option<&ScanCount>,
) -> anyhow::Result<Tree> {
    if !root.is_dir() {
        anyhow::bail!("Folder not found or not reachable: {}", root.display());
    }
    let meta = platform::fs::dir_meta(root).ok();
    let (id, mtime) = meta.map(|m| (m.id, m.mtime)).unwrap_or((0, 0));
    Ok(Tree { dirs: walk(root, vec![(String::new(), id, mtime)], threads, stop, count)? })
}

/// Reads the folders `dirty` again and what is new below them; everything else stays as it was.
/// A folder below a dirty one counts as the same as before when its id is (and is then only read
/// again if it is dirty itself): Linux reuses the inode number of a deleted folder for one created
/// right after, so a replaced folder has to be in `dirty` too, as both watchers report it.
fn refresh(
    root: &Path,
    tree: &mut Tree,
    mut dirty: Vec<String>,
    threads: usize,
    stop: &AtomicBool,
    count: Option<&ScanCount>,
) -> anyhow::Result<()> {
    // Parents first: a folder they no longer have is gone before it would be read
    dirty.sort_by_key(|rel| (rel.matches('/').count() + usize::from(!rel.is_empty()), rel.clone()));
    dirty.dedup();
    let mut new_trees: Vec<Job> = Vec::new();
    for rel in dirty {
        check_stop(stop)?;
        let Some(old) = tree.dirs.get(&rel) else { continue };
        let now = match platform::fs::dir_meta(&abs(root, &rel)) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound && !rel.is_empty() => {
                // Gone: its parent changed as well; drop it there too in case that is not read
                tree.remove_subtree(&rel);
                let (parent, name) = rel.rsplit_once('/').unwrap_or(("", rel.as_str()));
                if let Some(p) = tree.dirs.get_mut(parent) {
                    p.subdirs.retain(|n| n != name);
                }
                continue;
            }
            // Not readable now: its listing says so
            Err(_) => EntryMeta { id: old.id, mtime: old.mtime, size: 0, ctime: None, encrypted: false },
        };
        if old.id != 0 && now.id != old.id {
            // Another folder under the same name: read it as new
            tree.remove_subtree(&rel);
            new_trees.push((rel, now.id, now.mtime));
            continue;
        }
        let old_subdirs = old.subdirs.clone();
        let (node, children) = read_node(root, &rel, now.id, now.mtime);
        if let Some(c) = count {
            c.add(&node, rel.is_empty());
        }
        for (child, cid, cmtime) in children {
            match tree.dirs.get(&child) {
                // The same folder as before: what is below it is known
                Some(c) if c.id == cid => {}
                _ => {
                    tree.remove_subtree(&child);
                    new_trees.push((child, cid, cmtime));
                }
            }
        }
        for name in old_subdirs.iter().filter(|n| !node.subdirs.contains(n)) {
            tree.remove_subtree(&join_rel(&rel, name));
        }
        tree.dirs.insert(rel, node);
    }
    if !new_trees.is_empty() {
        tree.dirs.extend(walk(root, new_trees, threads, stop, count)?);
    }
    Ok(())
}

/// What a root folder is to the backup: sources are read with the journal only, archives may also
/// rely on folder times (see the module notes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Source,
    Archive,
}

/// How a scan came to its result, for the analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanMode {
    /// Read in full
    Full,
    /// Only changed folders read (journal or folder times)
    Incremental,
    /// Taken from another backup of the same "Check all" that has the same folder
    Shared,
}

/// A scan's result.
pub struct Scanned {
    pub snapshot: Arc<Snapshot>,
    pub mode: ScanMode,
    pub ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Watch {
    Journal(Position),
    DirTimes,
}

/// A kept tree with what is needed to tell what changed since.
struct Cached {
    watch: Watch,
    /// When the tree was read (start of the scan), ms since the epoch
    scanned_at: i64,
    /// When it was last read in full
    full_at: i64,
    tree: Tree,
}

/// The kept trees (in the data folder) and the turns scans take. One per app (`AppState`).
pub struct ScanCache {
    /// Folder of the cache files; None: nothing is kept (tests)
    dir: Option<PathBuf>,
    /// One scan per root at a time, so a run and a check do not write the same cache file
    roots: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// One check per volume at a time (`turns`): checks of different drives run side by side
    volumes: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// Snapshots shared within one "Check all" (by its number), so a source of two backups is
    /// read once
    shared: Mutex<HashMap<(u64, String), Arc<Snapshot>>>,
}

fn now_ms() -> i64 {
    platform::fs::millis(std::time::SystemTime::now())
}

fn lock_of(map: &Mutex<HashMap<String, Arc<Mutex<()>>>>, key: &str) -> Arc<Mutex<()>> {
    map.lock().unwrap().entry(key.to_string()).or_default().clone()
}

impl ScanCache {
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self {
            dir,
            roots: Mutex::new(HashMap::new()),
            volumes: Mutex::new(HashMap::new()),
            shared: Mutex::new(HashMap::new()),
        }
    }

    /// The locks of the physical disks of these folders (partitions of one disk share it), in a
    /// fixed order (no two checks can wait for each other). Lock each of them for as long as the
    /// check reads.
    pub fn turns<'a>(&self, roots: impl IntoIterator<Item = &'a str>) -> Vec<Arc<Mutex<()>>> {
        let mut ids: Vec<String> = roots.into_iter().map(|r| platform::volume::drive(Path::new(r)).disk).collect();
        ids.sort();
        ids.dedup();
        ids.iter().map(|id| lock_of(&self.volumes, id)).collect()
    }

    /// Forgets what "Check all" number `batch` shared.
    pub fn end_batch(&self, batch: u64) {
        self.shared.lock().unwrap().retain(|(b, _), _| *b != batch);
    }

    fn file_of(&self, root: &Path) -> Option<PathBuf> {
        let key = root.to_string_lossy();
        let hash = xxhash_rust::xxh3::xxh3_64(key.as_bytes());
        self.dir.as_ref().map(|d| d.join(format!("{hash:016x}.bin")))
    }

    /// `scan_counted` without counting.
    #[cfg(test)]
    pub fn scan(&self, root: &Path, role: Role, batch: Option<u64>, stop: &AtomicBool) -> anyhow::Result<Scanned> {
        self.scan_counted(root, role, batch, stop, None)
    }

    /// Reads the tree below `root`: only what changed when a kept tree can be trusted (see the
    /// module notes), otherwise all of it. `batch`: the number of a "Check all" whose other
    /// backups may share the result. Counts the folders and files it reads in `count`.
    pub fn scan_counted(
        &self,
        root: &Path,
        role: Role,
        batch: Option<u64>,
        stop: &AtomicBool,
        count: Option<&ScanCount>,
    ) -> anyhow::Result<Scanned> {
        let started = Instant::now();
        let key = root.to_string_lossy().into_owned();
        let turn = lock_of(&self.roots, &key);
        let _turn = turn.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(b) = batch {
            if let Some(snap) = self.shared.lock().unwrap().get(&(b, key.clone())) {
                return Ok(Scanned { snapshot: snap.clone(), mode: ScanMode::Shared, ms: 0 });
            }
        }
        let threads = platform::volume::scan_threads(root);
        let watch = match journal::position(root) {
            Some(pos) => Some(Watch::Journal(pos)),
            None if role == Role::Archive && platform::volume::dir_times_reliable(root) => Some(Watch::DirTimes),
            None => None,
        };
        let scanned_at = now_ms();
        let file = self.file_of(root);

        let (tree, mode, full_at) = match (watch, file.as_deref().and_then(|f| load(f, root))) {
            (Some(watch), Some(cached)) if self.usable(&cached, watch, root, scanned_at) => {
                let mut cached = cached;
                match self.dirty(root, &cached, watch) {
                    Some(dirty) => {
                        refresh(root, &mut cached.tree, dirty, threads, stop, count)?;
                        (cached.tree, ScanMode::Incremental, cached.full_at)
                    }
                    None => (read_tree_counted(root, threads, stop, count)?, ScanMode::Full, scanned_at),
                }
            }
            _ => (read_tree_counted(root, threads, stop, count)?, ScanMode::Full, scanned_at),
        };

        if let Some(f) = &file {
            match watch {
                Some(watch) => {
                    let cached = Cached { watch, scanned_at, full_at, tree };
                    if let Err(e) = save(f, root, &cached) {
                        eprintln!("Scan cache of {} not written: {e}", root.display());
                    }
                    return Ok(self.finish(cached.tree, mode, started, batch, key));
                }
                None => {
                    let _ = fs::remove_file(f);
                }
            }
        }
        Ok(self.finish(tree, mode, started, batch, key))
    }

    fn finish(&self, tree: Tree, mode: ScanMode, started: Instant, batch: Option<u64>, key: String) -> Scanned {
        let snapshot = Arc::new(tree.snapshot());
        if let Some(b) = batch {
            self.shared.lock().unwrap().insert((b, key), snapshot.clone());
        }
        Scanned { snapshot, mode, ms: started.elapsed().as_millis() as u64 }
    }

    /// A kept tree is used only when it was kept the same way, its root is still the same folder
    /// and it was read in full not too long ago.
    fn usable(&self, cached: &Cached, watch: Watch, root: &Path, now: i64) -> bool {
        let same_kind = matches!(
            (cached.watch, watch),
            (Watch::Journal(a), Watch::Journal(b)) if a.journal == b.journal
        ) || (cached.watch == Watch::DirTimes && watch == Watch::DirTimes);
        let root_id = cached.tree.dirs.get("").map(|n| n.id);
        let same_root = platform::fs::dir_meta(root).ok().map(|m| m.id) == root_id;
        same_kind && same_root && now - cached.full_at < FULL_SCAN_EVERY.as_millis() as i64
    }

    /// Folders to read again; None when only a full scan can tell.
    fn dirty(&self, root: &Path, cached: &Cached, watch: Watch) -> Option<Vec<String>> {
        let tree = &cached.tree;
        // Always: folders that could not be read or had entries with issues (not kept in the file)
        let mut dirty: Vec<String> =
            tree.dirs.iter().filter(|(_, n)| n.unreadable || n.stale).map(|(rel, _)| rel.clone()).collect();
        match (cached.watch, watch) {
            (Watch::Journal(since), Watch::Journal(_)) => {
                let changed = journal::changed_folders(root, since)?;
                if changed.is_empty() {
                    return Some(dirty);
                }
                dirty.extend(tree.dirs.iter().filter(|(_, n)| changed.contains(&n.id)).map(|(rel, _)| rel.clone()));
            }
            (Watch::DirTimes, Watch::DirTimes) => {
                for (rel, node) in &tree.dirs {
                    let changed = match platform::fs::dir_meta(&abs(root, rel)) {
                        Ok(m) => m.id != node.id || m.mtime != node.mtime || m.mtime >= cached.scanned_at - RACY_MS,
                        Err(_) => true,
                    };
                    if changed {
                        dirty.push(rel.clone());
                    }
                }
            }
            _ => return None,
        }
        Some(dirty)
    }
}

// ---- The cache file: a simple binary format, rewritten whole after each scan ----

struct Writer<W: Write>(W);

impl<W: Write> Writer<W> {
    fn u8(&mut self, v: u8) -> io::Result<()> {
        self.0.write_all(&[v])
    }
    fn u32(&mut self, v: u32) -> io::Result<()> {
        self.0.write_all(&v.to_le_bytes())
    }
    fn u64(&mut self, v: u64) -> io::Result<()> {
        self.0.write_all(&v.to_le_bytes())
    }
    fn i64(&mut self, v: i64) -> io::Result<()> {
        self.0.write_all(&v.to_le_bytes())
    }
    fn str(&mut self, s: &str) -> io::Result<()> {
        self.u32(s.len() as u32)?;
        self.0.write_all(s.as_bytes())
    }
}

struct Reader<R: Read>(R);

impl<R: Read> Reader<R> {
    fn bytes<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        let mut b = [0u8; N];
        self.0.read_exact(&mut b)?;
        Ok(b)
    }
    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.bytes::<1>()?[0])
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.bytes()?))
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.bytes()?))
    }
    fn i64(&mut self) -> io::Result<i64> {
        Ok(i64::from_le_bytes(self.bytes()?))
    }
    fn str(&mut self) -> io::Result<String> {
        let len = self.u32()? as usize;
        let mut b = vec![0u8; len];
        self.0.read_exact(&mut b)?;
        String::from_utf8(b).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

fn save(file: &Path, root: &Path, cached: &Cached) -> io::Result<()> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = file.with_extension("tmp");
    {
        let mut w = Writer(io::BufWriter::new(fs::File::create(&tmp)?));
        w.0.write_all(CACHE_MAGIC)?;
        w.u32(CACHE_VERSION)?;
        w.str(&root.to_string_lossy())?;
        match cached.watch {
            Watch::Journal(p) => {
                w.u8(1)?;
                w.u64(p.journal)?;
                w.i64(p.usn)?;
            }
            Watch::DirTimes => w.u8(2)?,
        }
        w.i64(cached.scanned_at)?;
        w.i64(cached.full_at)?;
        w.u64(cached.tree.dirs.len() as u64)?;
        for (rel, n) in &cached.tree.dirs {
            w.str(rel)?;
            w.u64(n.id)?;
            w.i64(n.mtime)?;
            w.u8(u8::from(n.unreadable) | (u8::from(!n.issues.is_empty()) << 1))?;
            w.u32(n.subdirs.len() as u32)?;
            for s in &n.subdirs {
                w.str(s)?;
            }
            w.u32(n.leftovers.len() as u32)?;
            for s in &n.leftovers {
                w.str(s)?;
            }
            w.u32(n.files.len() as u32)?;
            for f in &n.files {
                w.str(&f.name)?;
                w.u64(f.size)?;
                w.i64(f.mtime)?;
                w.u8(u8::from(f.ctime.is_some()) | (u8::from(f.encrypted) << 1))?;
                if let Some(c) = f.ctime {
                    w.i64(c)?;
                }
            }
        }
        w.0.flush()?;
    }
    fs::rename(tmp, file)
}

fn load(file: &Path, root: &Path) -> Option<Cached> {
    let mut r = Reader(io::BufReader::new(fs::File::open(file).ok()?));
    let read = |r: &mut Reader<io::BufReader<fs::File>>| -> io::Result<Option<Cached>> {
        if &r.bytes::<4>()? != CACHE_MAGIC || r.u32()? != CACHE_VERSION || r.str()? != root.to_string_lossy() {
            return Ok(None);
        }
        let watch = match r.u8()? {
            1 => Watch::Journal(Position { journal: r.u64()?, usn: r.i64()? }),
            2 => Watch::DirTimes,
            _ => return Ok(None),
        };
        let scanned_at = r.i64()?;
        let full_at = r.i64()?;
        let count = r.u64()? as usize;
        let mut dirs = HashMap::with_capacity(count);
        for _ in 0..count {
            let rel = r.str()?;
            let id = r.u64()?;
            let mtime = r.i64()?;
            let flags = r.u8()?;
            let mut node =
                DirNode { id, mtime, unreadable: flags & 1 != 0, stale: flags & 2 != 0, ..Default::default() };
            for _ in 0..r.u32()? {
                node.subdirs.push(r.str()?);
            }
            for _ in 0..r.u32()? {
                node.leftovers.push(r.str()?);
            }
            for _ in 0..r.u32()? {
                let name = r.str()?;
                let size = r.u64()?;
                let mtime = r.i64()?;
                let f = r.u8()?;
                let ctime = if f & 1 != 0 { Some(r.i64()?) } else { None };
                node.files.push(FileEntry { name, size, mtime, ctime, encrypted: f & 2 != 0 });
            }
            dirs.insert(rel, node);
        }
        Ok(Some(Cached { watch, scanned_at, full_at, tree: Tree { dirs } }))
    };
    read(&mut r).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, content: &str) {
        let p = abs(root, rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    fn files(snap: &Snapshot) -> Vec<(String, u64)> {
        let mut v: Vec<_> = snap.files.iter().map(|f| (f.rel.clone(), f.size)).collect();
        v.sort();
        v
    }

    #[test]
    fn a_parallel_scan_reads_the_same_as_a_single_one() {
        let tmp = tempfile::tempdir().unwrap();
        for i in 0..30 {
            write(tmp.path(), &format!("d{}/e{}/f{i}.txt", i % 5, i % 3), &"x".repeat(i));
        }
        let stop = AtomicBool::new(false);
        let one = read_tree(tmp.path(), 1, &stop).unwrap().snapshot();
        let many = read_tree(tmp.path(), 8, &stop).unwrap().snapshot();
        assert_eq!(files(&one), files(&many));
        assert_eq!(one.dirs, many.dirs);
        assert_eq!(one.files.len(), 30);
    }

    #[test]
    fn system_folders_at_the_top_are_not_part_of_the_tree() {
        #[cfg(windows)]
        const SYSTEM: &str = "$RECYCLE.BIN";
        #[cfg(not(windows))]
        const SYSTEM: &str = "lost+found";
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), &format!("{SYSTEM}/deleted.txt"), "x");
        write(tmp.path(), &format!("{}/old.txt", SYSTEM.to_lowercase()), "x");
        // Deeper down a folder of that name is the user's
        write(tmp.path(), &format!("keep/{SYSTEM}/mine.txt"), "x");
        #[cfg(windows)]
        {
            write(tmp.path(), "DumpStack.log.tmp", "x");
            write(tmp.path(), "keep/pagefile.sys", "x");
        }
        let snap = read_tree(tmp.path(), 2, &AtomicBool::new(false)).unwrap().snapshot();
        #[cfg(windows)]
        assert_eq!(files(&snap), [(format!("keep/{SYSTEM}/mine.txt"), 1), ("keep/pagefile.sys".into(), 1)]);
        #[cfg(not(windows))]
        assert_eq!(files(&snap), [(format!("keep/{SYSTEM}/mine.txt"), 1)]);
        assert_eq!(snap.dirs, ["keep".to_string(), format!("keep/{SYSTEM}")]);
    }

    #[test]
    fn refreshing_changed_folders_matches_a_full_scan() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "a/1.txt", "1");
        write(root, "a/b/2.txt", "22");
        write(root, "c/3.txt", "333");
        let stop = AtomicBool::new(false);
        let mut tree = read_tree(root, 2, &stop).unwrap();

        // A file changed in a, folder c replaced by a new one with other content, a new folder d
        write(root, "a/1.txt", "1111");
        fs::remove_dir_all(root.join("c")).unwrap();
        write(root, "c/4.txt", "4444");
        write(root, "d/e/5.txt", "55555");
        // What a watcher reports: a, the root (c and d came and went in it), and c itself (the
        // journal saw the old c go, its folder time changed). On Linux the new c often gets the
        // inode number of the old one, so only its own entry here makes it read again.
        refresh(root, &mut tree, vec!["a".into(), String::new(), "c".into()], 2, &stop, None).unwrap();

        let full = read_tree(root, 1, &stop).unwrap().snapshot();
        let incremental = tree.snapshot();
        assert_eq!(files(&incremental), files(&full));
        assert_eq!(incremental.dirs, full.dirs);
    }

    #[test]
    fn the_cache_file_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        write(&root, "a/1.txt", "1");
        write(&root, ".x.diskman-tmp", "half");
        let tree = read_tree(&root, 1, &AtomicBool::new(false)).unwrap();
        let cached = Cached { watch: Watch::DirTimes, scanned_at: 5, full_at: 3, tree };
        let file = tmp.path().join("cache.bin");
        save(&file, &root, &cached).unwrap();
        let back = load(&file, &root).unwrap();
        assert_eq!(back.watch, Watch::DirTimes);
        assert_eq!((back.scanned_at, back.full_at), (5, 3));
        assert_eq!(files(&back.tree.snapshot()), files(&cached.tree.snapshot()));
        assert_eq!(back.tree.snapshot().leftovers, cached.tree.snapshot().leftovers);
        assert!(load(&file, &tmp.path().join("other")).is_none(), "the file of another root");
    }

    #[test]
    fn a_cached_scan_sees_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        write(&root, "a/1.txt", "1");
        let cache = ScanCache::new(Some(tmp.path().join("cache")));
        let stop = AtomicBool::new(false);
        let first = cache.scan(&root, Role::Archive, None, &stop).unwrap();
        assert_eq!(first.mode, ScanMode::Full);
        write(&root, "a/2.txt", "22");
        write(&root, "b/3.txt", "333");
        let second = cache.scan(&root, Role::Archive, None, &stop).unwrap();
        assert_eq!(second.mode, ScanMode::Incremental);
        assert_eq!(files(&second.snapshot), [("a/1.txt".into(), 1), ("a/2.txt".into(), 2), ("b/3.txt".into(), 3)]);
    }

    /// A file written in place changes nothing about its folder; the NTFS journal still names it.
    #[cfg(windows)]
    #[test]
    fn the_journal_sees_a_file_changed_in_place() {
        use std::io::Write as _;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        write(&root, "a/1.txt", "1");
        if journal::position(&root).is_none() {
            return; // not on NTFS
        }
        let cache = ScanCache::new(Some(tmp.path().join("cache")));
        let stop = AtomicBool::new(false);
        cache.scan(&root, Role::Source, None, &stop).unwrap();
        fs::OpenOptions::new().append(true).open(root.join("a/1.txt")).unwrap().write_all(b"23").unwrap();
        let second = cache.scan(&root, Role::Source, None, &stop).unwrap();
        assert_eq!(second.mode, ScanMode::Incremental);
        assert_eq!(files(&second.snapshot), [("a/1.txt".into(), 3)]);
    }

    /// Timing of the kinds of scans on a real folder:
    /// `DISKMAN_BENCH_DIR=<folder> cargo test --release bench_scans -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn bench_scans() {
        let Ok(dir) = std::env::var("DISKMAN_BENCH_DIR") else { return };
        let root = PathBuf::from(dir);
        let stop = AtomicBool::new(false);
        let time = |label: &str, f: &dyn Fn() -> usize| {
            let started = Instant::now();
            let n = f();
            println!("{label:<40} {:>8} ms  {n} files", started.elapsed().as_millis());
        };
        let old = || {
            let walk = walkdir::WalkDir::new(&root).follow_links(false).min_depth(1);
            walk.into_iter().filter_map(Result::ok).filter(|e| e.file_type().is_file() && e.metadata().is_ok()).count()
        };
        time("walkdir + metadata (as before)", &old);
        time("native listing, 1 thread", &|| read_tree(&root, 1, &stop).unwrap().snapshot().files.len());
        let threads = platform::volume::scan_threads(&root);
        time(&format!("native listing, {threads} threads (drive)"), &|| {
            read_tree(&root, threads, &stop).unwrap().snapshot().files.len()
        });
        time("native listing, 8 threads", &|| read_tree(&root, 8, &stop).unwrap().snapshot().files.len());
        let tmp = tempfile::tempdir().unwrap();
        let cache = ScanCache::new(Some(tmp.path().to_path_buf()));
        time("kept tree: first scan (writes cache)", &|| {
            cache.scan(&root, Role::Source, None, &stop).unwrap().snapshot.files.len()
        });
        time("kept tree: next scan", &|| {
            let s = cache.scan(&root, Role::Source, None, &stop).unwrap();
            println!("  mode {:?}", s.mode);
            s.snapshot.files.len()
        });
    }

    #[test]
    fn a_batch_shares_its_scans() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "1.txt", "1");
        let cache = ScanCache::new(None);
        let stop = AtomicBool::new(false);
        assert_eq!(cache.scan(tmp.path(), Role::Source, Some(7), &stop).unwrap().mode, ScanMode::Full);
        assert_eq!(cache.scan(tmp.path(), Role::Source, Some(7), &stop).unwrap().mode, ScanMode::Shared);
        cache.end_batch(7);
        assert_eq!(cache.scan(tmp.path(), Role::Source, Some(7), &stop).unwrap().mode, ScanMode::Full);
    }
}
