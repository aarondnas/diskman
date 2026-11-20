//! config.json in the data folder: read on start, written atomically on every change. The version
//! before the last change is kept as config.backup.json.

use std::fs;
use std::io::Write;
use std::path::Path;

use serde::Serialize;

use super::data_dir::{BACKUP_CONFIG_FILE, BROKEN_CONFIG_FILE, CONFIG_FILE};
use crate::model::Config;
use crate::platform;

/// Reads the config. Without a file there is an empty one; a file that cannot be read is kept as
/// config.broken.json instead of refusing to start (and instead of being overwritten later).
/// Either way the backup of the previous version is taken if there is a readable one.
pub fn load(dir: &Path) -> Config {
    let path = dir.join(CONFIG_FILE);
    if path.exists() {
        match read(&path) {
            Ok(config) => return config,
            Err(e) => {
                eprintln!("{CONFIG_FILE} is invalid ({e:#}), keeping it as {BROKEN_CONFIG_FILE}");
                let _ = fs::rename(&path, dir.join(BROKEN_CONFIG_FILE));
            }
        }
    }
    let backup = dir.join(BACKUP_CONFIG_FILE);
    if backup.exists() {
        match read(&backup) {
            Ok(config) => {
                eprintln!("{CONFIG_FILE} restored from {BACKUP_CONFIG_FILE}");
                return config;
            }
            Err(e) => eprintln!("{BACKUP_CONFIG_FILE} is invalid as well ({e:#})"),
        }
    }
    Config::default()
}

fn read(path: &Path) -> anyhow::Result<Config> {
    Ok(Config::from_json(&fs::read_to_string(path)?)?)
}

/// Writes the config; neither a crash nor a power cut in between leaves a broken file.
pub fn save(dir: &Path, config: &Config) -> anyhow::Result<()> {
    let path = dir.join(CONFIG_FILE);
    // The current version becomes the backup (a copy: config.json stays in place meanwhile)
    if path.exists() {
        if let Err(e) = fs::copy(&path, dir.join(BACKUP_CONFIG_FILE)) {
            eprintln!("{BACKUP_CONFIG_FILE} not written: {e}");
        }
    }
    save_json(&path, config)
}

/// Writes JSON atomically: temporary file, flushed to the drive, then renamed over the file.
fn save_json<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    let tmp = path.with_extension("json.tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&serde_json::to_vec_pretty(value)?)?;
        file.sync_all()?;
    }
    fs::rename(tmp, path)?;
    if let Some(dir) = path.parent() {
        platform::fs::sync_dir(dir);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_config_loads_again() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load(dir.path()).backups.is_empty());
        let config = Config::from_json(r#"{"backups": [{"id": "b1", "name": "Docs"}]}"#).unwrap();
        save(dir.path(), &config).unwrap();
        assert_eq!(load(dir.path()).backups[0].name, "Docs");
    }

    #[test]
    fn the_previous_version_steps_in_for_a_broken_file() {
        let dir = tempfile::tempdir().unwrap();
        let first = Config::from_json(r#"{"backups": [{"id": "b1", "name": "Docs"}]}"#).unwrap();
        let second = Config::from_json(r#"{"backups": [{"id": "b1", "name": "Photos"}]}"#).unwrap();
        save(dir.path(), &first).unwrap();
        save(dir.path(), &second).unwrap();
        assert_eq!(load(dir.path()).backups[0].name, "Photos");
        fs::write(dir.path().join(CONFIG_FILE), "").unwrap();
        assert_eq!(load(dir.path()).backups[0].name, "Docs");
        assert!(dir.path().join(BROKEN_CONFIG_FILE).exists());
    }

    #[test]
    fn broken_file_is_kept_aside() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(CONFIG_FILE), "{ not json").unwrap();
        assert!(load(dir.path()).backups.is_empty());
        assert!(dir.path().join(BROKEN_CONFIG_FILE).exists());
        assert!(!dir.path().join(CONFIG_FILE).exists());
    }
}
