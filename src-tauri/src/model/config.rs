//! The user's configuration as data: backups (named lists of source → archive entries), each with
//! an optional schedule and colour, plus the app settings. Also the rules for it (migration of
//! older configs, validation). Reading and writing config.json: `storage::config_file`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How an archive is synced with its backup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// The archive becomes an exact copy of the source (including deletions and moves).
    Mirror,
    /// New and changed files are copied, nothing is ever removed from the archive.
    Additive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Archive {
    pub id: String,
    /// Folder that is backed up into this archive.
    #[serde(default)]
    pub source: String,
    pub path: String,
    pub mode: Mode,
    #[serde(default = "enabled_default")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backup {
    pub id: String,
    pub name: String,
    /// Entries, each a source folder with its archive.
    #[serde(default)]
    pub archives: Vec<Archive>,
    /// When the backup runs by itself; None = only when started by hand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<BackupSchedule>,
    /// Colour of the status lights of the Diskman rack that marks the backup ("#rrggbb");
    /// None = the amber of the logo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Source of older configs where all archives of a backup shared one folder.
    /// Moved into each archive on load and never written again.
    #[serde(default, rename = "path", skip_serializing)]
    pub legacy_path: String,
}

impl Backup {
    /// The different source folders of the given archives, for history and messages.
    pub fn sources_of<'a>(archives: impl IntoIterator<Item = &'a str>) -> String {
        let mut list: Vec<&str> = Vec::new();
        for s in archives {
            if !list.contains(&s) {
                list.push(s);
            }
        }
        list.join(", ")
    }

    pub fn sources(&self) -> String {
        Self::sources_of(self.archives.iter().filter(|a| a.enabled).map(|a| a.source.as_str()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Tolerance when comparing modification times. 2 s covers FAT32/exFAT destination drives.
    #[serde(default = "tolerance_default")]
    pub mtime_tolerance_ms: i64,
    /// UI language code ("de", "en", …); None = follow the system language.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// The user answered whether Diskman starts at login (`commands::startup`); asked only once.
    /// Whether it does is not stored here but read from the system (`platform::autostart`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub autostart_asked: bool,
    /// Light theme chosen in the settings; Diskman is dark by default, whatever the system's mode.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub light_mode: bool,
    /// Read every copy back and compare it with its source before it replaces anything
    /// (`sync::executor`); slower, off by default.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub verify_copies: bool,
    /// How much the file lists of the activities may take in the history, in MB
    /// (`storage::plan_store`); the oldest go first.
    #[serde(default = "activity_limit_default")]
    pub activity_limit_mb: u64,
    /// Look for a new version of Diskman once a day (`updates`); on by default.
    #[serde(default = "yes")]
    pub check_updates: bool,
}

fn yes() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mtime_tolerance_ms: tolerance_default(),
            language: None,
            autostart_asked: false,
            light_mode: false,
            verify_copies: false,
            activity_limit_mb: activity_limit_default(),
            check_updates: true,
        }
    }
}

/// When a backup runs. Times are in the local time zone of the computer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Schedule {
    /// On the chosen weekdays (1 = Monday … 7 = Sunday) at `time` ("HH:MM").
    Daily { time: String, days: Vec<u8> },
    /// Every `minutes` minutes, counted from the start of Diskman or the last run.
    Interval { minutes: u32 },
}

/// Schedule of one backup; paused schedules keep their timing but do not run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupSchedule {
    pub when: Schedule,
    #[serde(default = "enabled_default")]
    pub enabled: bool,
    /// Log the schedule (`storage::history`, `schedule_log`), so that runs missed while Diskman
    /// was not running are found on the next start and offered to run (`jobs::catch_up`).
    #[serde(default)]
    pub catch_up: bool,
}

/// Schedule of older configs: an ordered list of backups with one timing. Only read, so that
/// `migrate` can hand its timing to each of its backups.
#[derive(Debug, Clone, Deserialize)]
pub struct LegacyPipeline {
    pub backup_ids: Vec<String>,
    pub schedule: Schedule,
    #[serde(default = "enabled_default")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, alias = "targets")] // old name from configs before the rename
    pub backups: Vec<Backup>,
    /// Schedules of older configs, moved into the backups on load and never written again.
    #[serde(default, rename = "pipelines", skip_serializing)]
    pub legacy_pipelines: Vec<LegacyPipeline>,
    #[serde(default)]
    pub settings: Settings,
}

