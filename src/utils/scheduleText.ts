// Readable texts for the schedule of a backup (backup.schedule = { when, enabled }), used on the
// backup cards and in the schedule editor. The backend runs the schedules (src-tauri/src/jobs/schedules.rs).
import type { BackupSchedule, Schedule } from '../api/types.ts';
import { t } from '../i18n/index.svelte.ts';
import { weekdayLong, weekdayShort } from './format.ts';

/** Timing as text, e.g. "Mon–Fri at 02:00" or "Every 2 h". */
export function describe(when: Schedule): string {
  if (when.kind === 'interval') {
    return when.minutes % 60 === 0
      ? t('scheduler.everyHours', { count: when.minutes / 60 })
      : t('scheduler.everyMinutes', { count: when.minutes });
  }
  const d = [...when.days].sort((a, b) => a - b);
  const key = d.join(',');
  const days =
    key === '1,2,3,4,5,6,7'
      ? t('scheduler.daily')
      : key === '1,2,3,4,5'
        ? `${weekdayShort(1)}–${weekdayShort(5)}`
        : key === '6,7'
          ? `${weekdayShort(6)} + ${weekdayShort(7)}`
          : d.map((day) => weekdayShort(day)).join(', ');
  return t('scheduler.atTime', { days, time: when.time });
}

/** Next run of a daily timing, e.g. "tomorrow, 02:00"; null for intervals (they count from the start of Diskman). */
export function nextRun(when: Schedule, now = new Date()): string | null {
  if (when.kind !== 'daily') return null;
  const [h, m] = when.time.split(':').map(Number);
  for (let i = 0; i < 8; i++) {
    const date = new Date(now.getFullYear(), now.getMonth(), now.getDate() + i, h, m);
    const weekday = ((date.getDay() + 6) % 7) + 1; // 1 = Monday
    if (date > now && when.days.includes(weekday)) {
      const day = i === 0 ? t('scheduler.today') : i === 1 ? t('scheduler.tomorrow') : weekdayLong(date);
      return t('scheduler.dayTime', { day, time: when.time });
    }
  }
  return null;
}

/** Second line under the timing: next run, "counted from start" or "paused". */
export function stateText(schedule: Pick<BackupSchedule, 'when' | 'enabled'>): string {
  if (!schedule.enabled) return t('scheduler.paused');
  const next = nextRun(schedule.when);
  return next ? t('scheduler.nextRun', { when: next }) : t('scheduler.countedFromStart');
}
