<script lang="ts">
  // The app's question dialog (stores/confirm.svelte.ts): a question, an optional remark and two
  // buttons; red with a trash can for deleting, amber with a play symbol for starting something,
  // amber with a shield for restarting with administrator rights, amber with a power symbol for
  // starting at login.
  import { fade, scale } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { pending, type ConfirmKind } from '../stores/confirm.svelte.ts';
  import { t } from '../i18n/index.svelte.ts';
  import { focusTrap } from '../utils/focusTrap.ts';
  import { ICONS } from '../utils/icons.ts';

  const q = $derived(pending.current);

  const answer = (yes: boolean) => q?.resolve(yes);
  const BADGES: Partial<Record<ConfirmKind, string>> = { run: ICONS.play, admin: ICONS.shield, startup: ICONS.power };
  const go = $derived(!!q && q.kind in BADGES);

  function onkeydown(e: KeyboardEvent) {
    if (q && e.key === 'Escape') {
      // Before the dialogs below, which would close as well
      e.preventDefault();
      e.stopImmediatePropagation();
      answer(false);
    }
  }
</script>

<svelte:window {onkeydown} />

{#if q}
  <div
    class="backdrop"
    role="presentation"
    transition:fade={{ duration: 120 }}
    onclick={(e) => e.target === e.currentTarget && answer(false)}
  >
    <!-- Cancel is focused, so a stray Enter does not delete anything -->
    <div
      use:focusTrap
      class="dialog"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="confirm-question"
      aria-describedby={q.detail ? 'confirm-detail' : undefined}
      transition:scale={{ duration: 140, start: 0.96, easing: cubicOut }}
    >
      <div class="body">
        <span class="badge" class:run={go} aria-hidden="true">
          <svg viewBox="0 0 24 24" width="22" height="22">
            <path fill="currentColor" d={BADGES[q.kind] ?? ICONS.trash} />
          </svg>
        </span>
        <div class="text">
          <h2 id="confirm-question">{q.question}</h2>
          {#if q.detail}
            <p id="confirm-detail">{q.detail}</p>
          {/if}
        </div>
      </div>
      <footer>
        <button data-autofocus class="ghost" onclick={() => answer(false)}>{q.cancel ?? t('common.cancel')}</button>
        <button class={go ? 'go' : 'destroy'} onclick={() => answer(true)}>{q.ok}</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  /* Above everything, also the top bar, like the native dialog before */
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgb(0 0 0 / 0.45);
    backdrop-filter: blur(2px);
  }
  .dialog {
    width: min(420px, 100%);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 14px;
    box-shadow: 0 20px 60px rgb(0 0 0 / 0.35);
    overflow: hidden;
  }
  .body {
    display: flex;
    gap: 14px;
    padding: 22px 22px 18px;
  }
  .badge {
    flex: none;
    display: grid;
    place-items: center;
    width: 42px;
    height: 42px;
    border-radius: 50%;
    background: var(--danger-soft);
    color: var(--danger);
  }
  /* Starting something (e.g. "Run all"): amber like the run buttons, nothing gets deleted */
  .badge.run {
    background: color-mix(in srgb, var(--amber) 22%, var(--surface));
    color: color-mix(in srgb, var(--amber) 70%, var(--text));
  }
  .go {
    background: var(--amber);
    border-color: var(--amber);
    color: var(--on-amber);
    font-weight: 600;
  }
  .go:hover:not(:disabled) {
    border-color: var(--amber);
    filter: brightness(1.1);
  }
  .go:focus-visible {
    outline-color: var(--amber);
  }
  .text {
    min-width: 0;
    padding-top: 1px;
  }
  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    line-height: 1.4;
    /* Long folder paths must not push the dialog wider */
    overflow-wrap: anywhere;
  }
  p {
    margin: 6px 0 0;
    font-size: 13px;
    color: var(--muted);
    white-space: pre-line;
    overflow-wrap: anywhere;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 16px;
    background: var(--surface-2);
    border-top: 1px solid var(--border-soft);
  }
  .destroy {
    background: var(--danger);
    border-color: var(--danger);
    /* The light red of the dark theme needs dark text */
    color: #2a0c10;
    font-weight: 600;
  }
  .destroy:hover:not(:disabled) {
    border-color: var(--danger);
    filter: brightness(1.1);
  }
  .destroy:focus-visible {
    outline-color: var(--danger);
  }
  /* The darker red of the light theme takes white text */
  :global(:root[data-theme='light']) .destroy {
    color: white;
  }
</style>