/// Time of day of a daily schedule, `None` if the format is invalid.
pub fn parse_time(time: &str) -> Option<chrono::NaiveTime> {
    chrono::NaiveTime::parse_from_str(time.trim(), "%H:%M").ok()
}

fn enabled_default() -> bool {
    true
}

fn tolerance_default() -> i64 {
    2000
}

/// 1 GB for the file lists of the activities
pub const ACTIVITY_LIMIT_DEFAULT_MB: u64 = 1024;

fn activity_limit_default() -> u64 {
    ACTIVITY_LIMIT_DEFAULT_MB
}

impl Config {
    /// Reads a config from its JSON text, brought up to date with `migrate`.
    pub fn from_json(text: &str) -> serde_json::Result<Self> {
        let mut config: Self = serde_json::from_str(text)?;
        config.migrate();
        Ok(config)
    }

    /// Brings older configs up to date:
    /// - one source per backup: it becomes the source of each of its archives;
    /// - schedules with several backups: each backup gets the timing of the first schedule it
    ///   was in (the order between the backups is not kept).
    pub fn migrate(&mut self) {
        for b in &mut self.backups {
            let legacy = std::mem::take(&mut b.legacy_path);
            for a in &mut b.archives {
                if a.source.trim().is_empty() {
                    a.source = legacy.clone();
                }
            }
        }
        for p in std::mem::take(&mut self.legacy_pipelines) {
            for id in &p.backup_ids {
                if let Some(b) = self.backups.iter_mut().find(|b| &b.id == id) {
                    b.schedule.get_or_insert_with(|| BackupSchedule {
                        when: p.schedule.clone(),
                        enabled: p.enabled,
                        catch_up: false,
                    });
                }
            }
        }
    }

    /// Validates the config before it is applied. Error texts go straight to the UI.
    pub fn validate(&self) -> Result<(), String> {
        if !(0..=60_000).contains(&self.settings.mtime_tolerance_ms) {
            return Err("The time tolerance must be between 0 and 60000 ms.".into());
        }
        if !(64..=100_000).contains(&self.settings.activity_limit_mb) {
            return Err("The storage for activities must be between 64 MB and 100 GB.".into());
        }
        for t in &self.backups {
            if t.name.trim().is_empty() {
                return Err("Every backup needs a name.".into());
            }
            let mut seen = Vec::new();
            for a in &t.archives {
                check_absolute(&a.source, &format!("Backup path of \"{}\"", t.name))?;
                check_absolute(&a.path, &format!("Archive of \"{}\"", t.name))?;
                let archive = normalize(&a.path);
                if is_nested(&normalize(&a.source), &archive) {
                    return Err(format!(
                        "Archive \"{}\" overlaps with the backup path \"{}\". An archive may neither be inside the backup nor contain it.",
                        a.path, a.source
                    ));
                }
                if seen.contains(&archive) {
                    return Err(format!("Archive \"{}\" is listed twice for \"{}\".", a.path, t.name));
                }
                seen.push(archive);
            }
            if let Some(s) = &t.schedule {
                validate_schedule(&s.when, &t.name)?;
            }
            if let Some(c) = &t.color {
                let hex = c.strip_prefix('#').unwrap_or("");
                if hex.len() != 6 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
                    return Err(format!("Colour of \"{}\": \"{c}\" is not a colour (#rrggbb).", t.name));
                }
            }
        }
        Ok(())
    }
}

fn validate_schedule(schedule: &Schedule, backup: &str) -> Result<(), String> {
    match schedule {
        Schedule::Daily { time, days } => {
            if parse_time(time).is_none() {
                return Err(format!("Schedule of \"{backup}\": \"{time}\" is not a valid time (HH:MM)."));
            }
            if days.is_empty() || days.iter().any(|d| !(1..=7).contains(d)) {
                return Err(format!("Schedule of \"{backup}\": please choose at least one weekday."));
            }
        }
        Schedule::Interval { minutes } => {
            if !(1..=7 * 24 * 60).contains(minutes) {
                return Err(format!("Schedule of \"{backup}\": the interval must be between 1 minute and 7 days."));
            }
        }
    }
    Ok(())
}

