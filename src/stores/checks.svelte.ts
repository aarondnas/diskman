// Results of "Check" (what a sync would change, without running it): per backup, and of
// "Check all". Kept here and not in a page, so a check that finishes while another page is open
// still shows its result on the card when the list comes back.
import { backend } from '../api/backend.ts';
import type { CheckProgress, Id, Plan } from '../api/types.ts';
import { summarize, type CheckTotals } from '../utils/checkSummary.ts';

/**
 * State of the check of one backup: running, failed, or its result.
 * `batch`: started by "Check all", so it is cancelled there and not on its card.
 * `started` / `elapsedMs`: when it started (Date.now()) and how long it took until the result,
 * waiting for its turn included; the probe that follows is not part of it.
 */
export type CheckState =
  | ({ loading: true; token: number; batch: boolean; started: number } & NoneOf<
      'error' | 'plans' | 'elapsedMs' | keyof CheckTotals
    >)
  | ({ error: string } & NoneOf<'loading' | 'token' | 'batch' | 'started' | 'plans' | 'elapsedMs' | keyof CheckTotals>)
  | (CheckTotals & { plans: Plan[]; elapsedMs: number } & NoneOf<'loading' | 'token' | 'batch' | 'started' | 'error'>);

/** The fields of the other states, absent here (so each can be read on any state). */
type NoneOf<K extends string> = { [P in K]?: undefined };

/** Result of "Check all": the plans of all checked backups, their totals and how long it took. */
export interface OverallResult {
  plans: Plan[];
  summary: CheckTotals;
  elapsedMs: number;
}

/**
 * Live view of the running checks: a clock for their timers, and per backup the files and folders
 * its check has read so far (asked from the backend twice a second while any check runs).
 */
export const live = $state<{ now: number; progress: Record<Id, CheckProgress> }>({ now: Date.now(), progress: {} });

let ticker: ReturnType<typeof setInterval> | null = null;
let asking = false;

function tick() {
  live.now = Date.now();
  const running = Object.keys(checks).filter((id) => checks[id]?.loading);
  for (const id of Object.keys(live.progress)) {
    if (!checks[id]?.loading) delete live.progress[id];
  }
  if (!running.length) {
    if (ticker) clearInterval(ticker);
    ticker = null;
    return;
  }
  // A slow answer is not asked for again before it arrives
  if (asking) return;
  asking = true;
  backend
    .checkProgress(running)
    .then((found) => {
      for (const [id, p] of Object.entries(found)) {
        if (checks[id]?.loading) live.progress[id] = p;
      }
    })
    .catch(() => {})
    .finally(() => (asking = false));
}

function startTicking() {
  live.now = Date.now();
  ticker ??= setInterval(tick, 500);
}

/** Per backup id. */
export const checks = $state<Record<Id, CheckState>>({});

// ids: the backups the running "Check all" checks
const all = $state<{ result: OverallResult | null; running: boolean; ids: Id[] }>({
  result: null,
  running: false,
  ids: [],
});

/** "Check all": its result (null without one) and whether it is running. */
export const overall = {
  get result() {
    return all.result;
  },
  get running() {
    return all.running;
  },
};

let tokens = 0;
// Counts the runs of "Check all"; a cancel moves it on, so the cancelled run knows it is outdated
let allRuns = 0;
// Token of the check whose result is being probed, per backup (see `probe`)
const probing: Record<Id, number> = {};
// Called with the plans of a backup once its probe is done (the open analysis shows them)
const probeListeners: ((plans: Plan[]) => void)[] = [];

/** `fn` gets the plans of a backup again when the second step of its check has finished. */
export function onProbed(fn: (plans: Plan[]) => void) {
  probeListeners.push(fn);
}

/**
 * Builds the plans for all enabled archives of a backup and keeps them with their totals. Checks
 * of the same drive take turns in the backend, so a check may first wait (shown as "checking").
 * Returns the plans, or null if it failed, was cancelled or the result is outdated: a sync of the
 * backup was queued meanwhile (see `dropChecks`), or a newer check of it was started.
 * `batch`: part of "Check all", whose number `group` lets its backups share scans.
 * The result shows at once; whether the files to copy can be read follows (`probe`).
 */
