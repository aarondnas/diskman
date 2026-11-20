<script lang="ts">
  import type { Plan, PlanSummary, PlanWarning, RowFilter, ScanMode } from '../../api/types.ts';
  import { formatBytes, formatCount, formatElapsed } from '../../utils/format.ts';
  import { t, type TextKey } from '../../i18n/index.svelte.ts';
  import ChangeTree from './ChangeTree.svelte';

  // One pipeline (source → archive) in the preview: route, warnings, figures and its file changes,
  // grouped by folder (ChangeTree). What cannot be transferred (plan.blocked) is a kind of its own,
  // with its own card and the reason of each entry.
  // AnalysisDialog.svelte shows one of these per entry, one below the other; ActivityDialog.svelte
  // a stored one (`live` false: no check runs for it any more).
  let { plan, live = true }: { plan: Plan; live?: boolean } = $props();

  function warningText(w: PlanWarning): string {
    switch (w.kind) {
      case 'archive_unreadable':
        return t('preview.warnings.archive_unreadable', { count: w.count, n: formatCount(w.count) });
      case 'many_deletes':
        return t('preview.warnings.many_deletes', { n: formatCount(w.count), total: formatCount(w.total) });
      default:
        return t(`preview.warnings.${w.kind}`);
    }
  }

  // The figure cards double as the filter: a click shows only that kind, a second click all again.
  // "Unchanged" has no entries in the list, so it is no button. Kinds that do not occur get no card.
  interface Card {
    cls: string;
    filter: RowFilter;
    value: (s: PlanSummary) => string;
    label: TextKey;
    sub: (s: PlanSummary) => string;
  }
  const CARDS: Card[] = [
    {
      cls: 'blk',
      filter: 'blocked',
      value: (s) => formatCount(s.blocked),
      label: 'preview.stats.blocked',
      sub: () => (needAdmin ? t('preview.stats.needAdmin', { count: needAdmin }) : formatBytes(blockedBytes)),
    },
    { cls: 'add', filter: 'copy', value: (s) => formatCount(s.copy_files), label: 'preview.stats.new', sub: (s) => formatBytes(s.copy_bytes) },
    { cls: 'upd', filter: 'update', value: (s) => formatCount(s.update_files), label: 'preview.stats.updated', sub: (s) => formatBytes(s.update_bytes) },
    {
      cls: 'mov',
      filter: 'move',
      value: (s) => formatCount(s.move_files),
      label: 'preview.stats.moved',
      sub: (s) => t('preview.stats.notCopied', { size: formatBytes(s.move_bytes) }),
    },
    { cls: 'del', filter: 'delete', value: (s) => formatCount(s.delete_files), label: 'preview.stats.deleted', sub: (s) => formatBytes(s.delete_bytes) },
    {
      cls: 'dir',
      filter: 'dirs',
      value: (s) => `+${s.dirs_created} / −${s.dirs_deleted}`,
      label: 'preview.stats.folders',
      sub: (s) => t('preview.stats.dirsMoved', { count: s.dirs_moved }),
    },
  ];

  let filter = $state<RowFilter>('all');
  let search = $state('');
  const toggle = (id: RowFilter) => (filter = filter === id ? 'all' : id);

  const needAdmin = $derived(plan.blocked_admin);
  const blockedBytes = $derived(plan.blocked_bytes);
  const counts = $derived(plan.counts);
  const shownCards = $derived(CARDS.filter((c) => counts[c.filter] > 0));

  let tree = $state<ChangeTree>();
  let canCollapse = $state(false);

  /** Time of a scan and how it came about. */
  function scanText(ms: number, mode: ScanMode) {
    if (mode === 'shared') return t('preview.scanShared');
    const time = formatElapsed(ms);
    return mode === 'incremental' ? t('preview.scanIncremental', { time }) : time;
  }
  const times = $derived.by(() => {
    const src = scanText(plan.src_scan_ms, plan.src_scan);
    const diff = formatElapsed(plan.diff_ms);
    const scans = plan.dst_scan
      ? t('preview.scanTimes', { src, dst: scanText(plan.dst_scan_ms, plan.dst_scan), diff })
      : t('preview.scanTimesNew', { src, diff });
    return plan.probe_ms == null ? scans : `${scans} · ${t('preview.probeTime', { time: formatElapsed(plan.probe_ms) })}`;
  });
</script>

