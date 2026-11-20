<script lang="ts">
  // Every analysis and every run of the backups, newest first (src-tauri/src/commands/activity.rs):
  // when, what (backup run or analysis), which backup with its folders, started by hand or by its
  // schedule, and how it ended with what it changed. A click opens what it changed file by file
  // (ActivityDialog). The list follows new activities by itself while it is shown.
  import { onMount, untrack } from 'svelte';
  import { backend } from '../../api/backend.ts';
  import type { Activity, Id } from '../../api/types.ts';
  import { t } from '../../i18n/index.svelte.ts';
  import { config } from '../../stores/config.svelte.ts';
  import { confirm } from '../../stores/confirm.svelte.ts';
  import { showError } from '../../stores/notice.svelte.ts';
  import { formatBytes, formatCount } from '../../utils/format.ts';
  import { ICONS } from '../../utils/icons.ts';
  import { loadPreference, savePreference } from '../../utils/storage.ts';
  import ActivityDialog from './ActivityDialog.svelte';
  import { entryName, outcomeOf, totals, whenParts } from './activity.ts';

  type KindFilter = 'all' | Activity['kind'];
  const KINDS: KindFilter[] = ['all', 'sync', 'check'];
  /** Activities per page ("Load older" fetches the next). */
  const PAGE = 50;
  /** While the page is shown, new activities and running ones are fetched this often. */
  const REFRESH_MS = 3000;

  const KIND_KEY = 'activityKind';
  let kind = $state<KindFilter>(
    loadPreference(KIND_KEY, 'all', (v): v is KindFilter => KINDS.includes(v as KindFilter)),
  );
  // Only the activities of this backup; '' = all
  let backupId = $state<Id | ''>('');

  let items = $state<Activity[]>([]);
  let loaded = $state(false);
  let loading = $state(false);
  // The last fetch returned a full page: there may be older ones
  let more = $state(false);
  let error = $state('');
  let open = $state<Activity | null>(null);

  // Responses can overtake each other (filter changed while loading): only the latest counts
  let requests = 0;

  /**
   * Fetches the newest activities again, as many as are shown (at least a page), so a refresh
   * keeps the pages loaded with "Load older".
   */
  async function reload() {
    const seq = ++requests;
    const limit = Math.max(PAGE, items.length);
    try {
      const list = await backend.activities({ backup_id: backupId || null, kind: kind === 'all' ? null : kind, limit });
      if (seq !== requests) return;
      items = list;
      more = list.length === limit;
      error = '';
    } catch (e) {
      if (seq === requests) error = (e as Error).message;
    } finally {
      if (seq === requests) loaded = true;
    }
  }

  async function loadOlder() {
    const last = items.at(-1);
    if (!last || loading) return;
    loading = true;
    const seq = ++requests;
    try {
      const list = await backend.activities({
        backup_id: backupId || null,
        kind: kind === 'all' ? null : kind,
        before: last.id,
        limit: PAGE,
      });
      if (seq !== requests) return;
      items = [...items, ...list];
      more = list.length === PAGE;
    } catch (e) {
      if (seq === requests) error = (e as Error).message;
    } finally {
      loading = false;
    }
  }

  let clearing = $state(false);

  /** Deletes every analysis and run (a sync still running stays), after asking. */
  async function clearHistory() {
    if (!(await confirm(t('activity.clearHistoryConfirm'), t('activity.clearHistoryOk')))) return;
    clearing = true;
    try {
      await backend.clearActivities();
      items = [];
      await reload();
    } catch (e) {
      showError(e);
    } finally {
      clearing = false;
    }
  }

  function setKind(k: KindFilter) {
    kind = k;
    savePreference(KIND_KEY, k);
  }

  // A new filter starts from the first page
  $effect(() => {
    void kind;
    void backupId;
    untrack(() => {
      items = [];
      loaded = false;
      reload();
    });
  });

  onMount(() => {
    const timer = setInterval(() => {
      if (!open && !loading) reload();
    }, REFRESH_MS);
    return () => clearInterval(timer);
  });

  // The backups to filter by, by name; a deleted backup takes its activities with it
  const backups = $derived([...config.backups].sort((a, b) => a.name.localeCompare(b.name)));
  const colorOf = (id: Id) => config.backups.find((b) => b.id === id)?.color;

  /** The figures of an activity in words: what it changed (or would change), and how much. */
  function changesText(a: Activity): string {
    const s = totals(a);
    const parts = [
      s.copy && t('activity.changes.new', { n: formatCount(s.copy) }),
      s.update && t('activity.changes.updated', { n: formatCount(s.update) }),
      s.move && t('activity.changes.moved', { n: formatCount(s.move) }),
      s.delete && t('activity.changes.deleted', { n: formatCount(s.delete) }),
    ].filter((p): p is string => !!p);
    if (!parts.length) return a.finished_at ? t('activity.changes.none') : '';
    const bytes = a.kind === 'sync' ? s.bytesDone : s.bytesTotal;
    if (bytes) parts.push(t(a.kind === 'sync' ? 'activity.changes.transferred' : 'activity.changes.toTransfer', { size: formatBytes(bytes) }));
    return parts.join(' · ');
  }

  function problemsText(a: Activity): string {
    const s = totals(a);
    if (a.kind === 'sync' && s.errors) return t('activity.errors', { count: s.errors, n: formatCount(s.errors) });
    if (a.kind === 'check' && s.blocked) return t('activity.blocked', { count: s.blocked, n: formatCount(s.blocked) });
    return '';
  }
