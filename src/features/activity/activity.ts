// What the activity list and its dialog derive from an activity (src/api/types.ts, Activity).
import type { Activity, ActivityOutcome } from '../../api/types.ts';
import { i18n } from '../../i18n/index.svelte.ts';

/** How an activity is shown: its result, or "running" while it has none yet. */
export type ShownOutcome = ActivityOutcome | 'running';

export function outcomeOf(a: Activity): ShownOutcome {
  return a.finished_at ? (a.outcome ?? 'cancelled') : 'running';
}

/** The last folder of a path ("D:\Photos\2026" → "2026"); a drive or root stays whole. */
export function entryName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, '');
  const name = trimmed.split(/[\\/]/).pop() ?? '';
  return !name || /^[A-Za-z]:$/.test(trimmed) ? path : name;
}

/** Date and time of a moment, each on its own (the list shows them one below the other). */
export function whenParts(iso: string): { date: string; time: string } {
  const d = new Date(iso);
  const thisYear = d.getFullYear() === new Date().getFullYear();
  return {
    date: d.toLocaleDateString(i18n.locale, {
      weekday: 'short',
      day: '2-digit',
      month: '2-digit',
      ...(thisYear ? {} : { year: 'numeric' }),
    }),
    time: d.toLocaleTimeString(i18n.locale, { hour: '2-digit', minute: '2-digit' }),
  };
}

/** The figures of all entries of an activity together. */
export function totals(a: Activity) {
  const sum = (f: (e: Activity['entries'][number]) => number) => a.entries.reduce((n, e) => n + f(e), 0);
  return {
    copy: sum((e) => e.summary.copy_files),
    update: sum((e) => e.summary.update_files),
    move: sum((e) => e.summary.move_files),
    delete: sum((e) => e.summary.delete_files),
    /** What a run copied, or what an analysis would copy */
    bytesDone: sum((e) => e.bytes_done),
    bytesTotal: sum((e) => e.bytes_total),
    errors: sum((e) => e.error_count),
    blocked: sum((e) => e.summary.blocked),
  };
}
