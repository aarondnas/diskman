// Which page is shown. The backup list is the home page; editing a backup (or drafting a new one)
// is its subpage; activities, settings and info are pages of their own.
import type { Backup, Id } from '../api/types.ts';
import { findBackup } from './config.svelte.ts';

export type Page = 'backups' | 'activity' | 'settings' | 'info';

const state = $state<{ page: Page; editingId: Id | null; draft: Backup | null }>({
  page: 'backups',
  editingId: null,
  draft: null,
});

export const nav = {
  get page() {
    return state.page;
  },
  /** The backup being edited; null if there is none (or it was deleted meanwhile). */
  get editing() {
    return state.editingId ? findBackup(state.editingId) : null;
  },
  /** A new backup that is not saved yet (see backups.svelte.ts), or null. */
  get draft() {
    return state.draft;
  },
  /** In the editor of a backup or a draft. */
  get inEditor() {
    return !!(this.editing || state.draft);
  },
  /** The backup list itself is shown (not a subpage, not another page). */
  get onList() {
    return state.page === 'backups' && !this.inEditor;
  },
};

/** Shows a page; leaves an open editor (an unsaved draft is dropped). */
export function navigate(page: Page) {
  state.page = page;
  state.editingId = null;
  state.draft = null;
}

export function openEditor(id: Id) {
  state.page = 'backups';
  state.editingId = id;
  state.draft = null;
}

export function startDraft() {
  state.page = 'backups';
  state.editingId = null;
  state.draft = { id: crypto.randomUUID(), name: '', archives: [] };
}

/** Puts back an earlier state of the draft (after a refused save). */
export function restoreDraft(draft: Backup) {
  state.draft = draft;
}
