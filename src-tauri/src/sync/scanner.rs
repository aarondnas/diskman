//! Captures the state of a directory tree (paths, sizes, timestamps).

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::issues::Issue;

/// Suffix of the temporary files created while copying. They are never scanned.
pub const TMP_SUFFIX: &str = ".diskman-tmp";
/// Folder at the top of an archive where a run puts what it deletes, emptied when the run ends
/// (see `sync::executor`). Never scanned, in sources neither.
pub const TRASH_DIR: &str = ".diskman-trash";
/// Suffixes from earlier app names; leftovers are skipped as well.
const OLD_TMP_SUFFIXES: [&str; 4] = [".synchappens-tmp", ".finalbackup-tmp", ".planb-tmp", ".synchro-tmp"];

#[derive(Debug, Clone)]
pub struct FileMeta {
    /// Relative path with `/` as separator.
    pub rel: String,
    pub size: u64,
    /// Modification time in ms since the Unix epoch.
    pub mtime: i64,
    /// Creation time in ms (not available on all file systems).
    pub ctime: Option<i64>,
    /// Encrypted by the file system (Windows EFS); copying needs a file system that supports it.
    pub encrypted: bool,
}

#[derive(Debug, Default)]
pub struct Snapshot {
    pub files: Vec<FileMeta>,
    pub dirs: Vec<String>,
    /// Entries that could not be read, with the reason ("" = root).
    pub errors: Vec<Issue>,
    /// Skipped on purpose: symbolic links and junctions (neither followed nor copied) and special
    /// files such as named pipes, sockets and devices (Linux).
    pub skipped: Vec<Issue>,
    /// Temporary files of copies that never finished (Diskman was closed or crashed during them);
    /// not part of the tree. The executor deletes those in an archive.
    pub leftovers: Vec<String>,
}

pub fn join(root: &Path, rel: &str) -> PathBuf {
    let mut p = root.to_path_buf();
    p.extend(rel.split('/'));
    p
}

/// A temporary file of an unfinished copy (ours or from an earlier app name).
pub fn is_leftover(name: &str) -> bool {
    name.ends_with(TMP_SUFFIX) || OLD_TMP_SUFFIXES.iter().any(|s| name.ends_with(s))
}

/// Reads the whole tree below `root` in one thread, without keeping it (see `sync::tree` for the
/// kept, incremental and parallel scans). Stops with `Cancelled` as soon as `stop` is set.
#[cfg(test)]
pub fn scan(root: &Path, stop: &std::sync::atomic::AtomicBool) -> anyhow::Result<Snapshot> {
    Ok(super::tree::read_tree(root, 1, stop)?.snapshot())
}

/// Fast content fingerprint: start, middle and end (64 KiB each) plus size.
/// Small files are hashed completely. Used to confirm detected moves.
pub fn sample_hash(path: &Path, size: u64) -> io::Result<u64> {
    const CHUNK: u64 = 64 * 1024;
    let mut file = File::open(path)?;
    let mut hasher = xxhash_rust::xxh3::Xxh3::new();
    if size <= 3 * CHUNK {
        let mut all = Vec::with_capacity(size as usize);
        file.read_to_end(&mut all)?;
        hasher.update(&all);
    } else {
        let mut buf = vec![0u8; CHUNK as usize];
        for offset in [0, size / 2 - CHUNK / 2, size - CHUNK] {
            file.seek(SeekFrom::Start(offset))?;
            file.read_exact(&mut buf)?;
            hasher.update(&buf);
        }
    }
    hasher.update(&size.to_le_bytes());
    Ok(hasher.digest())
}

/// Content fingerprint of the whole file, to verify a copy (`Settings::verify_copies`).
pub fn full_hash(path: &Path) -> io::Result<u64> {
    let mut file = File::open(path)?;
    let mut hasher = xxhash_rust::xxh3::Xxh3::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        match file.read(&mut buf)? {
            0 => return Ok(hasher.digest()),
            n => hasher.update(&buf[..n]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn a_set_stop_ends_the_scan_as_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        assert_eq!(scan(dir.path(), &AtomicBool::new(false)).unwrap().files.len(), 1);
        let err = scan(dir.path(), &AtomicBool::new(true)).unwrap_err();
        assert!(crate::sync::is_cancelled(&err));
    }

    #[test]
    fn leftovers_and_the_trash_are_not_part_of_the_tree() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        std::fs::write(dir.path().join(format!(".b.txt{TMP_SUFFIX}")), "half").unwrap();
        std::fs::create_dir_all(dir.path().join(TRASH_DIR).join("old")).unwrap();
        std::fs::write(dir.path().join(TRASH_DIR).join("old/c.txt"), "c").unwrap();
        let snap = scan(dir.path(), &AtomicBool::new(false)).unwrap();
        assert_eq!(snap.files.iter().map(|f| f.rel.as_str()).collect::<Vec<_>>(), ["a.txt"]);
        assert!(snap.dirs.is_empty());
        assert_eq!(snap.leftovers, [format!(".b.txt{TMP_SUFFIX}")]);
    }
}
