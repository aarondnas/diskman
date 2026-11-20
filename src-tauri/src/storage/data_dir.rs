//! The data folder (DISKMAN_DATA_DIR, otherwise the system's place for app data, see
//! `platform::paths`) and the names of the files in it.
//! Also takes over the folder and database of earlier app names once.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::platform;

/// Backups, entries, schedules and settings (see `config_file`).
pub const CONFIG_FILE: &str = "config.json";
/// A config.json that could not be read is kept under this name instead of being overwritten.
pub const BROKEN_CONFIG_FILE: &str = "config.broken.json";
/// The config as it was before the last change; read when config.json is missing or unreadable.
pub const BACKUP_CONFIG_FILE: &str = "config.backup.json";
/// History of all runs (see `history`).
pub const DATABASE_FILE: &str = "diskman.db";
/// Kept folder trees of the scans (see `sync::tree`); deleting it only means full scans.
pub const SCAN_CACHE_DIR: &str = "scan-cache";
/// Status file of versions before the history; imported once (see `History::import_legacy_status`).
pub const LEGACY_STATUS_FILE: &str = "status.json";

/// Earlier names of the app, newest first: their data folder, variable and database are taken over.
const OLD_DIR_NAMES: [&str; 5] = ["SyncHappens", "FinalBackup", "PlanB", "Synchrom", "Synchro"];
const OLD_DATABASE_FILES: [&str; 4] = ["synchappens.db", "finalbackup.db", "planb.db", "synchrom.db"];
/// SYNCHAPPENS_/FINALBACKUP_/PLANB_/SYNCHROM_DATA_DIR: old names of the variable, still honored
const DIR_VARIABLES: [&str; 5] =
    ["DISKMAN_DATA_DIR", "SYNCHAPPENS_DATA_DIR", "FINALBACKUP_DATA_DIR", "PLANB_DATA_DIR", "SYNCHROM_DATA_DIR"];

/// Finds the data folder: from the environment variable, otherwise the system's default (taking
/// over the folder of an earlier app name next to it). Creates it if needed.
pub fn locate() -> anyhow::Result<PathBuf> {
    let dir = DIR_VARIABLES
        .iter()
        .find_map(std::env::var_os)
        .map(PathBuf::from)
        .or_else(|| {
            let dir = platform::paths::data_dir()?;
            let parent = dir.parent()?.to_path_buf();
            Some(migrate_old_dir(&OLD_DIR_NAMES.map(|n| parent.join(n)), dir))
        })
        .context("No folder for app data found (set DISKMAN_DATA_DIR)")?;
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Path of the history database; renames the database of an earlier app name first.
pub fn database_path(dir: &Path) -> PathBuf {
    let path = dir.join(DATABASE_FILE);
    migrate_old_db(&OLD_DATABASE_FILES.map(|n| dir.join(n)), &path);
    path
}

/// Takes over the data folder from when the app had another name.
/// `olds` in order of preference; the first existing one is taken over.
fn migrate_old_dir(olds: &[PathBuf], new: PathBuf) -> PathBuf {
    if !new.exists() {
        if let Some(old) = olds.iter().find(|o| o.is_dir()) {
            if let Err(e) = fs::rename(old, &new) {
                eprintln!("Could not take over old data folder {}: {e}", old.display());
            }
        }
    }
    new
}

/// Renames the history database from an earlier app name, including its WAL files.
/// `olds` in order of preference; the first existing one is taken over.
fn migrate_old_db(olds: &[PathBuf], new: &Path) {
    if new.exists() {
        return;
    }
    let Some(old) = olds.iter().find(|o| o.exists()) else { return };
    for suffix in ["", "-wal", "-shm"] {
        let from = PathBuf::from(format!("{}{suffix}", old.display()));
        if from.exists() {
            if let Err(e) = fs::rename(&from, format!("{}{suffix}", new.display())) {
                eprintln!("Could not take over {}: {e}", from.display());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_over_old_data_dir_once() {
        let root = tempfile::tempdir().unwrap();
        let oldest = root.path().join("Synchro");
        let old = root.path().join("Synchrom");
        let new = root.path().join("Diskman");
        let olds = [old.clone(), oldest.clone()];
        fs::create_dir(&oldest).unwrap();
        fs::write(oldest.join("config.json"), "{}").unwrap();

        // Only the oldest name exists → it is taken over
        assert_eq!(migrate_old_dir(&olds, new.clone()), new);
        assert!(new.join("config.json").exists());
        assert!(!oldest.exists());

        // If the new one already exists, old ones stay untouched
        fs::create_dir(&old).unwrap();
        migrate_old_dir(&olds, new.clone());
        assert!(old.exists() && new.join("config.json").exists());
    }

    #[test]
    fn prefers_newer_old_name() {
        let root = tempfile::tempdir().unwrap();
        let (oldest, old, new) = (root.path().join("Synchro"), root.path().join("PlanB"), root.path().join("Diskman"));
        fs::create_dir(&oldest).unwrap();
        fs::create_dir(&old).unwrap();
        fs::write(old.join("config.json"), "{}").unwrap();
        migrate_old_dir(&[old.clone(), oldest.clone()], new.clone());
        assert!(new.join("config.json").exists() && oldest.exists() && !old.exists());
    }

    #[test]
    fn renames_old_database_once() {
        let dir = tempfile::tempdir().unwrap();
        let (older, old, new) =
            (dir.path().join("synchrom.db"), dir.path().join("planb.db"), dir.path().join("diskman.db"));
        fs::write(&older, "older").unwrap();
        fs::write(&old, "db").unwrap();
        fs::write(dir.path().join("planb.db-wal"), "wal").unwrap();
        migrate_old_db(&[old.clone(), older.clone()], &new);
        // The newer old name wins, the older one stays
        assert_eq!(fs::read_to_string(&new).unwrap(), "db");
        assert!(dir.path().join("diskman.db-wal").exists() && !old.exists() && older.exists());
    }
}
