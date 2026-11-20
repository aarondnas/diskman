// The analysis dialog (features/analysis): which analysis is open, and the finished checks
// waiting to open by themselves.
//
// A finished check opens its analysis by itself, but only while the backup list is shown (the
// list page reports that with `setListShown`): what finishes elsewhere stays a result on its card
// or next to "Check all" and is opened from there. If an analysis is open already, the next one
// waits until it is closed; leaving the list drops the waiting ones.
import type { Id, Plan } from '../api/types.ts';
import { checkAll, checkBackup, onProbed, overall, plansOf } from './checks.svelte.ts';

/** Waiting item for "Check all" (otherwise items are backup ids). */
const ALL = 'all';

/** An open analysis: of one backup (its id), or of "Check all" (request null). */
export interface OpenAnalysis {
  request: Id | null;
  plans: Plan[];
}

const state = $state<{ open: OpenAnalysis | null; waiting: (Id | typeof ALL)[]; listShown: boolean }>({
  open: null,
  waiting: [],
  listShown: false,
});

export const analysis = {
  get open() {
    return state.open;
  },
};

export function showAnalysis(request: Id | null, plans: Plan[]) {
  state.open = { request, plans };
}

// The open analysis takes the plans of a backup again once their probe is done
onProbed((plans) => {
  const open = state.open;
  if (!open || !open.plans.some((p) => plans.some((n) => n.id === p.id))) return;
  state.open = { ...open, plans: open.plans.map((p) => plans.find((n) => n.id === p.id) ?? p) };
});

export function closeAnalysis() {
  state.open = null;
}

export function setListShown(shown: boolean) {
  state.listShown = shown;
  if (!shown) state.waiting = [];
}

function offer(item: Id | typeof ALL) {
  if (state.listShown && !state.waiting.includes(item)) state.waiting.push(item);
}

/**
 * Opens the next waiting analysis unless something is in the way (`blocked`, an open analysis);
 * skips results that became outdated meanwhile. Call it from an effect: it reruns by itself when
 * the dialog closes or something new waits.
 */
export function openNext(blocked: boolean) {
  if (!state.listShown || blocked || state.open || !state.waiting.length) return;
  const [item, ...rest] = state.waiting;
  state.waiting = rest;
  const plans = item === ALL ? overall.result?.plans : plansOf(item);
  if (plans?.length) state.open = { request: item === ALL ? null : item, plans };
}

/** "Check" on a card: its analysis opens when done. */
export async function checkAndShow(id: Id) {
  if (await checkBackup(id)) offer(id);
}

/** "Check all": the analysis of all of them opens when done. */
export async function checkAllAndShow(ids: Id[]) {
  await checkAll(ids);
  offer(ALL);
}

/** "Rescan" in the open analysis: checks again and shows the new result in place. */
export async function rescan(allIds: Id[]) {
  const request = state.open?.request;
  if (request) {
    const plans = await checkBackup(request);
    if (state.open?.request === request) state.open = plans ? { request, plans } : null;
  } else {
    const result = await checkAll(allIds);
    if (state.open && !state.open.request) state.open = result ? { request: null, plans: result.plans } : null;
  }
}

/** A deleted backup no longer waits to be shown. */
export function forgetAnalysis(id: Id) {
  state.waiting = state.waiting.filter((w) => w !== id);
}
