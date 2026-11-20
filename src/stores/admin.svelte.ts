// Restarting Diskman with administrator rights (Windows asks for permission). Offered where entries
// could not be read for lack of permission, only where the system allows it (stores/platform).
import { backend } from '../api/backend.ts';
import { t } from '../i18n/index.svelte.ts';
import { confirm } from './confirm.svelte.ts';
import { showError } from './notice.svelte.ts';

const state = $state({ restarting: false });

export const admin = {
  /** Waiting for the Windows permission prompt. */
  get restarting() {
    return state.restarting;
  },
};

/**
 * Asks, then restarts Diskman with administrator rights. This instance quits once Windows has
 * given permission; if it was declined, everything stays as it is.
 */
export async function restartAsAdmin() {
  if (state.restarting || !(await confirm(t('issues.admin.confirm'), t('issues.admin.confirmOk'), 'admin'))) return;
  state.restarting = true;
  try {
    await backend.restartAsAdmin();
  } catch (e) {
    showError(e);
  } finally {
    state.restarting = false;
  }
}
