//! Catching up runs that were missed while Diskman was not running, for schedules with
//! `catch_up` switched on.
//!
//! While Diskman runs, the scheduler logs these schedules (`schedules::spawn`, table
//! `schedule_log` of the history): until when it was watching them, and from when their interval
//! counts. On the next start, before the scheduler begins, `find` compares that with the
//! schedule: every due time after the last check was missed, because a schedule that was due
//! while Diskman ran was run and logged. After a normal start the result waits in
//! `AppState::missed` until the UI asks (`commands::catch_up`) and the user runs the backups or
//! dismisses them. After a start at login there is nobody to ask: `run_all` queues them right away
//! and a system notification says so.

use chrono::{DateTime, Datelike, Local, NaiveDateTime, TimeZone, Utc};
use serde::Serialize;

use super::queue;
use crate::model::config::parse_time;
use crate::model::Schedule;
use crate::platform::locale;
use crate::state::AppState;

/// Daily schedules look at most this many days back (a longer pause is still reported).
const MAX_DAYS: i64 = 400;

/// A backup whose schedule was due while Diskman was not running.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MissedRun {
    pub backup_id: String,
    /// First missed due time: overdue since then
    pub due_since: DateTime<Utc>,
    /// How many due times were missed
    pub missed: u32,
    /// Start of the latest run of the backup (by hand or scheduled), if it has one
    pub last_run: Option<DateTime<Utc>>,
}

/// The backups that missed runs, in the order of the config. Only enabled schedules with
/// `catch_up` and at least one enabled archive count, and only if they were logged before.
pub fn find(state: &AppState) -> Vec<MissedRun> {
    let log = match state.history.schedule_log() {
        Ok(log) => log,
        Err(e) => {
            eprintln!("Schedule log not readable, missed runs are not checked: {e:#}");
            return Vec::new();
        }
    };
    let now = Local::now().naive_local();
    let config = state.config.lock().unwrap().clone();
    config
        .backups
        .iter()
        .filter_map(|b| {
            let schedule = b.schedule.as_ref().filter(|s| s.enabled && s.catch_up)?;
            if !b.archives.iter().any(|a| a.enabled) {
                return None;
            }
            let entry = log.get(&b.id)?;
            let (first, count) = missed(&schedule.when, local(entry.checked_at), local(entry.counted_at), now)?;
            Some(MissedRun {
                backup_id: b.id.clone(),
                due_since: utc(first),
                missed: count,
                last_run: state.history.last_run_at(&b.id).unwrap_or_else(|e| {
                    eprintln!("Last run of {} not readable: {e:#}", b.id);
                    None
                }),
            })
        })
        .collect()
}

/// Queues all these backups (origin `catch_up`) and returns title and text of the notification
/// that says so, in the language of the app; `None` if nothing was missed.
pub fn run_all(state: &AppState, missed: &[MissedRun]) -> Option<(String, String)> {
    let (names, language) = {
        let config = state.config.lock().unwrap();
        let names: Vec<String> = missed
            .iter()
            .filter_map(|m| config.backups.iter().find(|b| b.id == m.backup_id))
            .map(|b| b.name.clone())
            .collect();
        (names, config.settings.language.clone().or_else(locale::system_language))
    };
    if names.is_empty() {
        return None;
    }
    for m in missed {
        queue::enqueue(state, &m.backup_id, "catch_up");
    }
    Some(notice(language.as_deref().unwrap_or("en"), &names))
}

