// How the backup list is shown: search text and sort order (the order is remembered).
import type { Backup } from '../../api/types.ts';
import { t } from '../../i18n/index.svelte.ts';
import { loadPreference, savePreference } from '../../utils/storage.ts';

export interface Sort {
  by: 'name' | 'created';
  dir: 'asc' | 'desc';
}

export interface ListView {
  query: string;
  sort: Sort;
}

const SORT_KEY = 'backupSort';
const validSort = (s: unknown): s is Sort => {
  const v = s as Partial<Sort> | null;
  return ['name', 'created'].includes(v?.by as string) && ['asc', 'desc'].includes(v?.dir as string);
};

export const view = $state<ListView>({
  query: '',
  sort: loadPreference<Sort>(SORT_KEY, { by: 'created', dir: 'asc' }, validSort),
});

export function setSort(next: Partial<Sort>) {
  view.sort = { ...view.sort, ...next };
  savePreference(SORT_KEY, view.sort);
}

export const toggleSortDirection = () => setSort({ dir: view.sort.dir === 'asc' ? 'desc' : 'asc' });

export function clearQuery() {
  view.query = '';
}

const collator = new Intl.Collator('en', { sensitivity: 'base', numeric: true });

/**
 * The backups to show, in order: matching the search (name or a folder, ignoring case) and
 * sorted. New backups are always appended, so the order of the config is the creation order.
 */
export function visibleBackups(backups: Backup[], { query, sort }: ListView): Backup[] {
  const q = query.trim().toLowerCase();
  const list = backups.filter(
    (b) =>
      !q ||
      b.name.toLowerCase().includes(q) ||
      b.archives.some((a) => a.source.toLowerCase().includes(q) || a.path.toLowerCase().includes(q)),
  );
  if (sort.by === 'name') list.sort((a, b) => collator.compare(a.name, b.name));
  return sort.dir === 'desc' ? list.reverse() : list;
}

/** The current direction in words, e.g. "A → Z" or "Newest first". */
export function directionLabel({ by, dir }: Sort): string {
  if (by === 'name') return t(dir === 'asc' ? 'app.sortAz' : 'app.sortZa');
  return t(dir === 'asc' ? 'app.sortOldest' : 'app.sortNewest');
}
