// Creating, changing and deleting backups (the editor and the list call these).
import type { Backup, BackupSchedule, Id } from '../api/types.ts';
import { t } from '../i18n/index.svelte.ts';
import { forgetAnalysis } from './analysis.svelte.ts';
import { forgetChecks } from './checks.svelte.ts';
import { forgetProblems } from './problems.svelte.ts';
import { defaultBackupName, updateBackup, updateConfig } from './config.svelte.ts';
import { confirm } from './confirm.svelte.ts';
import { nav, openEditor, restoreDraft } from './navigation.svelte.ts';

/**
 * Changes a new backup (the draft). It is saved by itself as soon as it has its first entry
 * (without a name as `defaultBackupName`) and from then on edited like any other backup; until
 * then it only lives in the navigation and is dropped when the page is left. If the backend
 * refuses it, the change is undone. Returns true on success.
 */
export async function updateDraft(mutate: (draft: Backup) => unknown): Promise<boolean> {
  const draft = nav.draft;
  if (!draft) return false;
  const before = $state.snapshot(draft);
  mutate(draft);
  if (!draft.archives.length) return true;
  const backup = $state.snapshot(draft);
  backup.name = backup.name.trim() || defaultBackupName();
  if (!(await updateConfig((c) => c.backups.push(backup)))) {
    restoreDraft(before);
    return false;
  }
  openEditor(backup.id);
  return true;
}

/**
 * Deletes a backup after asking. Its entries and schedule are part of it; the backend removes the
 * rest (history, waiting runs, a running sync: AppState::forget), here its check results go.
 */
export async function removeBackup(backup: Backup) {
  if (!(await confirm(t('app.removeConfirm', { name: backup.name }), t('common.remove')))) return;
  if (!(await updateConfig((c) => (c.backups = c.backups.filter((x) => x.id !== backup.id))))) return;
  forgetChecks(backup.id);
  forgetAnalysis(backup.id);
  forgetProblems(backup.id);
}

function applySchedule(backup: Backup, schedule: BackupSchedule | null) {
  if (schedule) backup.schedule = structuredClone(schedule);
  else delete backup.schedule;
}

/** Saves the schedule of a backup (null removes it); true on success. */
export function saveSchedule(id: Id, schedule: BackupSchedule | null) {
  return updateBackup(id, (b) => applySchedule(b, schedule));
}

/**
 * Writes one schedule (null removes it) into several backups at once, in one save. Nothing links
 * them afterwards: each backup keeps its own copy and can be changed on its own. True on success.
 */
export function saveSchedules(ids: Id[], schedule: BackupSchedule | null) {
  return updateConfig((c) => {
    for (const b of c.backups) if (ids.includes(b.id)) applySchedule(b, schedule);
  });
}
