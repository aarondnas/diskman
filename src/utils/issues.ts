// Texts for entries that cannot be (or were not) transferred: the reason codes come from the
// backend (src-tauri/src/sync/issues.rs) and are explained here in the UI's language.
import type { Issue, ObjectKind } from '../api/types.ts';
import { t } from '../i18n/index.svelte.ts';
import { ICONS } from './icons.ts';

/** Short label of the reason, e.g. "No permission". */
export const reasonLabel = (issue: Issue) => t(`issues.reasons.${issue.reason}`);

/** What it means and what helps; for unknown errors the system's message. */
export function reasonHint(issue: Issue): string {
  if (issue.reason === 'other') return issue.detail ?? '';
  if (issue.reason === 'case_conflict') return t('issues.hints.case_conflict', { other: issue.detail ?? '' });
  return t(`issues.hints.${issue.reason}`);
}

/** The path as shown; the source folder itself has the empty path. */
export const issuePath = (issue: Issue) => issue.path || t('issues.root');

const OBJECT_ICONS: Partial<Record<ObjectKind, string>> = { folder: ICONS.folder, link: ICONS.link, special: ICONS.drive };

/** Icon of the kind of object: file, folder, link or special file (pipe, socket, device). */
export const objectIcon = (issue: Issue) => OBJECT_ICONS[issue.object] ?? ICONS.document;

/** Skipped on purpose: links and special files. Everything else means something is missing in the archive. */
const SKIPPED: Issue['reason'][] = ['link', 'special'];
export const isFailure = (issue: Issue) => !SKIPPED.includes(issue.reason);

/** Number of entries that administrator rights could probably make readable. */
export const adminCount = (issues: Issue[]) => issues.filter((i) => i.admin).length;
