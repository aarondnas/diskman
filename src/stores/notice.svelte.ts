// The message bar below the page title: the last error the backend reported (one at a time).

const state = $state({ error: '' });

export const notice = {
  get error() {
    return state.error;
  },
};

/** Shows an error: an Error's message, or the value itself as text. */
export function showError(e: unknown) {
  state.error = (e as Error | null | undefined)?.message ?? String(e ?? '');
}

export function clearError() {
  state.error = '';
}
