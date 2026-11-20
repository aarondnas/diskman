<script lang="ts">
  // Result of a check as a clickable capsule: an icon with the count per kind of change
  // (new, moved, deleted; updated as text), or "Up to date". The click opens the whole analysis.
  // Used on each backup card and next to "Check all".
  import { PARTS, type CheckTotals } from '../utils/checkSummary.ts';
  import { formatCount } from '../utils/format.ts';
  import { t, type TextKey } from '../i18n/index.svelte.ts';
  import { ICONS } from '../utils/icons.ts';

  interface Props {
    /** From summarize() (utils/checkSummary.ts). */
    summary: CheckTotals;
    /** Accessible name of the button. */
    label: string;
    onclick: () => void;
  }
  let { summary, label, onclick }: Props = $props();

  const parts = $derived(
    PARTS.map(([, key], i): [number, TextKey] => [summary.counts[i], key]).filter(([n]) => n),
  );
</script>

<button class="capsule" class:err={summary.dangerous} title={t('list.showAnalysis')} aria-label={label} {onclick}>
  {#if !parts.length}
    <span class="dot"></span>{t('list.upToDate')}
  {:else}
    {#each parts as [n, key], i}
      {#if i}<span class="sep">·</span>{/if}
      {#if key === 'preview.stats.blocked'}
        <svg class="ico blocked" viewBox="0 0 24 24" width="14" height="14" role="img" aria-label={t(key)}>
          <title>{t(key)}</title>
          <path fill="currentColor" d={ICONS.warning} />
        </svg>{formatCount(n)}
      {:else if key === 'preview.stats.new'}
        <svg class="ico new" viewBox="0 0 24 24" width="14" height="14" role="img" aria-label={t(key)}>
          <path fill="currentColor" d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8zm-1 7V3.5L18.5 9z" />
        </svg>{formatCount(n)}
      {:else if key === 'preview.stats.moved'}
        <svg class="ico moved" viewBox="0 0 24 24" width="14" height="14" role="img" aria-label={t(key)}>
          <path
            fill="currentColor"
            d="M20 6h-8l-2-2H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2zm-6 12v-3h-4v-4h4V8l5 5z"
          />
        </svg>{formatCount(n)}
      {:else if key === 'preview.stats.deleted'}
        <svg class="ico deleted" viewBox="0 0 24 24" width="14" height="14" role="img" aria-label={t(key)}>
          <path fill="currentColor" d="M6 19a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V7H6zM19 4h-3.5l-1-1h-5l-1 1H5v2h14z" />
        </svg>{formatCount(n)}
      {:else}
        {formatCount(n)} {t(key)}
      {/if}
    {/each}
  {/if}
  {#if summary.dangerous}<span class="sep">·</span>{t('list.emptiesArchive')}{/if}
  <svg class="more" viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
    <path fill="currentColor" d="M10 6 8.6 7.4l4.6 4.6-4.6 4.6L10 18l6-6z" />
  </svg>
</button>

<style>
  /* Clickable capsule; amber on hover like the other main actions */
  .capsule {
    position: relative;
    pointer-events: auto;
    display: inline-flex;
    align-items: center;
    max-width: 100%;
    min-height: 26px;
    padding: 2px 6px 2px 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    color: var(--muted);
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    cursor: pointer;
  }
  .capsule:hover:not(:disabled) {
    border-color: var(--amber);
    background: color-mix(in srgb, var(--amber) 12%, var(--surface));
    color: var(--text);
  }
  .capsule.err {
    color: var(--danger);
  }
  .ico {
    flex: none;
    margin-right: 3px;
  }
  .ico.new {
    color: var(--ok);
  }
  .ico.moved {
    color: var(--warn);
  }
  .ico.deleted {
    color: var(--danger);
  }
  .ico.blocked {
    color: var(--warn);
  }
  .sep {
    margin: 0 5px;
    opacity: 0.6;
  }
  .dot {
    flex: none;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--ok);
    margin-right: 6px;
  }
  .more {
    flex: none;
    margin-left: 2px;
    opacity: 0.6;
  }
  .capsule:hover .more {
    opacity: 1;
  }
</style>
