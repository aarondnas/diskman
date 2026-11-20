// Runs missed while Diskman was off (src-tauri/src/jobs/catch_up.rs): the backend finds them at
// its start for schedules that catch up; the app asks once whether to run them now.
import { backend } from '../api/backend.ts';
import type { Id, MissedRun } from '../api/types.ts';
import { showError } from './notice.svelte.ts';

const state = $state<{ runs: MissedRun[] }>({ runs: [] });

export const missed = {
  /** In the order of the config; empty when answered. */
  get runs() {
    return state.runs;
  },
};

/** Asks the backend once after the start; nothing missed (or no answer) leaves the list empty. */
export async function loadMissed() {
  try {
    state.runs = await backend.missedRuns();
  } catch {
    state.runs = [];
  }
}

/** Queues the chosen backups (as catch-up runs) and closes the question. */
export async function runMissed(ids: Id[]) {
  state.runs = [];
  try {
    await backend.runMissed(ids);
  } catch (e) {
    showError(e);
  }
}

/** Closes the question without running anything; the schedules go on as usual. */
export async function dismissMissed() {
  state.runs = [];
  try {
    await backend.dismissMissed();
  } catch {
    // Only the question is affected; it may come back after a reload of the window
  }
}