<section class="pipeline" class:critical={plan.dangerous}>
  <header>
    <div class="route">
      <span class="mono" title={plan.source}>{plan.source}</span>
      <span class="arrow">→</span>
      <span class="mono" title={plan.dest}>{plan.dest}</span>
    </div>
    <span class="badge {plan.mode}">{t(plan.mode === 'mirror' ? 'common.mirror' : 'common.additive')}</span>
    <span class="muted small">{times}</span>
  </header>

  {#if live && !plan.probed && plan.summary.copy_files + plan.summary.update_files > 0}
    <div class="probing muted small"><span class="spinner" aria-hidden="true"></span>{t('preview.probing')}</div>
  {/if}

  {#each plan.warnings as w}
    <div class="warning" class:critical={plan.dangerous}>⚠ {warningText(w)}</div>
  {/each}

  <!-- Only kinds that occur get a card -->
  {#if shownCards.length || plan.summary.unchanged}
    <div class="stats">
      {#each shownCards as card (card.cls)}
        <button
          class="stat {card.cls}"
          class:active={filter === card.filter}
          aria-pressed={filter === card.filter}
          onclick={() => toggle(card.filter)}
        >
          <b>{card.value(plan.summary)}</b>{t(card.label)}<small>{card.sub(plan.summary)}</small>
        </button>
      {/each}
      {#if plan.summary.unchanged}
        <div class="stat"><b>{formatCount(plan.summary.unchanged)}</b>{t('preview.stats.unchanged')}<small>&nbsp;</small></div>
      {/if}
    </div>
  {/if}

  {#if counts.all}
    <div class="toolbar">
      <!-- The active card filter, removable here as well -->
      {#if filter !== 'all'}
        <button class="chip" onclick={() => (filter = 'all')} title={t('preview.filters.all')}>
          {t(`preview.filters.${filter}`)} <span>{counts[filter]}</span> <span aria-hidden="true">✕</span>
        </button>
      {/if}
      {#if canCollapse}
        <button class="ghost collapse" onclick={() => tree?.collapseAll()}>{t('preview.tree.collapseAll')}</button>
      {/if}
      <input type="search" placeholder={t('preview.filterPaths')} bind:value={search} />
    </div>
    <ChangeTree bind:this={tree} bind:canCollapse {plan} {filter} {search} />
  {:else}
    <div class="uptodate">{t('preview.upToDate')}</div>
  {/if}
</section>

<style>
  /* The second step of a check: opening the files to copy */
  .probing {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 10px;
  }
  .spinner {
    width: 12px;
    height: 12px;
    border: 2px solid var(--accent-soft);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation-duration: 2.4s;
    }
  }
  .pipeline {
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
    overflow: hidden;
  }
  .pipeline.critical {
    border-color: var(--danger);
  }
  header {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
    padding: 10px 14px;
    background: var(--surface-2);
    border-bottom: 1px solid var(--border-soft);
    font-size: 13px;
  }
  .route {
    display: flex;
    gap: 8px;
    align-items: center;
    flex: 1 1 320px;
    min-width: 0;
    font-weight: 600;
  }
  .route .mono {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent-text);
    font-weight: 600;
  }
  .badge.additive {
    background: var(--ok-soft);
    color: var(--ok);
  }
  .small {
    font-size: 12px;
  }
  .warning {
    margin: 10px 14px 0;
    padding: 8px 12px;
    border-radius: var(--radius);
    background: var(--warn-soft);
    color: var(--warn);
    font-size: 13px;
  }
  .warning.critical {
    background: var(--danger-soft);
    color: var(--danger);
    font-weight: 600;
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
    gap: 6px;
    padding: 10px 14px;
  }
  /* --c: colour of the card's kind, on the left edge and as the tint when hovered or active */
  .stat {
    --c: var(--border);
    background: var(--surface);
    border: 1px solid var(--border);
    border-left: 3px solid var(--c);
    border-radius: var(--radius);
    padding: 6px 10px;
    min-height: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    text-align: left;
    font-size: 12px;
    color: var(--muted);
  }
  .stat b {
    font-size: 16px;
    color: var(--text);
  }
  .stat small {
    font-size: 11px;
  }
  .stat.add {
    --c: var(--ok);
  }
  .stat.upd {
    --c: var(--warn);
  }
  .stat.mov {
    --c: var(--accent);
  }
  .stat.del {
    --c: var(--danger);
  }
  .stat.dir {
    --c: var(--muted);
  }
  .stat.blk {
    --c: var(--danger);
  }
  button.stat:hover:not(:disabled) {
    border-color: color-mix(in srgb, var(--c) 55%, var(--border));
    border-left-color: var(--c);
    background: color-mix(in srgb, var(--c) 8%, var(--surface));
  }
  button.stat.active,
  button.stat.active:hover:not(:disabled) {
    border-color: var(--c);
    background: color-mix(in srgb, var(--c) 16%, var(--surface));
    box-shadow: 0 0 0 1px var(--c);
  }
  .toolbar {
    display: flex;
    gap: 12px;
    justify-content: flex-end;
    align-items: center;
    padding: 0 14px 8px;
    flex-wrap: wrap;
  }
  .chip {
    margin-right: auto;
    font-size: 12px;
    padding: 4px 10px;
    min-height: 28px;
    border-radius: 999px;
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }
  .chip span {
    margin-left: 2px;
    font-weight: 400;
  }
  .collapse {
    min-height: 30px;
    padding: 4px 10px;
    font-size: 12px;
    color: var(--muted);
  }
  .toolbar input {
    width: 220px;
    min-height: 30px;
  }
  .uptodate {
    padding: 18px 14px;
    text-align: center;
    color: var(--ok);
    font-weight: 600;
    border-top: 1px solid var(--border-soft);
  }
</style>
