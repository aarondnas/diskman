// Running backups: the job polled from the backend (the running sync and the waiting queue), the
// "last run" status of every archive, and starting or cancelling runs.
import { backend } from '../api/backend.ts';
import type { Id, JobProgress, StatusMap } from '../api/types.ts';
import { t } from '../i18n/index.svelte.ts';
import { config } from './config.svelte.ts';
import { confirm } from './confirm.svelte.ts';
import { clearError, showError } from './notice.svelte.ts';

const state = $state<{
  job: JobProgress | null;
  status: StatusMap;
  statusLoaded: boolean;
  pending: Id[];
  batch: Id[];
  now: number;
}>({
  // Time of the latest poll: the running clocks (time of a run) tick with it
  now: Date.now(),
  job: null,
  status: {},
  // The status has arrived at least once (before that it is empty, not "never synced")
  statusLoaded: false,
  // Clicked backups until the backend confirms them, so the button reacts without delay
  pending: [],
  // Started by "Run all": cancelled there, not on their cards. Pruned once they are done.
  batch: [],
});

export const jobs = {
  /** Progress of the running sync and the waiting queue (commands/sync.rs, JobProgress). */
  get job() {
    return state.job;
  },
  /** Latest result per archive id. */
  get status() {
    return state.status;
  },
  get statusLoaded() {
    return state.statusLoaded;
  },
  /** Now, renewed once a second with the poll. */
  get now() {
    return state.now;
  },
  get runningId(): Id | null {
    return state.job?.running ? state.job.backup_id : null;
  },
  /** Waiting backups in order, including those just clicked. */
  get queuedIds(): Id[] {
    return [...(state.job?.queued ?? []), ...state.pending];
  },
  /** Backups of "Run all" that are still running or waiting. */
  get batchIds(): Id[] {
    return state.batch.filter(busy);
  },
  /** Started by "Run all" (then only cancelled there). */
  inBatch(id: Id) {
    return state.batch.includes(id);
  },
};

/** Running or waiting. */
const busy = (id: Id) => id === jobs.runningId || jobs.queuedIds.includes(id);

// Responses can overtake each other (polling every second, clicks in between): only apply the latest
let requests = 0;
let applied = 0;
async function fetchJob(request: Promise<JobProgress>) {
  const seq = ++requests;
  const result = await request;
  if (seq > applied) {
    applied = seq;
    state.job = result;
  }
}

async function poll() {
  state.now = Date.now();
  try {
    [, state.status] = await Promise.all([fetchJob(backend.job()), backend.status()]);
    state.statusLoaded = true;
    if (!state.batch.every(busy)) state.batch = state.batch.filter(busy);
  } catch {
    // Backend briefly unreachable, the next attempt follows
  }
}

/** Polls job and status every second; returns the function that stops it. */
export function startPolling() {
  poll();
  const timer = setInterval(poll, 1000);
  return () => clearInterval(timer);
}

/**
 * Queues the backups in this order; running or waiting ones are skipped. `batch`: started by
 * "Run all", which then also cancels them (`cancelBatch`).
 */
export async function runBackups(ids: Id[], batch = false) {
  const todo = ids.filter((id) => !busy(id));
  if (!todo.length) return;
  state.pending.push(...todo); // show as waiting right away
  if (batch) state.batch = [...state.batch.filter(busy), ...todo];
  clearError();
  try {
    for (const id of todo) {
      // Cancelled before it was sent: not queued at all
      if (!state.pending.includes(id)) continue;
      try {
        await fetchJob(backend.runBackup(id));
      } finally {
        const cancelled = !state.pending.includes(id);
        state.pending = state.pending.filter((p) => p !== id);
        // Cancelled while it was being sent: the cancel may have reached the backend first
        if (cancelled) await fetchJob(backend.cancelBackups([id]));
      }
    }
  } catch (e) {
    showError(e);
    state.pending = state.pending.filter((p) => !todo.includes(p));
  }
}

export const runBackup = (id: Id) => runBackups([id]);

/**
 * A backup started by hand (its card, the problems dialog): asks first, naming the backup, its
 * entries and, when one mirrors, that the archive also loses what the source no longer has.
 * ("Run all" asks on its own, for all of them at once.)
 */
export async function confirmAndRun(id: Id) {
  const backup = config.backups.find((b) => b.id === id);
  if (!backup) return;
  const enabled = backup.archives.filter((a) => a.enabled);
  const lines = [t('app.runConfirm', { name: backup.name, count: enabled.length })];
  if (enabled.some((a) => a.mode === 'mirror')) lines.push(t('app.runConfirmMirror'));
  if (await confirm(lines.join('\n'), t('common.runNow'), 'run')) await runBackup(id);
}

/** Cancels the running sync and empties the queue. */
export const cancelJobs = () => backend.cancel();

/** Cancels these backups only, running or waiting; the others go on. */
export async function cancelBackups(ids: Id[]) {
  if (!ids.length) return;
  // Those not yet sent are dropped right here (see runBackups)
  state.pending = state.pending.filter((p) => !ids.includes(p));
  try {
    await fetchJob(backend.cancelBackups(ids));
  } catch (e) {
    showError(e);
  }
}

/** "Run all" again while it runs: cancels its backups that are still running or waiting. */
export function cancelBatch() {
  const ids = jobs.batchIds;
  state.batch = [];
  return cancelBackups(ids);
}
