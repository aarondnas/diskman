<script lang="ts">
  // Bar above the backup list: "Schedule all", "Check all" and "Run all", the search and the sort order, the result
  // of "Check all", and the activities and settings on the right edge. Search and sort only from two backups on.
  import type { Id } from '../../api/types.ts';
  import CheckSummary from '../../components/CheckSummary.svelte';
  import { t } from '../../i18n/index.svelte.ts';
  import { checkAllAndShow, showAnalysis } from '../../stores/analysis.svelte.ts';
  import { cancelCheckAll, overall } from '../../stores/checks.svelte.ts';
  import { confirm } from '../../stores/confirm.svelte.ts';
  import { cancelBatch, jobs, runBackups } from '../../stores/jobs.svelte.ts';
  import { navigate } from '../../stores/navigation.svelte.ts';
  import { ICONS } from '../../utils/icons.ts';
  import { clearQuery, directionLabel, setSort, toggleSortDirection, view } from './listView.svelte.ts';

  // count: number of backups; runnable: ids of the shown backups that can run now, in list order;
  // shown: number of shown backups; onscheduleall(): opens one schedule for several backups
  interface Props {
    count: number;
    runnable: Id[];
    shown: number;
    onscheduleall: () => void;
  }
  let { count, runnable, shown, onscheduleall }: Props = $props();

  const dirLabel = $derived(directionLabel(view.sort));
  // Backups of "Run all" that still run or wait
  const batch = $derived(jobs.batchIds);

  /** Queues all shown backups one after another, after asking. */
  async function runAll() {
    if (!runnable.length) return;
    if (!(await confirm(t('app.runAllConfirm', { count: runnable.length }), t('app.runAllOk'), 'run'))) return;
    // Taken after the question: a backup may have started or finished in the meantime
    runBackups(runnable, true);
  }
</script>

