import { i18n, t } from '../i18n/index.svelte.ts';

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'];

export function formatBytes(n: number): string {
  if (!n) return '0 B';
  const i = Math.min(Math.floor(Math.log(n) / Math.log(1024)), UNITS.length - 1);
  return `${(n / 1024 ** i).toLocaleString(i18n.locale, { maximumFractionDigits: i ? 1 : 0 })} ${UNITS[i]}`;
}

export function formatCount(n: number): string {
  return n.toLocaleString(i18n.locale);
}

/** A moment (ISO 8601) relative to now: "just now", "5 min. ago", otherwise date and time. */
export function relativeTime(iso: string): string {
  const diff = (Date.now() - new Date(iso).getTime()) / 1000;
  if (diff < 60) return t('time.justNow');
  if (diff < 86400) {
    const rtf = new Intl.RelativeTimeFormat(i18n.locale, { numeric: 'auto', style: 'short' });
    return diff < 3600 ? rtf.format(-Math.floor(diff / 60), 'minute') : rtf.format(-Math.floor(diff / 3600), 'hour');
  }
  return new Date(iso).toLocaleString(i18n.locale, { dateStyle: 'short', timeStyle: 'short' });
}

/** Short weekday name, 1 = Monday … 7 = Sunday. */
export function weekdayShort(day: number): string {
  // 1 January 2024 was a Monday
  return new Date(2024, 0, day).toLocaleDateString(i18n.locale, { weekday: 'short' });
}

export function weekdayLong(date: Date): string {
  return date.toLocaleDateString(i18n.locale, { weekday: 'long' });
}

/**
 * A length of time in words, at most two units, e.g. "2 days, 3 hours" or "45 minutes"
 * (units and list from Intl, so every language gets its own words).
 */
export function formatDuration(ms: number): string {
  const minutes = Math.max(1, Math.floor(ms / 60000));
  const days = Math.floor(minutes / 1440);
  const hours = Math.floor((minutes % 1440) / 60);
  const unit = (n: number, u: 'day' | 'hour' | 'minute') =>
    new Intl.NumberFormat(i18n.locale, { style: 'unit', unit: u, unitDisplay: 'long' }).format(n);
  const parts = days
    ? [unit(days, 'day'), hours && unit(hours, 'hour')]
    : hours
      ? [unit(hours, 'hour'), minutes % 60 && unit(minutes % 60, 'minute')]
      : [unit(minutes, 'minute')];
  return new Intl.ListFormat(i18n.locale, { style: 'long', type: 'unit' }).format(
    parts.filter((p): p is string => !!p),
  );
}

/**
 * A measured time (e.g. how long a scan took), short and in the unit that suits its size:
 * "850 ms", "4.2 sec", "42 sec", "3 min, 12 sec", "2 hr, 5 min", "1 day, 3 hr". At most two
 * units; a second unit that is 0 is left out. Unit words come from Intl, so every language
 * gets its own.
 * `running`: a clock that is still counting, e.g. "0 sec", "4 sec": only whole seconds that have
 * passed, without ms or tenths, so it ticks once a second.
 */
export function formatElapsed(ms: number, running = false): string {
  const unit = (n: number, u: 'millisecond' | 'second' | 'minute' | 'hour' | 'day', digits = 0) =>
    new Intl.NumberFormat(i18n.locale, {
      style: 'unit',
      unit: u,
      unitDisplay: 'short',
      maximumFractionDigits: digits,
    }).format(n);
  const pair = (a: string, b: string | false) =>
    b ? new Intl.ListFormat(i18n.locale, { style: 'short', type: 'unit' }).format([a, b]) : a;

  ms = Math.max(0, ms);
  if (running) {
    ms = Math.floor(ms / 1000) * 1000;
    if (ms < 1000) return unit(0, 'second');
  }
  // The limits are checked after rounding, so 999.7 ms becomes "1 sec", not "1000 ms"
  if (Math.round(ms) < 1000) return unit(Math.round(ms), 'millisecond');
  // Below 10 seconds one decimal still says something, above it only adds noise
  if (!running && Math.round(ms / 100) < 100) return unit(Math.round(ms / 100) / 10, 'second', 1);

  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return unit(seconds, 'second');
  if (seconds < 3600) return pair(unit(Math.floor(seconds / 60), 'minute'), seconds % 60 > 0 && unit(seconds % 60, 'second'));

  const minutes = Math.round(seconds / 60);
  if (minutes < 1440) return pair(unit(Math.floor(minutes / 60), 'hour'), minutes % 60 > 0 && unit(minutes % 60, 'minute'));

  const hours = Math.round(minutes / 60);
  return pair(unit(Math.floor(hours / 24), 'day'), hours % 24 > 0 && unit(hours % 24, 'hour'));
}

/** A moment with weekday, date and time, e.g. "Mon, 11/10, 02:00". */
export function formatMoment(iso: string): string {
  return new Date(iso).toLocaleString(i18n.locale, {
    weekday: 'short',
    day: 'numeric',
    month: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}
