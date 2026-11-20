// New versions of Diskman (src-tauri/src/updates.rs): what the backend knows, checking now,
// installing after asking, and hiding the notice of a version until the next start. The backend
// checks by itself once a day; this only reads its status, often while something is going on.
import { backend } from '../api/backend.ts';
import type { UpdateStatus } from '../api/types.ts';
import { t } from '../i18n/index.svelte.ts';
import { confirm } from './confirm.svelte.ts';
import { jobs } from './jobs.svelte.ts';
import { showError } from './notice.svelte.ts';

/** How often the status is read: quickly while checking or downloading, otherwise now and then. */
const FAST_MS = 500;
const SLOW_MS = 60_000;

const state = $state<{ status: UpdateStatus | null; hidden: string | null; lostHidden: boolean }>({
  status: null,
  hidden: null,
  lostHidden: false,
});

export const updates = {
  get status() {
    return state.status;
  },
  /** The version a notice is shown for: found, not hidden, and installable with the known key. */
  get notice(): string | null {
    const v = state.status?.available?.version ?? null;
    return v && v !== state.hidden && state.status?.lost !== 'untrusted' ? v : null;
  },
  /** Why updates no longer come this way, for the warning (until it is hidden). */
  get lost() {
    return state.lostHidden ? null : (state.status?.lost ?? null);
  },
  /** A sync runs or waits: no update now (the backend refuses it too). */
  get blocked() {
    return !!jobs.job?.running || jobs.queuedIds.length > 0;
  },
  /** Share downloaded, 0–100; null while the size is unknown. */
  get percent(): number | null {
    const s = state.status;
    return s?.total ? Math.min(100, Math.floor((s.downloaded / s.total) * 100)) : null;
  },
};

let timer: ReturnType<typeof setTimeout> | undefined;

async function refresh() {
  clearTimeout(timer);
  try {
    state.status = await backend.updateStatus();
  } catch {
    // Backend briefly unreachable: the next round tries again
  }
  const busy = state.status?.checking || state.status?.installing;
  timer = setTimeout(refresh, busy ? FAST_MS : SLOW_MS);
}

/** Reads the status now and then; returns the function that stops it. */
export function startUpdates() {
  refresh();
  return () => clearTimeout(timer);
}

/** "Check now" in the settings. */
export async function checkNow() {
  if (state.status) state.status = { ...state.status, checking: true, error: null };
  try {
    state.status = await backend.checkForUpdate();
  } catch (e) {
    showError(e);
  }
}

/** Installs the version found, after asking; Diskman ends and starts again with it. */
export async function installUpdate() {
  const version = state.status?.available?.version;
  if (!version || updates.blocked) return;
  if (!(await confirm(t('updates.confirm', { version }), t('updates.install'), 'run'))) return;
  // Fast reads show the download
  setTimeout(refresh, FAST_MS);
  try {
    await backend.installUpdate();
  } catch (e) {
    showError(e);
  } finally {
    refresh();
  }
}

/**
 * The website in the browser, to download the new version and install it over this one: for
 * copies that cannot update themselves, and when updates no longer come this way.
 */
export async function openWebsite() {
  try {
    await backend.openWebsite();
  } catch (e) {
    showError(e);
  }
}

/** Hides the notice of this version until Diskman starts again. */
export function hideNotice() {
  state.hidden = state.status?.available?.version ?? null;
}

/** Hides the warning that updates no longer come this way, until Diskman starts again. */
export function hideLost() {
  state.lostHidden = true;
}