/// Title and text of the notification about catching up these backups.
fn notice(language: &str, names: &[String]) -> (String, String) {
    // (title, one backup: {name}, several: {count} and {names})
    let (title, one, many) = match language {
        "de" => (
            "Diskman holt verpasste Backups nach",
            "„{name}“ war fällig, während Diskman nicht lief, und läuft jetzt.",
            "{count} Backups waren fällig, während Diskman nicht lief, und laufen jetzt nacheinander: {names}",
        ),
        "fr" => (
            "Diskman rattrape des sauvegardes manquées",
            "« {name} » était due pendant que Diskman ne tournait pas et s'exécute maintenant.",
            "{count} sauvegardes étaient dues pendant que Diskman ne tournait pas et s'exécutent maintenant l'une après l'autre : {names}",
        ),
        "es" => (
            "Diskman recupera copias perdidas",
            "«{name}» tocaba mientras Diskman no estaba en marcha y se ejecuta ahora.",
            "{count} copias tocaban mientras Diskman no estaba en marcha y se ejecutan ahora una tras otra: {names}",
        ),
        "it" => (
            "Diskman recupera i backup persi",
            "«{name}» era previsto mentre Diskman non era in esecuzione e viene eseguito ora.",
            "{count} backup erano previsti mentre Diskman non era in esecuzione e vengono eseguiti ora uno dopo l'altro: {names}",
        ),
        "nl" => (
            "Diskman haalt gemiste back-ups in",
            "‘{name}’ was aan de beurt toen Diskman niet draaide en draait nu.",
            "{count} back-ups waren aan de beurt toen Diskman niet draaide en draaien nu na elkaar: {names}",
        ),
        "pt" => (
            "O Diskman está a recuperar backups perdidos",
            "«{name}» estava previsto enquanto o Diskman não estava em execução e é executado agora.",
            "{count} backups estavam previstos enquanto o Diskman não estava em execução e são executados agora um após o outro: {names}",
        ),
        _ => (
            "Diskman is catching up missed backups",
            "\"{name}\" was due while Diskman was not running and runs now.",
            "{count} backups were due while Diskman was not running and now run one after another: {names}",
        ),
    };
    let body = match names {
        [name] => one.replace("{name}", name),
        _ => many.replace("{count}", &names.len().to_string()).replace("{names}", &names.join(", ")),
    };
    (title.to_string(), body)
}

/// Due times of a schedule after `checked` (until then Diskman was watching) up to `now`: the
/// first one and how many. `counted` is where its interval starts. All in local time.
pub fn missed(
    schedule: &Schedule,
    checked: NaiveDateTime,
    counted: NaiveDateTime,
    now: NaiveDateTime,
) -> Option<(NaiveDateTime, u32)> {
    if now <= checked {
        return None;
    }
    match schedule {
        Schedule::Daily { time, days } => {
            let time = parse_time(time)?;
            let mut first = None;
            let mut count = 0;
            let mut date = checked.date().max(now.date() - chrono::Duration::days(MAX_DAYS));
            while date <= now.date() {
                let slot = date.and_time(time);
                if slot > checked && slot <= now && days.contains(&(date.weekday().number_from_monday() as u8)) {
                    first.get_or_insert(slot);
                    count += 1;
                }
                date = date.succ_opt()?;
            }
            first.map(|f| (f, count))
        }
        Schedule::Interval { minutes } => {
            let step = chrono::Duration::minutes(i64::from(*minutes).max(1));
            // First due time after the last check (earlier ones were run while Diskman watched)
            let passed = ((checked - counted).num_seconds() / step.num_seconds()).max(0);
            let first = counted + step * (passed as i32 + 1);
            if first > now {
                return None;
            }
            let count = (now - first).num_seconds() / step.num_seconds() + 1;
            Some((first, u32::try_from(count).unwrap_or(u32::MAX)))
        }
    }
}

/// Local time of a logged moment.
pub fn local(t: DateTime<Utc>) -> NaiveDateTime {
    t.with_timezone(&Local).naive_local()
}

