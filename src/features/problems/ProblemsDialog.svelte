<script lang="ts">
  // What went wrong in the last run of a backup: per entry (source → archive) the result and every
  // file or folder that was not transferred, with the reason and what helps. Opened from the notice
  // after a run and from the error status on the backup's card.
  import { untrack } from 'svelte';
  import type { Id, RunProblem } from '../../api/types.ts';
  import AdminHint from '../../components/AdminHint.svelte';
  import IssueList from '../../components/IssueList.svelte';
  import { t } from '../../i18n/index.svelte.ts';
  import { checkAndShow } from '../../stores/analysis.svelte.ts';
  import { pending as confirmPending } from '../../stores/confirm.svelte.ts';
  import { config } from '../../stores/config.svelte.ts';
  import { confirmAndRun } from '../../stores/jobs.svelte.ts';
  import { dismissNotice, loadProblems } from '../../stores/problems.svelte.ts';
  import { focusTrap } from '../../utils/focusTrap.ts';
  import { formatCount, relativeTime } from '../../utils/format.ts';
  import { adminCount } from '../../utils/issues.ts';

  let { backupId, onclose }: { backupId: Id; onclose: () => void } = $props();

  const backup = $derived(config.backups.find((b) => b.id === backupId));
  let list = $state<RunProblem[] | null>(null);
  let error = $state('');

  $effect(() => {
    const id = backupId;
    list = null;
    error = '';
    loadProblems(id).then(
      (l) => id === backupId && (list = l),
      (e: Error) => id === backupId && (error = e.message),
    );
  });

  const sourceOf = (archiveId: Id) => backup?.archives.find((a) => a.id === archiveId)?.source ?? '';
  const needAdmin = $derived(adminCount((list ?? []).flatMap((p) => p.issues)));

  // Seen: the notice of this backup is done
  $effect(() => {
    const id = backupId;
    untrack(() => dismissNotice(id));
  });

  function check() {
    onclose();
    checkAndShow(backupId);
  }

  function runAgain() {
    onclose();
    confirmAndRun(backupId);
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
  <div use:focusTrap tabindex="-1" class="modal" role="dialog" aria-modal="true" aria-labelledby="problems-title">
    <header>
      <div>
        <h2 id="problems-title">{t('problems.title')}</h2>
        <p class="muted">{backup?.name ?? ''}</p>
      </div>
      <button class="ghost icon" onclick={onclose} title={t('common.close')} aria-label={t('common.close')}>✕</button>
    </header>

    <div class="body">
      {#if error}
        <p class="failed">{error}</p>
      {:else if !list}
        <p class="muted">{t('list.checking')}</p>
      {:else if !list.length}
        <p class="ok">{t('problems.none')}</p>
      {:else}
        <AdminHint count={needAdmin} />
        {#each list as p (p.archive_id)}
          <section>
            <header>
              <div class="route">
                <span class="mono" title={sourceOf(p.archive_id)}>{sourceOf(p.archive_id)}</span>
                <span class="arrow">→</span>
                <span class="mono" title={p.dest}>{p.dest}</span>
              </div>
              <span class="muted small">{relativeTime(p.finished_at)}</span>
            </header>
            {#if p.issues.length}
              <p class="summary">{t('problems.notTransferred', { count: p.error_count, n: formatCount(p.error_count) })}</p>
              <IssueList issues={p.issues} total={p.error_count} />
            {:else}
              <!-- Failed or skipped as a whole: the backend's message says why -->
              <p class="failed">{p.message}</p>
            {/if}
          </section>
        {/each}
      {/if}
    </div>

    <footer>
      <span class="muted small">{t('problems.checkHint')}</span>
      <button class="ghost" onclick={check}>{t('list.check')}</button>
      <button class="amber" onclick={runAgain}>{t('problems.runAgain')}</button>
    </footer>
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
    width: min(880px, 100%);
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    box-shadow: 0 20px 60px rgb(0 0 0 / 0.35);
    overflow: hidden;
  }
  .modal > header {
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
  .modal > header p {
    margin: 2px 0 0;
  }
  .body {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    padding: 16px 20px;
    background: var(--bg);
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .body > * {
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
  .small {
    font-size: 12px;
  }
  .summary {
    margin: 10px 14px 4px;
    font-size: 13px;
    font-weight: 600;
    color: var(--danger);
  }
  .failed {
    margin: 12px 14px;
    color: var(--danger);
    font-size: 13px;
  }
  .ok {
    color: var(--ok);
    font-weight: 600;
    text-align: center;
  }
  footer {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 20px;
    border-top: 1px solid var(--border);
  }
  footer span {
    flex: 1;
    min-width: 0;
  }
</style>
