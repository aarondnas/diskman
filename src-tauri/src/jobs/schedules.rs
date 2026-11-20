//! Schedules of the backups: regularly checks which backups are due and queues them (`queue`).
//! There they run like a manual start, with the same safety rules, and appear in the history as
//! `schedule`. Backups due at the same time run one after another in the order of the config.
//!
//! Runs missed while Diskman was not running are not caught up by themselves. For schedules with
//! `catch_up` the scheduler logs until when it watched them; on the next start `catch_up` finds
//! the missed runs from that and the UI offers to run them.
//! If the computer was only asleep, a missed daily schedule runs once on wake-up.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{Datelike, Local, NaiveDateTime};

use super::{catch_up, queue};
use crate::model::config::parse_time;
use crate::model::Schedule;
use crate::state::AppState;
use crate::storage::history::ScheduleLog;

/// Check interval; a daily schedule therefore starts up to 15 s after its time.
const TICK: Duration = Duration::from_secs(15);

/// The log is written this often, and right away when a logged schedule runs or the set of logged
/// schedules changes. A due time between the last write and quitting is not reported as missed by
/// mistake: had it come while Diskman ran, the run would have been logged at once.
const LOG_EVERY: chrono::Duration = chrono::Duration::seconds(60);

/// One schedule as the scheduler sees it on a tick.
struct Watched {
    id: String,
    when: Schedule,
    enabled: bool,
    catch_up: bool,
}

pub fn spawn(state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(TICK);
        let mut prev = Local::now().naive_local();
        // Last run per backup with an interval; the first run is one interval after Diskman starts
        let mut last_fired: HashMap<String, NaiveDateTime> = HashMap::new();
        // Last write of the log and the schedules in it (None: not written yet since the start)
        let mut logged: Option<(NaiveDateTime, Vec<String>)> = None;
        loop {
            tick.tick().await;
            let now = Local::now().naive_local();
            let watched: Vec<Watched> = state
                .config
                .lock()
                .unwrap()
                .backups
                .iter()
                .filter_map(|b| {
                    b.schedule.as_ref().map(|s| Watched {
                        id: b.id.clone(),
                        when: s.when.clone(),
                        enabled: s.enabled,
                        catch_up: s.catch_up,
                    })
                })
                .collect();
            let mut fired_logged = false;
            for w in watched.iter().filter(|w| w.enabled) {
                let last = *last_fired.entry(w.id.clone()).or_insert(now);
                if is_due(&w.when, prev, now, last) {
                    last_fired.insert(w.id.clone(), now);
                    queue::enqueue(&state, &w.id, "schedule");
                    fired_logged |= w.catch_up;
                }
            }
            // Paused schedules are logged as well: a pause is no missed run
            let ids: Vec<String> = watched.iter().filter(|w| w.catch_up).map(|w| w.id.clone()).collect();
            let write = match &logged {
                None => true,
                Some((at, before)) => fired_logged || *before != ids || now - *at >= LOG_EVERY,
            };
            if write {
                let checked_at = catch_up::utc(now);
                let entries: Vec<(String, ScheduleLog)> = ids
                    .iter()
                    .map(|id| {
                        let counted_at = last_fired.get(id).map_or(checked_at, |t| catch_up::utc(*t));
                        (id.clone(), ScheduleLog { checked_at, counted_at })
                    })
                    .collect();
                if let Err(e) = state.history.write_schedule_log(&entries) {
                    eprintln!("Schedule log not written: {e:#}");
                }
                logged = Some((now, ids));
            }
            prev = now;
        }
    });
}

/// Is the schedule due between the previous check `prev` (exclusive) and `now` (inclusive)?
/// `last` is its last run (relevant for intervals).
pub fn is_due(schedule: &Schedule, prev: NaiveDateTime, now: NaiveDateTime, last: NaiveDateTime) -> bool {
    match schedule {
        Schedule::Interval { minutes } => now - last >= chrono::Duration::minutes(*minutes as i64),
        Schedule::Daily { time, days } => {
            let Some(time) = parse_time(time) else { return false };
            // Look at every day between both checks (usually just today; after sleep also earlier days,
            // at most one week back)
            let mut date = prev.date().max(now.date() - chrono::Duration::days(7));
            while date <= now.date() {
                let slot = date.and_time(time);
                if slot > prev && slot <= now && days.contains(&(date.weekday().number_from_monday() as u8)) {
                    return true;
                }
                date = date.succ_opt().expect("date within valid range");
            }
            false
        }
    }
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
    fn daily_fires_once_when_its_time_passes() {
        let s = daily("02:00", &[1, 2, 3, 4, 5, 6, 7]);
        let last = at("2026-11-01 00:00:00");
        assert!(!is_due(&s, at("2026-11-09 01:59:30"), at("2026-11-09 01:59:45"), last));
        assert!(is_due(&s, at("2026-11-09 01:59:45"), at("2026-11-09 02:00:00"), last));
        assert!(!is_due(&s, at("2026-11-09 02:00:00"), at("2026-11-09 02:00:15"), last));
    }

    #[test]
    fn daily_respects_weekdays() {
        let s = daily("08:30", &[2]); // Tuesdays only
        let last = at("2026-11-01 00:00:00");
        // Monday no, Tuesday yes
        assert!(!is_due(&s, at("2026-11-09 08:29:50"), at("2026-11-09 08:30:05"), last));
        assert!(is_due(&s, at("2026-11-10 08:29:50"), at("2026-11-10 08:30:05"), last));
    }

    #[test]
    fn daily_catches_up_after_sleep_across_midnight() {
        let s = daily("23:00", &[1]);
        // Computer slept from Monday 22:00 to Tuesday 07:00: Monday 23:00 is caught up
        assert!(is_due(&s, at("2026-11-09 22:00:00"), at("2026-11-10 07:00:00"), at("2026-11-01 00:00:00")));
    }

    #[test]
    fn interval_counts_from_last_run() {
        let s = Schedule::Interval { minutes: 60 };
        let last = at("2026-11-09 10:00:00");
        assert!(!is_due(&s, at("2026-11-09 10:59:30"), at("2026-11-09 10:59:45"), last));
        assert!(is_due(&s, at("2026-11-09 10:59:45"), at("2026-11-09 11:00:00"), last));
    }

    #[test]
    fn invalid_time_never_fires() {
        let s = daily("25:99", &[1, 2, 3, 4, 5, 6, 7]);
        assert!(!is_due(&s, at("2026-11-09 00:00:00"), at("2026-11-10 00:00:00"), at("2026-11-01 00:00:00")));
    }
}
