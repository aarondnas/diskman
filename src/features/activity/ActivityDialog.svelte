<script lang="ts">
  // One activity in detail: per entry (source → archive) how it ended and what it changed, file by
  // file, as a tree like in the analysis (PipelineReport), and for a run what failed. The backend
  // keeps the stored plans while this is open (commands/activity.rs).
  import { onDestroy } from 'svelte';
  import { backend } from '../../api/backend.ts';
  import type { Activity, ActivityEntry, EntryDetail } from '../../api/types.ts';
  import IssueList from '../../components/IssueList.svelte';
  import { t } from '../../i18n/index.svelte.ts';
  import { pending as confirmPending } from '../../stores/confirm.svelte.ts';
  import { focusTrap } from '../../utils/focusTrap.ts';
  import { formatCount, formatElapsed } from '../../utils/format.ts';
  import PipelineReport from '../analysis/PipelineReport.svelte';
  import { outcomeOf, whenParts } from './activity.ts';

  let { activity, onclose }: { activity: Activity; onclose: () => void } = $props();

  let details = $state<EntryDetail[] | null>(null);
  let error = $state('');

  $effect(() => {
    const a = activity;
    details = null;
    error = '';
    backend.openActivity(a.id, a.entries.map((e) => e.id)).then(
      (d) => a === activity && (details = d),
      (e: Error) => a === activity && (error = e.message),
    );
  });
  onDestroy(() => backend.closeActivity().catch(() => {}));

  const when = $derived(whenParts(activity.started_at));
  const outcome = $derived(outcomeOf(activity));
  const duration = $derived(
    activity.finished_at ? new Date(activity.finished_at).getTime() - new Date(activity.started_at).getTime() : null,
  );
  const detailOf = (e: ActivityEntry) => details?.find((d) => d.id === e.id);

  /** For a run that stopped early: how far it got. */
  function progressText(e: ActivityEntry): string {
    if (activity.kind !== 'sync' || !['cancelled', 'interrupted'].includes(e.outcome) || !e.ops_total) return '';
    return t('activity.detail.progress', { done: formatCount(e.ops_done), total: formatCount(e.ops_total) });
  }

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
  <div use:focusTrap tabindex="-1" class="modal" role="dialog" aria-modal="true" aria-labelledby="activity-title">
    <header>
      <div>
        <h2 id="activity-title">{t(`activity.kinds.${activity.kind}`)} · {activity.backup_name}</h2>
        <p class="muted">
          {when.date}, {when.time} · {t(`activity.origins.${activity.origin}`)}
          {#if duration != null}· {t('activity.detail.duration', { time: formatElapsed(duration) })}{/if}
        </p>
      </div>
      <span class="outcome {outcome}">{t(`activity.outcomes.${outcome}`)}</span>
      <button class="ghost icon" onclick={onclose} title={t('common.close')} aria-label={t('common.close')}>✕</button>
    </header>

    <div class="body">
      {#if error}
        <p class="failed">{error}</p>
      {:else if !details}
        <p class="muted">{t('preview.tree.loading')}</p>
      {:else if !activity.entries.length}
        <p class="muted">{t('activity.detail.noEntries')}</p>
      {:else}
        {#each activity.entries as e (e.id)}
          {@const d = detailOf(e)}
          {@const progress = progressText(e)}
          <div class="entry">
            {#if e.outcome !== 'ok' || progress}
              <div class="status {e.outcome}">
                <b>{t(`activity.outcomes.${e.outcome}`)}</b>
                {#if progress}<span>{progress}</span>{/if}
                {#if e.outcome === 'failed' || e.outcome === 'skipped' || e.outcome === 'interrupted'}
                  <span class="message">{e.message}</span>
                {/if}
              </div>
            {/if}

            {#if d?.plan}
              <PipelineReport plan={d.plan} live={false} />
            {:else}
              <section class="plain">
                <header>
                  <div class="route">
                    <span class="mono" title={e.source}>{e.source}</span>
                    <span class="arrow">→</span>
                    <span class="mono" title={e.dest}>{e.dest}</span>
                  </div>
                </header>
                {#if e.outcome !== 'failed'}
                  <p class="muted note">{t('activity.detail.noPlan')}</p>
                {/if}
              </section>
            {/if}

            {#if d?.errors.length}
              <section class="errors">
                <p class="summary">{t('problems.notTransferred', { count: e.error_count, n: formatCount(e.error_count) })}</p>
                <IssueList issues={d.errors} total={e.error_count} />
              </section>
            {/if}
          </div>
        {/each}
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
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    box-shadow: 0 20px 60px rgb(0 0 0 / 0.35);
    overflow: hidden;
  }
  .modal > header {
    display: flex;
    align-items: flex-start;
    gap: 14px;
    padding: 18px 20px 14px;
    border-bottom: 1px solid var(--border);
  }
  .modal > header > div {
    flex: 1;
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: 19px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .modal > header p {
    margin: 2px 0 0;
    font-size: 13px;
  }
  /* --c: colour of a result */
  .outcome,
  .status {
    --c: var(--muted);
  }
  .ok {
    --c: var(--ok);
  }
  .errors,
  .cancelled,
  .interrupted,
  .skipped {
    --c: var(--warn);
  }
  .failed {
    --c: var(--danger);
  }
  .running {
    --c: var(--accent-text);
  }
  .outcome {
    align-self: center;
    padding: 3px 10px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    color: var(--c);
    background: color-mix(in srgb, var(--c) 14%, var(--surface));
  }
  .body {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    padding: 16px 20px;
    background: var(--bg);
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .body > * {
    flex-shrink: 0;
  }
  .body > .failed {
    color: var(--danger);
  }
  .entry {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .status {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    align-items: baseline;
    padding: 8px 12px;
    border-radius: var(--radius);
    font-size: 13px;
    color: var(--c);
    background: color-mix(in srgb, var(--c) 12%, var(--surface));
  }
  .message {
    color: var(--text);
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
    border-bottom: 1px solid var(--border-soft);
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
  .note {
    margin: 12px 14px;
    font-size: 13px;
  }
  section.errors {
    padding-bottom: 6px;
  }
  .summary {
    margin: 10px 14px 4px;
    font-size: 13px;
    font-weight: 600;
    color: var(--danger);
  }
</style>