</script>

<div class="bar">
  <div class="segmented" role="group" aria-label={t('activity.kindFilter')}>
    {#each KINDS as k (k)}
      <button class:active={kind === k} aria-pressed={kind === k} onclick={() => setKind(k)}>{t(`activity.filters.${k}`)}</button>
    {/each}
  </div>
  {#if backups.length > 1}
    <select bind:value={backupId} aria-label={t('activity.backupFilter')}>
      <option value="">{t('activity.allBackups')}</option>
      {#each backups as b (b.id)}
        <option value={b.id}>{b.name}</option>
      {/each}
    </select>
  {/if}
  <button class="round refresh" onclick={reload} title={t('activity.refresh')} aria-label={t('activity.refresh')}>
    <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d={ICONS.rescan} /></svg>
  </button>
  <!-- Deletes the whole history (not only what the filter shows), after asking -->
  <button
    class="round clear"
    onclick={clearHistory}
    disabled={clearing || (loaded && !items.length && kind === 'all' && !backupId)}
    aria-busy={clearing}
    title={t('activity.clearHistory')}
    aria-label={t('activity.clearHistory')}
  >
    <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d={ICONS.trash} /></svg>
  </button>
</div>

{#if error}
  <p class="failed" role="alert">{error}</p>
{/if}

{#if !loaded}
  <p class="muted">{t('preview.tree.loading')}</p>
{:else if !items.length}
  <p class="empty muted">{kind === 'all' && !backupId ? t('activity.empty') : t('activity.emptyFiltered')}</p>
{:else}
  <ol class="list">
    {#each items as a (a.id)}
      {@const when = whenParts(a.started_at)}
      {@const outcome = outcomeOf(a)}
      {@const problems = problemsText(a)}
      <li>
        <button class="row {outcome}" onclick={() => (open = a)} aria-label={t('activity.openLabel', { name: a.backup_name, when: `${when.date} ${when.time}` })}>
          <span class="when">
            <span class="date">{when.date}</span>
            <span class="time">{when.time}</span>
          </span>

          <span class="kind {a.kind}">
            <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
              <path fill="currentColor" d={a.kind === 'sync' ? ICONS.play : ICONS.check} />
            </svg>
            {t(`activity.kinds.${a.kind}`)}
          </span>

          <span class="what">
            <span class="name">
              <span class="dot" style:background={colorOf(a.backup_id) ?? 'var(--amber)'}></span>
              {a.backup_name}
            </span>
            <span class="entries">
              {#each a.entries.slice(0, 3) as e (e.id)}
                <span class="entry mono" title="{e.source} → {e.dest}">
                  {entryName(e.source)} <span class="arrow">→</span> {entryName(e.dest)}
                </span>
              {/each}
              {#if a.entries.length > 3}
                <span class="muted small">{t('activity.moreEntries', { count: a.entries.length - 3 })}</span>
              {/if}
            </span>
          </span>

          <span class="origin">{t(`activity.origins.${a.origin}`)}</span>

          <span class="result">
            <span class="outcome {outcome}">
              {#if outcome === 'running'}<span class="spinner" aria-hidden="true"></span>{/if}
              {t(`activity.outcomes.${outcome}`)}
            </span>
            <span class="changes">{changesText(a)}</span>
            {#if problems}<span class="problems">{problems}</span>{/if}
          </span>
        </button>
      </li>
    {/each}
  </ol>
  {#if more}
    <div class="more">
      <button class="ghost" onclick={loadOlder} disabled={loading}>{t('activity.loadMore')}</button>
    </div>
  {/if}
{/if}

{#if open}
  <ActivityDialog activity={open} onclose={() => (open = null)} />
{/if}

<style>
  .bar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin-bottom: 16px;
  }
  .segmented {
    background: var(--surface);
  }
  .bar select {
    min-width: 200px;
    max-width: 320px;
  }
  .refresh {
    margin-left: auto;
    color: var(--accent-text);
  }
  /* Outlined like the refresh button; red on hover, like other deletions */
  .clear {
    color: var(--muted);
  }
  .clear:hover:not(:disabled) {
    background: var(--danger-soft);
    border-color: var(--danger);
    color: var(--danger);
  }
  .failed {
    color: var(--danger);
  }
  .empty {
    padding: 32px 0;
    text-align: center;
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 6px;
  }
  /* One activity: when · kind · backup with its folders · origin · result. --c: colour of its result */
  .row {
    --c: var(--border);
    width: 100%;
    display: grid;
    grid-template-columns: 112px 112px minmax(0, 1fr) 110px minmax(180px, 300px);
    align-items: center;
    gap: 14px;
    padding: 10px 14px;
    text-align: left;
    background: var(--surface);
    border: 1px solid var(--border);
    border-left: 3px solid var(--c);
    border-radius: 10px;
    font-weight: 400;
    min-height: 0;
  }
  .row:hover:not(:disabled) {
    border-color: color-mix(in srgb, var(--c) 45%, var(--border));
    border-left-color: var(--c);
    background: var(--surface-2);
  }
  .row.ok {
    --c: var(--ok);
  }
  .row.errors,
  .row.cancelled,
  .row.interrupted {
    --c: var(--warn);
  }
  .row.failed {
    --c: var(--danger);
  }
  .row.running {
    --c: var(--accent);
  }
  .when {
    display: flex;
    flex-direction: column;
    font-variant-numeric: tabular-nums;
  }
  .date {
    font-size: 13px;
    color: var(--text);
  }
  .time {
    font-size: 12px;
    color: var(--muted);
  }
  .kind {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    justify-self: start;
    padding: 3px 10px 3px 8px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    background: var(--accent-soft);
    color: var(--accent-text);
  }
  .kind.sync {
    background: color-mix(in srgb, var(--amber) 18%, var(--surface));
    color: var(--amber);
  }
  .what {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .name {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .entries {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .entry {
    font-size: 12px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .arrow {
    opacity: 0.7;
  }
  .small {
    font-size: 12px;
  }
  .origin {
    font-size: 12px;
    color: var(--muted);
  }
  .result {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    font-size: 12px;
  }
  .outcome {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-weight: 600;
    font-size: 13px;
    color: var(--c);
  }
  .changes {
    color: var(--muted);
  }
  .problems {
    color: var(--danger);
  }
  .spinner {
    width: 11px;
    height: 11px;
    border: 2px solid var(--accent-soft);
    border-top-color: var(--accent-text);
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
  .more {
    display: flex;
    justify-content: center;
    margin-top: 14px;
  }

  /* Narrow window: when and kind on top, the result below the backup */
  @media (max-width: 860px) {
    .row {
      grid-template-columns: auto minmax(0, 1fr);
      gap: 8px 14px;
    }
    .when {
      flex-direction: row;
      gap: 8px;
      align-items: baseline;
    }
    .kind {
      justify-self: end;
    }
    .what,
    .result {
      grid-column: 1 / -1;
    }
    .origin {
      grid-column: 1 / -1;
      grid-row: 3;
    }
  }
</style>
