// Totals of a check (the plans of one or more backups), shown as a capsule (CheckSummary.svelte).
import type { CheckProgress, Plan } from '../api/types.ts';
import { t, type TextKey } from '../i18n/index.svelte.ts';
import { formatCount, formatElapsed } from './format.ts';

/** Counted parts of a check, in this order: [count of one plan, text key] */
export const PARTS: [count: (plan: Plan) => number, label: TextKey][] = [
  [(p) => p.blocked_failures, 'preview.stats.blocked'],
  [(p) => p.summary.copy_files, 'preview.stats.new'],
  [(p) => p.summary.update_files, 'preview.stats.updated'],
  [(p) => p.summary.move_files, 'preview.stats.moved'],
  [(p) => p.summary.delete_files, 'preview.stats.deleted'],
];

/** Totals of the plans of a check: counts (per PARTS) and whether one would empty an archive. */
export interface CheckTotals {
  counts: number[];
  dangerous: boolean;
}

/**
 * Files and folders a check compared: every source and every archive once (entries of a backup
 * may share a source, and "Check all" reads a folder several backups have only once).
 */
export function covered(plans: Plan[]): CheckProgress {
  const seen = new Set<string>();
  const total = { files: 0, dirs: 0 };
  const add = (key: string, files: number, dirs: number) => {
    if (seen.has(key)) return;
    seen.add(key);
    total.files += files;
    total.dirs += dirs;
  };
  for (const p of plans) {
    add(`source:${p.source}`, p.src_files, p.src_dirs);
    add(`archive:${p.dest}`, p.dst_files, p.dst_dirs);
  }
  return total;
}

/**
 * Time, files and folders of a check, e.g. "3 min, 12 sec · 306,718 files · 38,018 folders".
 * `running`: of a check still running, its time in whole seconds (`formatElapsed`).
 */
export function statsText(ms: number, { files, dirs }: CheckProgress, running = false): string {
  return [
    formatElapsed(ms, running),
    t('preview.files', { count: files, n: formatCount(files) }),
    t('preview.folders', { count: dirs, n: formatCount(dirs) }),
  ].join(' · ');
}

export function summarize(plans: Plan[]): CheckTotals {
  return {
    counts: PARTS.map(([count]) => plans.reduce((n, p) => n + count(p), 0)),
    dangerous: plans.some((p) => p.dangerous),
  };
}