export async function checkBackup(id: Id, batch = false, group: number | null = null): Promise<Plan[] | null> {
  const token = ++tokens;
  const started = Date.now();
  checks[id] = { loading: true, token, batch, started };
  delete live.progress[id];
  startTicking();
  try {
    const plans = await backend.preview(id, group);
    if (checks[id]?.token !== token) return null;
    if (!plans) {
      // Cancelled: no result, the card shows its last run again
      delete checks[id];
      return null;
    }
    checks[id] = { ...summarize(plans), plans, elapsedMs: Date.now() - started };
    void probe(id, token);
    return plans;
  } catch (e) {
    if (checks[id]?.token === token) checks[id] = { error: (e as Error).message };
    return null;
  }
}

/**
 * Second step of a check, in the background: the backend opens the files the plans would copy
 * and moves locked and unreadable ones to "not transferable". Replaces the result if it is still
 * the one of check `token`; also in "Check all" and in the open analysis (`onProbed`).
 */
async function probe(id: Id, token: number) {
  probing[id] = token;
  let plans: Plan[] | null = null;
  try {
    plans = await backend.probePlans(id);
  } catch {
    // Not checked: the run reports what it cannot read
  }
  if (probing[id] !== token) return;
  delete probing[id];
  if (!plans || !checks[id]?.plans) return;
  const ids = new Set(checks[id].plans.map((p) => p.id));
  if (!plans.every((p) => ids.has(p.id))) return;
  checks[id] = { ...summarize(plans), plans, elapsedMs: checks[id].elapsedMs };
  if (all.result?.plans.some((p) => ids.has(p.id))) {
    const merged = all.result.plans.map((p) => plans!.find((n) => n.id === p.id) ?? p);
    all.result = { ...all.result, plans: merged, summary: summarize(merged) };
  }
  for (const fn of probeListeners) fn(plans);
}

let groups = 0;

/**
 * "Check all": checks the given backups (those already being checked are not checked twice) and
 * keeps the totals of all of them. All are asked at once: the backend runs checks of different
 * drives side by side and lets those of one drive take turns; a folder that several of them
 * have is read once. Returns the result, null without any plan or when cancelled.
 */
export async function checkAll(ids: Id[]): Promise<OverallResult | null> {
  const run = ++allRuns;
  const group = ++groups;
  const started = Date.now();
  all.running = true;
  all.result = null;
  try {
    const todo = ids.filter((id) => !checks[id]?.loading);
    all.ids = todo;
    await Promise.all(todo.map((id) => checkBackup(id, true, group)));
    if (run !== allRuns) return null;
    const plans = ids.flatMap((id) => checks[id]?.plans ?? []);
    all.result = plans.length ? { plans, summary: summarize(plans), elapsedMs: Date.now() - started } : null;
    return all.result;
  } finally {
    backend.endCheckBatch(group).catch(() => {});
    if (run === allRuns) all.running = false;
  }
}

/**
 * Cancels the check of one backup started on its card. Its card shows its last run again at
 * once; the backend stops scanning shortly after.
 */
export function cancelCheck(id: Id) {
  if (!checks[id]?.loading) return;
  delete checks[id];
  backend.cancelChecks([id]).catch(() => {});
}

/**
 * Cancels the running "Check all": the backups still waiting or being checked lose their
 * "checking", those already checked keep their result on their card. No overall result.
 */
export function cancelCheckAll() {
  if (!all.running) return;
  allRuns++;
  all.running = false;
  all.result = null;
  const ids = all.ids.filter((id) => checks[id]?.batch);
  for (const id of ids) delete checks[id];
  all.ids = [];
  if (ids.length) backend.cancelChecks(ids).catch(() => {});
}

/** Plans of the last check of a backup while it is still valid, otherwise null. */
export const plansOf = (id: Id) => checks[id]?.plans ?? null;

/** A sync makes the results of these backups outdated; the backend may forget their plans. */
export function dropChecks(ids: (Id | null | undefined)[]) {
  const gone = ids.filter((id): id is Id => !!id && !!checks[id]);
  if (!gone.length) return;
  for (const id of gone) {
    delete checks[id];
    delete probing[id];
  }
  // Stops a probe that may still run, then drops the plans
  backend.cancelChecks(gone).catch(() => {});
  backend.dropPlans(gone).catch(() => {});
}

/** A sync makes the result of "Check all" outdated. */
export function dropOverall() {
  all.result = null;
}

/** A deleted backup leaves no result behind, also not in that of "Check all". */
export function forgetChecks(id: Id) {
  dropChecks([id]);
  const plans = all.result?.plans.filter((p) => p.backup_id !== id) ?? [];
  all.result = all.result && plans.length ? { ...all.result, plans, summary: summarize(plans) } : null;
}
