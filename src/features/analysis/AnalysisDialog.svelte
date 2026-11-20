<script lang="ts">
  import { fly } from 'svelte/transition';
  import type { Id, Plan } from '../../api/types.ts';
  import { cubicOut } from 'svelte/easing';
  import { formatCount, formatElapsed } from '../../utils/format.ts';
  import { covered } from '../../utils/checkSummary.ts';
  import { checks, overall } from '../../stores/checks.svelte.ts';
  import { config } from '../../stores/config.svelte.ts';
  import { jobs, runBackups } from '../../stores/jobs.svelte.ts';
  import PipelineReport from './PipelineReport.svelte';
  import AdminHint from '../../components/AdminHint.svelte';
  import { t } from '../../i18n/index.svelte.ts';
  import { pending as confirmPending } from '../../stores/confirm.svelte.ts';
  import { focusTrap } from '../../utils/focusTrap.ts';
  import { ICONS } from '../../utils/icons.ts';

  // Analysis of what a sync would change; ▶ in its header runs what it shows (the backup, or all
  // backups of "Check all") and closes it. The same window for one backup ("Check") and for all ("Check all"):
  // every pipeline (entry: source → archive) in its own section, one below the other, scrolling;
  // with several backups every backup is a page of its own, switched with the arrows of the page bar
  // (or ← / →) and its dots.
  interface Props {
    plans: Plan[];
    onrefresh: () => Promise<unknown>;
    onclose: () => void;
  }
  let { plans, onrefresh, onclose }: Props = $props();

  let refreshing = $state(false);

  // Plans grouped by backup, in the order they came
  const groups = $derived.by(() => {
    const list: { id: Id; name: string; plans: Plan[] }[] = [];
    for (const p of plans) {
      const last = list.at(-1);
      if (last?.id === p.backup_id) last.plans.push(p);
      else list.push({ id: p.backup_id, name: p.backup_name, plans: [p] });
    }
    return list;
  });
  const overview = $derived(groups.length > 1);

  // Page = one backup; dir: direction of the last change, so the new page slides in from that side
  let page = $state(0);
  let dir = $state(1);
  let scroller = $state<HTMLElement>();
  const current = $derived(groups[Math.min(page, groups.length - 1)]);

  function go(to: number) {
    if (to < 0 || to >= groups.length || to === page) return;
    dir = to > page ? 1 : -1;
    page = to;
    // The new backup starts at its top, not where the last one was scrolled to
    if (scroller) scroller.scrollTop = 0;
  }

  // Esc closes, ← / → switch backups. A question on top (ConfirmDialog) handles
  // its own keys.
  function onkeydown(e: KeyboardEvent) {
    if (confirmPending.current || e.defaultPrevented || e.ctrlKey || e.altKey || e.metaKey) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      onclose();
      return;
    }
    // Arrows move the cursor while typing in the path search
    if ((e.target as Element | null)?.closest?.('input[type="search"], input[type="text"], input:not([type]), select, textarea')) return;
    const step = e.key === 'ArrowLeft' ? -1 : e.key === 'ArrowRight' ? 1 : 0;
    if (!step) return;
    e.preventDefault();
    go(page + step);
  }


  const totalOps = $derived(plans.reduce((n, p) => n + p.ops, 0));
  // How long the check took (of this backup, or of "Check all") and what it compared
  const elapsedMs = $derived(overview ? overall.result?.elapsedMs : checks[plans[0]?.backup_id]?.elapsedMs);
  const compared = $derived(covered(plans));
  // Entries of all backups that administrator rights could make readable: one hint for all of them
  const needAdmin = $derived(plans.reduce((n, p) => n + p.blocked_admin, 0));
  const clean = (p: Plan) => !p.ops && !p.blocked_failures;

  // ▶ in the header: runs the analysed backup, or all analysed backups (in their order, as a
  // "Run all" batch, so the button there cancels them). Only those that still exist, have an
  // enabled entry and are not running or waiting already.
  const startable = $derived(
    groups
      .map((g) => g.id)
      .filter((id) => config.backups.find((b) => b.id === id)?.archives.some((a) => a.enabled))
      .filter((id) => id !== jobs.runningId && !jobs.queuedIds.includes(id)),
  );
  const runTitle = $derived(
    !startable.length
      ? t('preview.runNone')
      : overview
        ? t('preview.runAll', { count: startable.length })
        : t('preview.run'),
  );

  /** Starts the run and closes the analysis: what it shows is what the run will do. */
  function run() {
    // Everything is read before closing: once closed, the dialog's plans (and what is derived
    // from them) are gone, since `analysis.open` is null then
    const ids = [...startable];
    const batch = overview;
    if (!ids.length) return;
    runBackups(ids, batch);
    onclose();
  }

  async function refresh() {
    refreshing = true;
    try {
      await onrefresh();
    } finally {
      refreshing = false;
    }
  }