/// A local time as a moment; on a clock change the earlier one.
pub fn utc(t: NaiveDateTime) -> DateTime<Utc> {
    Local.from_local_datetime(&t).earliest().map_or_else(|| t.and_utc(), |d| d.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    fn daily(time: &str, days: &[u8]) -> Schedule {
        Schedule::Daily { time: time.into(), days: days.to_vec() }
    }

    // 2026-11-09 is a Monday
    #[test]
    fn daily_counts_the_due_times_while_diskman_was_off() {
        let s = daily("02:00", &[1, 2, 3, 4, 5, 6, 7]);
        // Last watched Monday 01:00, started again Wednesday 09:00: Mon, Tue, Wed 02:00 missed
        let r = missed(&s, at("2026-11-09 01:00:00"), at("2026-11-09 00:00:00"), at("2026-11-11 09:00:00"));
        assert_eq!(r, Some((at("2026-11-09 02:00:00"), 3)));
    }

    #[test]
    fn daily_due_time_before_the_last_check_was_run() {
        let s = daily("02:00", &[1, 2, 3, 4, 5, 6, 7]);
        // Diskman ran past 02:00 and was started again the same day: nothing missed
        assert_eq!(missed(&s, at("2026-11-09 02:00:10"), at("2026-11-09 00:00:00"), at("2026-11-09 18:00:00")), None);
    }

    #[test]
    fn daily_respects_weekdays() {
        let s = daily("08:30", &[6, 7]); // weekends only
                                         // Off from Monday to Friday evening: nothing; until Sunday noon: Sat and Sun
        let checked = at("2026-11-09 12:00:00");
        assert_eq!(missed(&s, checked, checked, at("2026-11-13 20:00:00")), None);
        assert_eq!(missed(&s, checked, checked, at("2026-11-15 12:00:00")), Some((at("2026-11-14 08:30:00"), 2)));
    }

    #[test]
    fn interval_counts_from_its_start() {
        let s = Schedule::Interval { minutes: 60 };
        // Counting from 10:00, watched until 10:30, started again 13:15: 11:00, 12:00, 13:00 missed
        let r = missed(&s, at("2026-11-09 10:30:00"), at("2026-11-09 10:00:00"), at("2026-11-09 13:15:00"));
        assert_eq!(r, Some((at("2026-11-09 11:00:00"), 3)));
        // Started again before the next due time: nothing
        assert_eq!(missed(&s, at("2026-11-09 10:30:00"), at("2026-11-09 10:00:00"), at("2026-11-09 10:59:00")), None);
    }

    #[test]
    fn interval_ignores_due_times_before_the_last_check() {
        let s = Schedule::Interval { minutes: 60 };
        // Logged start lies long before the last check: only due times after it count
        let r = missed(&s, at("2026-11-09 15:20:00"), at("2026-11-09 10:00:00"), at("2026-11-09 16:30:00"));
        assert_eq!(r, Some((at("2026-11-09 16:00:00"), 1)));
    }

    #[test]
    fn clock_set_back_misses_nothing() {
        let s = daily("02:00", &[1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(missed(&s, at("2026-11-10 09:00:00"), at("2026-11-10 09:00:00"), at("2026-11-09 09:00:00")), None);
    }

    #[test]
    fn only_logged_enabled_schedules_that_catch_up_are_reported() {
        use crate::model::Config;
        use crate::storage::history::ScheduleLog;

        let dir = tempfile::tempdir().unwrap();
        let state = AppState::load_from(dir.path().to_path_buf()).unwrap();
        let backup = |id: &str, enabled: bool, catch_up: bool| {
            serde_json::json!({
                "id": id, "name": id,
                "archives": [{ "id": format!("{id}-a"), "source": format!("C:/{id}"), "path": format!("D:/{id}"), "mode": "mirror" }],
                "schedule": { "when": { "kind": "interval", "minutes": 60 }, "enabled": enabled, "catch_up": catch_up }
            })
        };
        let config: Config = serde_json::from_value(serde_json::json!({ "backups": [
            backup("due", true, true),
            backup("plain", true, false),
            backup("paused", false, true),
            backup("unlogged", true, true),
        ]}))
        .unwrap();
        state.save_config(config).unwrap();

        // Diskman last ran three hours ago
        let then = Utc::now() - chrono::Duration::hours(3);
        let log = ScheduleLog { checked_at: then, counted_at: then };
        let entries: Vec<_> = ["due", "plain", "paused"].iter().map(|id| (id.to_string(), log)).collect();
        state.history.write_schedule_log(&entries).unwrap();

        let found = find(&state);
        assert_eq!(found.iter().map(|m| m.backup_id.as_str()).collect::<Vec<_>>(), ["due"]);
        assert_eq!(found[0].missed, 3);
        assert_eq!(found[0].last_run, None);
        let late = (Utc::now() - found[0].due_since).num_minutes();
        assert!((119..=121).contains(&late), "overdue for {late} min");
    }

    #[test]
    fn notice_names_the_backups_in_the_language() {
        let (title, body) = notice("de", &["Fotos".into()]);
        assert_eq!(title, "Diskman holt verpasste Backups nach");
        assert!(body.starts_with("„Fotos“ war fällig"));
        let (_, body) = notice("xx", &["A".into(), "B".into()]);
        assert_eq!(body, "2 backups were due while Diskman was not running and now run one after another: A, B");
    }
}
