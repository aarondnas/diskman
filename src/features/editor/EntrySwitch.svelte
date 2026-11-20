<script lang="ts">
  import { t } from '../../i18n/index.svelte.ts';

  // On/off for an entry: a dot in its top left corner that lights up while the entry is active.
  // The parent (the lane) must be position: relative (see .lane in BackupEditor.svelte).
  let { on, onchange }: { on: boolean; onchange: (on: boolean) => void } = $props();
</script>

<button
  type="button"
  class="dot"
  class:on
  role="switch"
  aria-checked={on}
  aria-label={t('card.entryActive')}
  title={t(on ? 'card.enabled' : 'card.disabled')}
  onclick={() => onchange(!on)}
></button>

<style>
  /* The button is the larger click area, the dot is drawn inside it */
  .dot {
    position: absolute;
    left: 6px;
    /* Middle on the label row of the entry (18 px) */
    top: 5px;
    width: 26px;
    height: 26px;
    min-height: 0;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: transparent;
  }
  .dot::after {
    content: '';
    position: absolute;
    inset: 8px;
    border-radius: 50%;
    border: 1.5px solid var(--muted);
    transition:
      background 0.2s,
      border-color 0.2s,
      box-shadow 0.2s;
  }
  .dot:hover:not(:disabled)::after {
    border-color: var(--text);
  }
  .dot.on::after {
    background: var(--amber);
    border-color: var(--amber);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--amber) 25%, transparent), 0 0 8px var(--amber);
  }
  .dot:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
</style>
