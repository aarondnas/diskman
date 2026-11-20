<script lang="ts">
  // A running sync in the large: its figures (time, operations, data, speed, time left, errors),
  // the file being worked on, and per entry (source → archive) its progress and a tree of what it
  // has done so far, which grows while it runs (commands/sync.rs, run_tree). Once the run ends,
  // the details of the finished run take its place (stores/runView.svelte.ts, ActivityDialog).
  import { untrack } from 'svelte';
  import type { EntryState, JobProgress, LiveEntry } from '../../api/types.ts';
  import { backend } from '../../api/backend.ts';
  import { t } from '../../i18n/index.svelte.ts';
  import { pending as confirmPending } from '../../stores/confirm.svelte.ts';
  import { config } from '../../stores/config.svelte.ts';
  import { cancelBackups, jobs } from '../../stores/jobs.svelte.ts';
  import { finishRun } from '../../stores/runView.svelte.ts';
  import { focusTrap } from '../../utils/focusTrap.ts';
  import { formatBytes, formatCount, formatElapsed } from '../../utils/format.ts';
  import { ICONS } from '../../utils/icons.ts';
  import ChangeTree from '../analysis/ChangeTree.svelte';

  let { backupId, onclose }: { backupId: string; onclose: () => void } = $props();

  // Still running: otherwise the details of the finished run come next
  const live = $derived(!!jobs.job?.running && jobs.job.backup_id === backupId);
  $effect(() => {
    if (!live) untrack(finishRun);
  });
  // The figures of this run; once it ended they stay as they were (the job may already be the
  // next backup's) until its details are loaded
  let last = $state.raw<JobProgress | null>(null);
  $effect(() => {
    if (live) last = jobs.job;
  });
  const job = $derived(live ? jobs.job : last);

  const name = $derived(config.backups.find((b) => b.id === backupId)?.name ?? job?.label ?? '');
  const entries = $derived(live ? (job?.entries ?? []) : []);
  const scanning = $derived(live && !entries.length);

  const since = (iso: string | null | undefined) => (iso ? Math.max(0, jobs.now - Date.parse(iso)) : 0);
  const elapsed = $derived(since(job?.started_at));
  // Speed and time left count from the start of the operations, without the scans
  const working = $derived(since(job?.ops_started_at));
  // Share done: by bytes when there is something to copy, otherwise by operations
  const share = $derived.by(() => {
    if (!job) return 0;
    if (job.total_bytes) return job.done_bytes / job.total_bytes;
    return job.total_ops ? job.done_ops / job.total_ops : 0;
  });
  const percent = $derived(Math.floor(share * 100));
  const speed = $derived(working >= 1000 && job ? job.done_bytes / (working / 1000) : 0);
  // Only once something is done and a few seconds passed: before that it is a guess
  const remaining = $derived(share > 0 && share < 1 && working >= 3000 ? (working * (1 - share)) / share : null);

  const STATES: Record<EntryState, string> = {
    waiting: 'waiting',
    running: 'running',
    done: 'done',
    cancelled: 'warn',
    skipped: 'warn',
  };
  const entryShare = (e: LiveEntry) =>
    e.total_bytes ? e.done_bytes / e.total_bytes : e.total_ops ? e.done_ops / e.total_ops : 1;

  function onkeydown(e: KeyboardEvent) {
    if (confirmPending.current || e.defaultPrevented) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      onclose();
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div use:focusTrap tabindex="-1" class="modal" role="dialog" aria-modal="true" aria-labelledby="run-title">
    <header>
      <span class="pulse" class:still={!live} aria-hidden="true"></span>
      <div class="heading">
        <h2 id="run-title">{live ? t('run.title') : t('run.finishing')}</h2>
        <p class="muted">{name}</p>
      </div>
      {#if live}
        <button class="stop" onclick={() => cancelBackups([backupId])} title={t('run.cancel')} aria-label={t('run.cancel')}>
          <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="currentColor" d={ICONS.stop} /></svg>
          {t('common.cancel')}
        </button>
      {/if}
      <button class="ghost icon" onclick={onclose} title={t('common.close')} aria-label={t('common.close')}>✕</button>
    </header>

    <!-- The figures of the card, in the large -->
    <div class="figures">
      <div class="figure">
        <span class="label">{t('run.elapsed')}</span>
        <b>{formatElapsed(elapsed, true)}</b>
      </div>
      <div class="figure">
        <span class="label">{t('run.ops')}</span>
        <b>{t('run.of', { done: formatCount(job?.done_ops ?? 0), total: formatCount(job?.total_ops ?? 0) })}</b>
      </div>
      <div class="figure">
        <span class="label">{t('run.data')}</span>
        <b>{t('run.of', { done: formatBytes(job?.done_bytes ?? 0), total: formatBytes(job?.total_bytes ?? 0) })}</b>
      </div>
      <div class="figure">
        <span class="label">{t('run.speed')}</span>
        <b>{speed ? t('run.perSecond', { size: formatBytes(speed) }) : '–'}</b>
      </div>
      <div class="figure">
        <span class="label">{t('run.remaining')}</span>
        <b>{remaining != null ? formatElapsed(remaining, true) : '–'}</b>
      </div>
      <div class="figure" class:bad={!!job?.error_count}>
        <span class="label">{t('run.errors')}</span>
        <b>{formatCount(job?.error_count ?? 0)}</b>
      </div>
    </div>

    <div class="overall">
      <div class="bar" class:indeterminate={scanning} role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={scanning ? undefined : percent}>
        <div style:width="{scanning ? 100 : percent}%"></div>
      </div>
      <span class="percent">{scanning ? '' : `${percent} %`}</span>
    </div>
    <p class="current">
      {#if scanning}
        <span class="muted">{t('run.scanning')}</span>
      {:else if live && job?.current}
        <span class="muted">{t('run.current')}:</span>
        <span class="mono" title={job.current}>{job.current}</span>
      {/if}
    </p>

    <div class="entries">
      {#each entries as e (e.plan_id)}
        <section>
          <header>
            <div class="route">
              <span class="mono" title={e.source}>{e.source}</span>
              <span class="arrow">→</span>
              <span class="mono" title={e.dest}>{e.dest}</span>
            </div>
            {#if e.state === 'running' || e.state === 'done'}
              <span class="small muted">
                {t('run.of', { done: formatCount(e.done_ops), total: formatCount(e.total_ops) })}
                {#if e.total_bytes}· {formatBytes(e.done_bytes)}{/if}
                {#if e.error_count}· <span class="bad">{t('activity.errors', { count: e.error_count, n: formatCount(e.error_count) })}</span>{/if}
              </span>
            {/if}
            <span class="state {STATES[e.state]}">{t(`run.states.${e.state}`)}</span>
          </header>
          {#if e.state === 'running' || e.state === 'done'}
            <div class="bar thin"><div style:width="{Math.floor(entryShare(e) * 100)}%"></div></div>
            <!-- What it has done so far; grows with every operation -->
            <ChangeTree
              plan={{ id: e.plan_id, probed: true, source: e.source, dest: e.dest }}
              filter="all"
              search=""
              levelOf={(dir) => backend.runTree(e.plan_id, dir)}
              version={e.done_ops}
              empty={t('run.nothingYet')}
            />
          {:else if e.state === 'skipped'}
            <p class="note warn">{t('run.skippedNote')}</p>
          {:else if e.state === 'waiting'}
            <p class="note muted">{t('run.waitingNote')}</p>
          {/if}
        </section>
      {/each}
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
    /* Like the analysis: above the top bar, below the questions (30) */
    z-index: 22;
  }
  .modal:focus {
    outline: none;
  }
  .modal {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 14px;
    width: min(1040px, 100%);
    height: min(860px, calc(100vh - 48px));
    display: flex;
    flex-direction: column;
    box-shadow: 0 20px 60px rgb(0 0 0 / 0.35);
    overflow: hidden;
  }
  .modal > header {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 18px 20px 14px;
    border-bottom: 1px solid var(--border);
  }
  .heading {
    flex: 1;
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: 19px;
  }
  .heading p {
    margin: 2px 0 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Pulsing while it runs, like the dot on the card */
  .pulse {
    flex: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--accent);
    animation: pulse 1.2s ease-in-out infinite;
  }
  .pulse.still {
    animation: none;
    background: var(--muted);
  }
  @keyframes pulse {
    50% {
      opacity: 0.3;
    }
  }
  .stop {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .stop:hover:not(:disabled) {
    background: var(--danger-soft);
    border-color: var(--danger);
    color: var(--danger);
  }

  .figures {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 10px;
    padding: 16px 20px 6px;
  }
  .figure {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface-2);
    min-width: 0;
  }
  .figure .label {
    font-size: 12px;
    color: var(--muted);
  }
  .figure b {
    font-size: 16px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .figure.bad b,
  .bad {
    color: var(--danger);
  }

  .overall {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 20px 0;
  }
  .percent {
    flex: none;
    min-width: 44px;
    text-align: right;
    font-size: 13px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .bar {
    flex: 1;
    height: 8px;
    border-radius: 999px;
    background: var(--accent-soft);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--accent);
    border-radius: inherit;
    transition: width 0.3s;
  }
  .bar.thin {
    height: 3px;
    border-radius: 0;
  }
  /* Scanning: no share yet, a band runs along the bar */
  .bar.indeterminate div {
    width: 30% !important;
    animation: sweep 1.4s ease-in-out infinite;
  }
  @keyframes sweep {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .pulse,
    .bar.indeterminate div {
      animation: none;
    }
  }
  .current {
    display: flex;
    gap: 6px;
    min-height: 20px;
    margin: 8px 20px 12px;
    font-size: 12.5px;
  }
  .current .mono {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }

  .entries {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    padding: 16px 20px;
    border-top: 1px solid var(--border);
    background: var(--bg);
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .entries > * {
    flex-shrink: 0;
  }
  section {
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
    overflow: hidden;
  }
  section > header {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 10px 14px;
    background: var(--surface-2);
    font-size: 13px;
  }
  .route {
    display: flex;
    gap: 8px;
    align-items: center;
    flex: 1;
    min-width: 0;
    font-weight: 600;
  }
  .route .mono {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .arrow {
    color: var(--muted);
  }
  .small {
    flex: none;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .state {
    --c: var(--muted);
    flex: none;
    padding: 2px 9px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    color: var(--c);
    background: color-mix(in srgb, var(--c) 14%, var(--surface));
  }
  .state.running {
    --c: var(--accent-text);
  }
  .state.done {
    --c: var(--ok);
  }
  .state.warn {
    --c: var(--warn);
  }
  .note {
    margin: 10px 14px;
    font-size: 13px;
  }
  .note.warn {
    color: var(--warn);
  }
</style>
