// Problems of the last runs: a notice when a run did not transfer everything, and the dialog with
// the details (features/problems/ProblemsDialog.svelte).
//
// The notice comes from the status polled every second (stores/jobs.svelte.ts): when an archive
// gets a new result, the problems of its backup are fetched again. A notice appears for runs that
// end while Diskman is open; older problems stay visible on the backup's card, where a click opens
// the same details. A later run without problems removes the notice by itself.
import { backend } from '../api/backend.ts';
import type { Id, RunProblem, StatusMap } from '../api/types.ts';
import { config } from './config.svelte.ts';

/** One per backup whose latest run did not transfer everything. */
export interface ProblemNotice {
  backupId: Id;
  name: string;
  /** Entries that failed. */
  count: number;
  /** Failed or skipped as a whole (no list of single entries). */
  failed: boolean;
}

const state = $state<{ notices: ProblemNotice[]; open: Id | null }>({ notices: [], open: null });

export const problems = {
  get notices() {
    return state.notices;
  },
  /** Backup whose details are shown, or null. */
  get open() {
    return state.open;
  },
};

export function openProblems(backupId: Id) {
  state.open = backupId;
}

export function closeProblems() {
  state.open = null;
}

export function dismissNotice(backupId: Id) {
  state.notices = state.notices.filter((n) => n.backupId !== backupId);
}

/** What went wrong in the latest run of each archive of a backup; cancelled runs are no problem. */
export async function loadProblems(backupId: Id): Promise<RunProblem[]> {
  const list = await backend.runProblems(backupId);
  return list.filter((p) => p.outcome !== 'cancelled');
}

// Archive id → time of its result seen last; null until the first status arrived
let seen: Record<Id, string> | null = null;
// Per backup: number of the latest request, so a slower older answer does not win
const requests: Record<Id, number> = {};

/** Call with every new status. */
export function watchStatus(status: StatusMap) {
  const before = seen;
  seen = Object.fromEntries(Object.entries(status).map(([id, st]) => [id, st.last_run]));
  // What was there at the start is old news (it shows on the cards)
  if (!before) return;
  const changed = Object.keys(status).filter((id) => before[id] !== status[id].last_run);
  const owners = new Set(
    changed
      .map((id) => config.backups.find((b) => b.archives.some((a) => a.id === id))?.id)
      .filter((id): id is Id => !!id),
  );
  for (const id of owners) refreshNotice(id);
}

async function refreshNotice(backupId: Id) {
  const seq = (requests[backupId] = (requests[backupId] ?? 0) + 1);
  let list: RunProblem[];
  try {
    list = await loadProblems(backupId);
  } catch {
    return; // The card still shows the result
  }
  if (seq !== requests[backupId]) return;
  const rest = state.notices.filter((n) => n.backupId !== backupId);
  const backup = config.backups.find((b) => b.id === backupId);
  if (!list.length || !backup) {
    state.notices = rest;
    return;
  }
  const count = list.reduce((n, p) => n + p.error_count, 0);
  // Failed, skipped or interrupted as a whole: there is no list of single entries
  const failed = list.some((p) => p.outcome === 'failed' || p.outcome === 'skipped' || p.outcome === 'interrupted');
  state.notices = [...rest, { backupId, name: backup.name, count, failed }];
}

/** A deleted backup leaves no notice behind. */
export function forgetProblems(backupId: Id) {
  dismissNotice(backupId);
  if (state.open === backupId) state.open = null;
}