</script>

<svelte:window {onkeydown} />

{#snippet arrow(to: number, label: string, path: string)}
  <button class="nav" disabled={to < 0 || to >= groups.length} title={label} aria-label={label} onclick={() => go(to)}>
    <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true"><path fill="currentColor" d={path} /></svg>
  </button>
{/snippet}

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <!-- The focus moves into the dialog, so keys do not reach what lies below -->
  <div use:focusTrap tabindex="-1" class="modal" class:paged={overview} role="dialog" aria-modal="true" aria-label={t('preview.dialog')}>
    <header>
      <div class="heading">
        <div>
          <h2>
            {overview ? t('preview.allChanges') : t('preview.title')}
          </h2>
          <p class="muted">
            {#if overview}
              {t('preview.backups', { count: groups.length })}
            {:else}
              {plans[0]?.backup_name}
            {/if}
            · {t('preview.changes', { count: totalOps, n: formatCount(totalOps) })}
          </p>
          {#if elapsedMs != null}
            <p class="muted scanned">
              {t('preview.scanned', {
                time: formatElapsed(elapsedMs),
                files: t('preview.files', { count: compared.files, n: formatCount(compared.files) }),
                folders: t('preview.folders', { count: compared.dirs, n: formatCount(compared.dirs) }),
              })}
            </p>
          {/if}
        </div>
        <!-- Scan again, right next to the figures it renews -->
        <button
          class="rescan"
          class:spinning={refreshing}
          onclick={refresh}
          disabled={refreshing}
          aria-busy={refreshing}
          title={t('preview.rescan')}
          aria-label={t('preview.rescan')}
        >
          <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
            <path fill="currentColor" d={ICONS.rescan} />
          </svg>
        </button>
        <!-- Run what was analysed; the analysis closes -->
        <button class="play" onclick={run} disabled={!startable.length || refreshing} title={runTitle} aria-label={runTitle}>
          <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
            <path fill="currentColor" d={ICONS.play} />
          </svg>
        </button>
      </div>
      <button class="ghost icon" onclick={onclose} title={t('common.close')}>✕</button>
    </header>

    {#if overview}
      <!-- Page bar: back | backup and one dot per backup (centred) | n of total, forward -->
      <nav class="pager" aria-label={t('preview.pages')}>
        <div class="side">
          {@render arrow(page - 1, t('preview.previous'), ICONS.previous)}
        </div>
        <div class="where">
          <div class="label"><b>{current.name}</b></div>
          <div class="dots">
            {#each groups as g, i (g.id)}
              <!-- Red: a warning in one of its pipelines; green: nothing to do -->
              <button
                class="dot"
                class:active={i === page}
                class:danger={g.plans.some((p) => p.dangerous)}
                class:clean={g.plans.every(clean)}
                aria-current={i === page ? 'page' : undefined}
                aria-label={g.name}
                title={g.name}
                onclick={() => go(i)}
              ></button>
            {/each}
          </div>
        </div>
        <div class="side end">
          <span class="count">{t('preview.page', { n: page + 1, total: groups.length })}</span>
          {@render arrow(page + 1, t('preview.next'), ICONS.next)}
        </div>
      </nav>
    {/if}

    {#if needAdmin}
      <div class="admin"><AdminHint count={needAdmin} /></div>
    {/if}

    <!-- The pipelines of the shown backup, one below the other; the area scrolls -->
    <div class="pipelines" bind:this={scroller}>
      {#if current}
        {#key current.id}
          <div class="page" in:fly={{ x: overview ? 48 * dir : 0, duration: overview ? 220 : 0, easing: cubicOut }}>
            {#each current.plans as plan (plan.id)}
              <PipelineReport {plan} />
            {/each}
          </div>
        {/key}
      {/if}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.55);
    display: grid;
    place-items: center;
    padding: 24px;
    /* Above the top bar (20), so the whole window height is available; below the dialogs
       for schedules (25) and questions (30) */
    z-index: 22;
  }
  /* Only focused so keys stay inside; no frame around the whole dialog */
  .modal:focus {
    outline: none;
  }
  .modal {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 14px;
    width: min(1040px, 100%);
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    box-shadow: 0 20px 60px rgb(0 0 0 / 0.35);
    overflow: hidden;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    padding: 18px 20px 14px;
    border-bottom: 1px solid var(--border);
  }
  h2 {
    margin: 0;
    font-size: 19px;
  }
  header p {
    margin: 2px 0 0;
  }
  .scanned {
    font-size: 12.5px;
    font-variant-numeric: tabular-nums;
  }
  .heading {
    display: flex;
    align-items: center;
    gap: 14px;
    min-width: 0;
  }
  /* Round like the arrows of the page bar */
  .rescan,
  .play {
    flex: none;
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    min-height: 0;
    padding: 0;
    border-radius: 50%;
    color: var(--accent-text);
  }
  .rescan:hover:not(:disabled),
  .play:hover:not(:disabled) {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
  }
  /* The heading's gap is for the text; the two buttons sit closer together */
  .play {
    margin-left: -6px;
  }
  .rescan:hover:not(:disabled) svg {
    transform: rotate(90deg);
  }
  .rescan svg {
    transition: transform 0.25s ease;
  }
  /* Scanning: the arrow keeps turning; the button stays fully visible meanwhile */
  .rescan.spinning:disabled {
    opacity: 1;
    cursor: progress;
  }
  .rescan.spinning svg {
    animation: rescan-spin 0.9s linear infinite;
  }
  @keyframes rescan-spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .rescan svg,
    .rescan.spinning svg {
      transition: none;
      animation: none;
    }
  }
  .admin:has(:global(.hint)) {
    padding: 10px 20px;
    border-bottom: 1px solid var(--border);
  }
  /* With several pages the dialog keeps one height, so it does not jump while paging */
  .modal.paged {
    height: min(860px, calc(100vh - 48px));
  }
  /* Header and page bar stay put, the pipelines scroll between them */
  .pipelines {
    flex: 1 1 auto;
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    padding: 16px 20px;
    background: var(--bg);
  }
  .page {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  /* The sections keep their full height, so the area scrolls instead of squeezing them */
  .page > :global(*) {
    flex-shrink: 0;
  }
  /* Equal side columns keep the backup name and the dots in the middle, whatever the count needs */
  .pager {
    display: grid;
    grid-template-columns: minmax(max-content, 1fr) minmax(0, auto) minmax(max-content, 1fr);
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--surface-2);
  }
  .nav {
    flex: none;
    display: grid;
    place-items: center;
    width: 38px;
    height: 38px;
    padding: 0;
    border-radius: 50%;
    color: var(--accent-text);
  }
  .nav:hover:not(:disabled) {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
  }
  .nav:disabled {
    opacity: 0.3;
  }
  .side {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .side.end {
    justify-content: flex-end;
  }
  .count {
    font-size: 13px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .where {
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }
  .label {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
    color: var(--muted);
  }
  .label b {
    color: var(--text);
  }
  /* One dot per backup; the current one stretches into a pill */
  .dots {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    align-items: center;
    gap: 6px;
  }
  .dot {
    --c: var(--accent);
    width: 8px;
    height: 8px;
    min-height: 0;
    padding: 0;
    border: none;
    border-radius: 999px;
    background: color-mix(in srgb, var(--c) 35%, var(--border));
    transition:
      width 0.2s ease,
      background 0.2s ease;
  }
  .dot.clean {
    --c: var(--ok);
  }
  .dot.danger {
    --c: var(--danger);
  }
  .dot:hover:not(:disabled) {
    background: color-mix(in srgb, var(--c) 70%, var(--border));
  }
  .dot.active {
    width: 24px;
    background: var(--c);
  }
  @media (prefers-reduced-motion: reduce) {
    .dot {
      transition: none;
    }
  }
</style>
