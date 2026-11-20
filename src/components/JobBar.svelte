<script lang="ts">
  // The running sync as a pill next to the page title: what runs, how far, how many wait, Cancel.
  import type { JobProgress } from '../api/types.ts';
  import { t, type TextKey } from '../i18n/index.svelte.ts';
  import { cancelJobs, jobs } from '../stores/jobs.svelte.ts';
  import { openRun } from '../stores/runView.svelte.ts';
  import { formatBytes } from '../utils/format.ts';

  const job = $derived(jobs.job);
  const LABELS: Partial<Record<JobProgress['origin'], TextKey>> = {
    schedule: 'app.scheduledSync',
    catch_up: 'app.catchUpSync',
  };
</script>

{#if job?.running}
  <div class="jobbar">
    <span class="spinner"></span>
    <!-- Opens the running sync live -->
    <button class="live" title={t('run.live')} onclick={() => job && openRun(job.backup_id)}>
      <span>{t(LABELS[job.origin] ?? 'app.sync')}: {job.label}</span>
      <span class="muted">{job.done_ops}/{job.total_ops} · {formatBytes(job.done_bytes)}</span>
    </button>
    {#if job.queued.length}
      <span class="muted">{t('app.waiting', { count: job.queued.length })}</span>
    {/if}
    <button class="ghost" onclick={cancelJobs} title={job.queued.length ? t('app.cancelAlsoQueued') : undefined}>
      {t('common.cancel')}
    </button>
  </div>
{/if}

<style>
  .jobbar {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 4px 6px 4px 14px;
    min-height: 34px;
  }
  /* Looks like the text it holds; underlined on hover */
  .live {
    display: inline-flex;
    gap: 10px;
    min-height: 0;
    padding: 0;
    border: none;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .live:hover:not(:disabled) {
    border: none;
    background: none;
    text-decoration: underline dotted;
    text-underline-offset: 3px;
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
</style>
