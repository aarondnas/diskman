// Question with two buttons, shown by ConfirmDialog.svelte (mounted once in App.svelte).
// The first line of the message is the question, further lines are shown smaller below it.

/**
 * 'delete' (red, trash can), 'run' (amber, play symbol) for starting something, 'admin' (amber,
 * shield) for restarting with administrator rights or 'startup' (amber, power symbol) for
 * starting at login.
 */
export type ConfirmKind = 'delete' | 'run' | 'admin' | 'startup';

export interface PendingQuestion {
  question: string;
  detail: string;
  ok: string;
  cancel: string | null;
  kind: ConfirmKind;
  resolve: (answer: boolean) => void;
}

/** The open question, or null. */
export const pending = $state<{ current: PendingQuestion | null }>({ current: null });

/**
 * Resolves with true when `ok` (label of the confirming button) was pressed. `cancel` labels the
 * other button (default: Cancel), for a yes/no question.
 */
export function confirm(message: string, ok: string, kind: ConfirmKind = 'delete', cancel: string | null = null) {
  // Only one question at a time; an open one counts as cancelled
  pending.current?.resolve(false);
  const [question, ...rest] = String(message).split('\n');
  // Texts put the remark in parentheses, which the dialog does not need
  const detail = rest
    .map((l) => l.trim().replace(/^\((.*)\)$/, '$1'))
    .filter(Boolean)
    .join('\n');
  return new Promise<boolean>((resolve) => {
    pending.current = {
      question,
      detail,
      ok,
      cancel,
      kind,
      resolve: (answer) => {
        pending.current = null;
        resolve(answer);
      },
    };
  });
}
