// Entries that mirror into the same archive undo each other: a mirror makes the archive an exact
// copy of its own source, so every run deletes what the other one put there. The same goes for an
// archive a folder level (or more) above the other one: its mirror deletes or overwrites the inner
// archive, which the inner mirror then copies back. While a backup is edited, one warning about it
// stands at the top of the page with the other notices (App.svelte); the backend only rejects the
// same archive twice within one backup.
import type { Archive, Backup } from '../api/types.ts';
import { IS_WINDOWS } from '../platform/index.ts';

/** Comparable form of a path: `/` as separator, no trailing one, case ignored on Windows. */
function key(path: string): string {
  const p = path.trim().replace(/\\/g, '/').replace(/\/+$/, '');
  return IS_WINDOWS ? p.toLowerCase() : p;
}

/** The same folder, or one inside the other (any number of levels apart). */
function overlaps(a: string, b: string): boolean {
  return a === b || a.startsWith(`${b}/`) || b.startsWith(`${a}/`);
}

const mirrors = (a: Archive) => a.enabled && a.mode === 'mirror' && !!a.path.trim();

/**
 * The archives involved when entries of `backup` (as edited: a draft is not in `all` yet, and wins
 * over its saved version there) mirror into the same archive as another enabled mirror entry, or
 * into one above or below it; that one may belong to any backup. Each archive once, those of other
 * backups with the backup's name, e.g. ["E:\Backup", "E:\Backup\Photos (Photos)"]. Empty: no clash.
 */
export function mirrorClashes(backup: Backup, all: Backup[]): string[] {
  const entries = [backup, ...all.filter((b) => b.id !== backup.id)].flatMap((b) =>
    b.archives.filter(mirrors).map((entry) => ({ backup: b, entry, key: key(entry.path) })),
  );
  const label = (e: (typeof entries)[number]) =>
    e.backup.id === backup.id ? e.entry.path : `${e.entry.path} (${e.backup.name})`;
  const involved = new Set<string>();
  for (const own of entries.filter((e) => e.backup.id === backup.id)) {
    const others = entries.filter((e) => e.entry.id !== own.entry.id && overlaps(own.key, e.key));
    if (!others.length) continue;
    involved.add(label(own));
    for (const other of others) involved.add(label(other));
  }
  return [...involved];
}