fn check_absolute(path: &str, what: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err(format!("{what}: path is missing."));
    }
    if !Path::new(path.trim()).is_absolute() {
        return Err(format!("{what}: \"{path}\" is not an absolute path."));
    }
    Ok(())
}

/// Comparable form of a path: resolved, with `/`, without trailing separator,
/// lowercase on Windows (the file system ignores case).
fn normalize(path: &str) -> String {
    let p = PathBuf::from(path.trim());
    let p = fs::canonicalize(&p).unwrap_or(p);
    let mut s = p.to_string_lossy().replace('\\', "/");
    if let Some(stripped) = s.strip_prefix("//?/") {
        s = stripped.to_string();
    }
    let s = s.trim_end_matches('/').to_string();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

fn is_nested(a: &str, b: &str) -> bool {
    let under = |x: &str, y: &str| x == y || (x.starts_with(y) && x.as_bytes().get(y.len()) == Some(&b'/'));
    under(a, b) || under(b, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_config_moves_backup_path_into_each_archive() {
        let config = Config::from_json(
            r#"{"backups": [{"id": "b1", "name": "Docs", "path": "C:/docs", "archives": [
                {"id": "a1", "path": "D:/docs", "mode": "mirror"},
                {"id": "a2", "path": "E:/docs", "mode": "additive", "enabled": false}]}]}"#,
        )
        .unwrap();
        let b = &config.backups[0];
        assert!(b.archives.iter().all(|a| a.source == "C:/docs"));
        let saved = serde_json::to_value(&config).unwrap();
        assert!(saved["backups"][0].get("path").is_none());
        assert_eq!(saved["backups"][0]["archives"][1]["source"], "C:/docs");
    }

    #[test]
    fn colour_is_optional_and_must_be_hex() {
        let with = |color: Option<&str>| Config {
            backups: vec![Backup {
                id: "b".into(),
                name: "B".into(),
                archives: vec![],
                schedule: None,
                color: color.map(String::from),
                legacy_path: String::new(),
            }],
            ..Default::default()
        };
        assert!(with(None).validate().is_ok());
        assert!(with(Some("#4b9be6")).validate().is_ok());
        for bad in ["4b9be6", "#4b9be", "#4b9bez", "blue", ""] {
            assert!(with(Some(bad)).validate().is_err(), "{bad} should be refused");
        }
        // Without a colour nothing is written, so the default can change later
        let saved = serde_json::to_value(with(None)).unwrap();
        assert!(saved["backups"][0].get("color").is_none());
    }

    #[test]
    fn old_pipelines_become_schedules_of_their_backups() {
        let config = Config::from_json(
            r#"{"backups": [{"id": "b1", "name": "A"}, {"id": "b2", "name": "B"}, {"id": "b3", "name": "C"}],
                "pipelines": [
                  {"id": "p1", "name": "Night", "backup_ids": ["b1", "b2"],
                   "schedule": {"kind": "daily", "time": "02:00", "days": [1, 2]}, "enabled": false},
                  {"id": "p2", "name": "Hourly", "backup_ids": ["b2", "gone"],
                   "schedule": {"kind": "interval", "minutes": 60}}]}"#,
        )
        .unwrap();
        let night = BackupSchedule {
            when: Schedule::Daily { time: "02:00".into(), days: vec![1, 2] },
            enabled: false,
            catch_up: false,
        };
        assert_eq!(config.backups[0].schedule.as_ref(), Some(&night));
        // b2 was in both: the first schedule wins
        assert_eq!(config.backups[1].schedule.as_ref(), Some(&night));
        assert_eq!(config.backups[2].schedule, None);
        let saved = serde_json::to_value(&config).unwrap();
        assert!(saved.get("pipelines").is_none());
        assert_eq!(saved["backups"][0]["schedule"]["when"]["kind"], "daily");
        assert!(saved["backups"][2].get("schedule").is_none());
    }
}
