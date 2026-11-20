// Svelte action for dialogs: <div role="dialog" use:focusTrap={'[data-autofocus]'}>
// Moves the keyboard focus into the dialog when it opens (to the first element matching the
// selector, otherwise the dialog itself), keeps Tab and Shift+Tab inside it, and gives the focus
// back to where it was when the dialog closes. Escape is left to the dialog.
import type { Action } from 'svelte/action';

const FOCUSABLE = 'button, input, select, textarea, a[href], [tabindex]:not([tabindex="-1"])';

export const focusTrap: Action<HTMLElement, string | undefined> = (node, initial = '[data-autofocus]') => {
  const returnFocus = document.activeElement as HTMLElement | null;
  (node.querySelector<HTMLElement>(initial) ?? node).focus();

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== 'Tab') return;
    const items = [...node.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
      (el) => !(el as HTMLButtonElement).disabled,
    );
    if (!items.length) return;
    const i = items.indexOf(document.activeElement as HTMLElement);
    const next = (i + (e.shiftKey ? -1 : 1) + items.length) % items.length;
    e.preventDefault();
    items[next].focus();
  }
  node.addEventListener('keydown', onkeydown);

  return {
    destroy() {
      node.removeEventListener('keydown', onkeydown);
      returnFocus?.focus?.();
    },
  };
};
