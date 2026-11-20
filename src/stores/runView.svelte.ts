// The run window (features/run): the live view of a running sync, opened from its card, and the
// details of a finished run (its activity in ActivityDialog). A live view that sees its run end
// turns into the details of that run by itself.
import { backend } from '../api/backend.ts';
import type { Activity, Id } from '../api/types.ts';
import { showError } from './notice.svelte.ts';

/** The open run window: live while `activity` is null, then the finished run. */
export interface OpenRun {
  backupId: Id;
  activity: Activity | null;
}

const state = $state<{ open: OpenRun | null }>({ open: null });

export const runView = {
  get open() {
    return state.open;
  },
};

/** The live view of the running sync of this backup. */
export function openRun(backupId: Id) {
  state.open = { backupId, activity: null };
}

/** The latest sync of a backup: the one that just ended, or the last one at all. */
async function lastRun(backupId: Id): Promise<Activity | null> {
  const [a] = await backend.activities({ backup_id: backupId, kind: 'sync', limit: 1 });
  return a ?? null;
}

/** The last run of a backup in detail (the card of a backup that is not running). */
export async function openLastRun(backupId: Id) {
  try {
    const activity = await lastRun(backupId);
    if (activity) state.open = { backupId, activity };
  } catch (e) {
    showError(e);
  }
}

/** The run of the open live view ended: the details of that run take its place. */
export async function finishRun() {
  const open = state.open;
  if (!open || open.activity) return;
  try {
    const activity = await lastRun(open.backupId);
    if (state.open === open) state.open = activity ? { ...open, activity } : null;
  } catch (e) {
    if (state.open === open) state.open = null;
    showError(e);
  }
}

export function closeRun() {
  state.open = null;
}
