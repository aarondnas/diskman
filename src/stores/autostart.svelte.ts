// Starting Diskman at login (src-tauri/src/platform/autostart.rs): whether the system does it, and
// the question on the first start. The system's entry is the truth; it is read, not stored here.
import { backend } from '../api/backend.ts';
import { t } from '../i18n/index.svelte.ts';
import { confirm } from './confirm.svelte.ts';
import { showError } from './notice.svelte.ts';

const state = $state({ loaded: false, enabled: false, asked: true });

export const autostart = {
  get loaded() {
    return state.loaded;
  },
  get enabled() {
    return state.enabled;
  },
  /** Answered already (in the app or the installer): the app does not ask. */
  get asked() {
    return state.asked;
  },
};

export async function loadAutostart() {
  try {
    Object.assign(state, await backend.autostart(), { loaded: true });
  } catch {
    // Without an answer nobody is asked; the settings show it as off
    Object.assign(state, { loaded: true, enabled: false, asked: true });
  }
}

/** Switches it on or off; true on success. */
export async function setAutostart(enabled: boolean) {
  try {
    Object.assign(state, await backend.setAutostart(enabled));
    return true;
  } catch (e) {
    showError(e);
    return false;
  }
}

let asking = false;

/** The question on the first start; either answer is remembered, so it comes only once. */
export async function askAutostart() {
  if (asking || state.asked) return;
  asking = true;
  try {
    const yes = await confirm(
      `${t('startup.question')}\n${t('startup.detail')}`,
      t('startup.yes'),
      'startup',
      t('startup.no'),
    );
    await setAutostart(yes);
  } finally {
    asking = false;
  }
}