<!-- A running "all" button: a spinner, replaced by a stop symbol while the pointer is on it -->
{#snippet busy()}
  <span class="spinner" aria-hidden="true"></span>
  <svg class="stop" viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d={ICONS.stop} /></svg>
{/snippet}

<div class="toolbar">
  <div class="actions">
    {#if count}
      <button
        class="amber round all"
        onclick={onscheduleall}
        disabled={!shown}
        title={shown ? t('app.scheduleAll', { count: shown }) : t('app.scheduleAllNone')}
        aria-label={t('app.scheduleAllLabel')}
      >
        <!-- "All": the clock of a card in front of the faded rim of a second one, like a stack -->
        <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
          <path
            class="back clock"
            d="M8.3 20.9A8 8 0 0 1 3.5 10.7"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
          />
          <g class="front clock">
            <path fill="currentColor" transform="translate(4.4 0.8) scale(0.8)" d={ICONS.clock} />
          </g>
        </svg>
      </button>
      <!-- While "Check all" runs, the button cancels it -->
      <button
        class="amber round all"
        class:cancel={overall.running}
        onclick={() => (overall.running ? cancelCheckAll() : checkAllAndShow(runnable))}
        disabled={!overall.running && !runnable.length}
        aria-busy={overall.running}
        title={overall.running
          ? t('app.cancelCheckAll')
          : runnable.length
            ? t('app.checkAll', { count: runnable.length })
            : t('app.checkAllNone')}
        aria-label={t(overall.running ? 'app.cancelCheckAllLabel' : 'app.checkAllLabel')}
      >
        {#if overall.running}
          {@render busy()}
        {:else}
        <!-- "All": the check symbol of a card in front of a faded second one, like a stack -->
        <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
          <path
            class="back check"
            d="M3 6.5v12.6a1.5 1.5 0 0 0 1.5 1.5H19"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
          />
          <!-- Group, so the hover movement does not replace the path's own transform -->
          <g class="front check">
            <path fill="currentColor" transform="translate(4.4 0.8) scale(0.8)" d={ICONS.check} />
          </g>
        </svg>
        {/if}
      </button>
      <!-- While backups of "Run all" still run or wait, the button cancels them -->
      <button
        class="amber round all"
        class:cancel={batch.length > 0}
        onclick={() => (batch.length ? cancelBatch() : runAll())}
        disabled={!batch.length && !runnable.length}
        aria-busy={batch.length > 0}
        title={batch.length
          ? t('app.cancelRunAll', { count: batch.length })
          : runnable.length
            ? t('app.runAll', { count: runnable.length })
            : t('app.runAllNone')}
        aria-label={t(batch.length ? 'app.cancelRunAllLabel' : 'app.runAllLabel')}
      >
        {#if batch.length}
          {@render busy()}
        {:else}
        <!-- "All": the play triangle in front of a faded second one, with a small gap between them -->
        <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
          <defs>
            <mask id="runall-gap">
              <rect width="24" height="24" fill="white" />
              <path d="M10 5v14l11-7z" fill="black" stroke="black" stroke-width="3" stroke-linejoin="round" />
            </mask>
          </defs>
          <!-- The mask moves along with the back triangle, so the gap only widens while it moves -->
          <path class="back play" d="M5 5v14l11-7z" fill="currentColor" mask="url(#runall-gap)" />
          <path class="front play" d="M10 5v14l11-7z" fill="currentColor" />
        </svg>
        {/if}
      </button>
    {/if}

    {#if count > 1}
      <!-- Search right after the two buttons, with some room in between -->
      <label class="search">
        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="currentColor" d={ICONS.search} /></svg>
        <input
          type="search"
          placeholder={t('app.searchPlaceholder')}
          aria-label={t('app.searchLabel')}
          bind:value={view.query}
          onkeydown={(e) => e.key === 'Escape' && clearQuery()}
        />
      </label>
      <!-- Sorting right next to the search -->
      <div class="segmented">
        <button class:active={view.sort.by === 'name'} aria-pressed={view.sort.by === 'name'} onclick={() => setSort({ by: 'name' })}>
          {t('app.sortName')}
        </button>
        <button
          class:active={view.sort.by === 'created'}
          aria-pressed={view.sort.by === 'created'}
          onclick={() => setSort({ by: 'created' })}
        >
          {t('app.sortCreated')}
        </button>
        <button class="dir" onclick={toggleSortDirection} title={dirLabel} aria-label={t('app.sortOrder', { dir: dirLabel })}>
          <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true" class:flip={view.sort.dir === 'desc'}>
            <path fill="currentColor" d={ICONS.sortUp} />
          </svg>
        </button>
      </div>
    {/if}

    <!-- Totals of "Check all"; opens every change of every backup -->
    {#if overall.result && !overall.running}
      <CheckSummary
        summary={overall.result.summary}
        label={t('app.showAllChanges')}
        onclick={() => overall.result && showAnalysis(null, overall.result.plans)}
      />
    {/if}
  </div>

  <!-- Right edge: the activities (every analysis and run) and the settings -->
  <div class="end">
    <button class="round activity" title={t('activity.open')} aria-label={t('activity.open')} onclick={() => navigate('activity')}>
      <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d={ICONS.history} /></svg>
    </button>
    <button class="round settings" title={t('nav.settings')} aria-label={t('nav.settings')} onclick={() => navigate('settings')}>
      <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d={ICONS.settings} /></svg>
    </button>
  </div>
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin-bottom: 16px;
  }
  /* Check and run all, the search and the sorting on the left; min-width: 0 lets the group get
     narrower than its content, so the search inside can shrink */
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  /* The buttons and the result of "Check all" keep their width; only the search gives way */
  .actions > :global(.capsule),
  .all {
    flex: none;
  }

  /* "Check all" and "Run all" are amber so they stand out from the round buttons on the cards.
     Hover: the stack fans out – the front symbol springs forward, the one behind steps back and
     comes out clearer */
  .all .front,
  .all .back {
    transition:
      transform 0.32s cubic-bezier(0.34, 1.56, 0.64, 1),
      opacity 0.2s ease;
  }
  .all .back {
    opacity: 0.5;
  }
  .all:hover:not(:disabled) .back {
    opacity: 0.8;
  }
  .all:hover:not(:disabled) .front.check,
  .all:hover:not(:disabled) .front.clock {
    transform: translate(1.5px, -1.5px);
  }
  .all:hover:not(:disabled) .back.check,
  .all:hover:not(:disabled) .back.clock {
    transform: translate(-1px, 1px);
  }
  .all:hover:not(:disabled) .front.play {
    transform: translateX(2px);
  }
  .all:hover:not(:disabled) .back.play {
    transform: translateX(-1.5px);
  }

  /* A fixed width (not just a flex-basis): its group only becomes as wide as its content, where a
     basis does not count. When the row gets tight it shrinks down to min-width. */
  .search {
    position: relative;
    flex: 0 1 auto;
    width: 380px;
    min-width: 180px;
    display: flex;
    margin-left: 28px;
  }
  .search svg {
    position: absolute;
    left: 10px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--muted);
    pointer-events: none;
  }
  .search input {
    width: 100%;
    padding-left: 32px;
  }

  .segmented {
    background: var(--surface);
  }
  .dir {
    display: grid;
    place-items: center;
    width: 34px;
    padding: 0;
  }
  .dir svg {
    transition: transform 0.15s ease;
  }
  .dir svg.flip {
    transform: rotate(180deg);
  }

  .end {
    display: flex;
    gap: 8px;
    margin-left: auto;
  }
  /* Outlined like the round buttons on the cards, filled grey only on hover; the gear turns a
     little, the clock of the activities turns back */
  .settings,
  .activity {
    color: var(--accent-text);
  }
  .settings:hover:not(:disabled),
  .activity:hover:not(:disabled) {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
    filter: brightness(1.12);
  }
  .settings svg,
  .activity svg {
    transition: transform 0.32s cubic-bezier(0.34, 1.56, 0.64, 1);
  }
  .settings:hover:not(:disabled) svg {
    transform: rotate(60deg);
  }
  .activity:hover:not(:disabled) svg {
    transform: rotate(-45deg);
  }

  /* Running and cancellable: a spinner in the amber button; the pointer on it shows a stop symbol
     in red, like the cancel buttons on the cards */
  .spinner {
    width: 16px;
    height: 16px;
    border: 2px solid transparent;
    border-top-color: var(--on-amber);
    border-right-color: var(--on-amber);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .all .stop {
    display: none;
  }
  .all.cancel:hover:not(:disabled) .stop,
  .all.cancel:focus-visible .stop {
    display: block;
  }
  .all.cancel:hover:not(:disabled) .spinner,
  .all.cancel:focus-visible .spinner {
    display: none;
  }
  .all.cancel:hover:not(:disabled) {
    background: var(--danger-soft);
    border-color: var(--danger);
    color: var(--danger);
  }

  @media (prefers-reduced-motion: reduce) {
    .all .front,
    .all .back,
    .settings svg,
    .activity svg {
      transition: none;
    }
    .spinner {
      animation-duration: 2.4s;
    }
  }
</style>
