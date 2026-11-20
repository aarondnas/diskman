<script lang="ts">
  // The app shell: title bar, page title with the running sync, error message, notices of runs with
  // problems, the page shown (stores/navigation.svelte.ts) and what lies above all pages (info
  // button, problem details, the running sync and the last run, missed scheduled runs, question
  // dialog).
  // The logic lives in src/stores, the pages in src/features.
  import { onMount, untrack } from 'svelte';
  import ConfirmDialog from './components/ConfirmDialog.svelte';
  import JobBar from './components/JobBar.svelte';
  import ActivityPage from './features/activity/ActivityPage.svelte';
  import BackupsPage from './features/backups/BackupsPage.svelte';
  import EditorPage from './features/editor/EditorPage.svelte';
  import InfoPage from './features/info/InfoPage.svelte';
  import SettingsPage from './features/settings/SettingsPage.svelte';
  import ProblemsDialog from './features/problems/ProblemsDialog.svelte';
  import ActivityDialog from './features/activity/ActivityDialog.svelte';
  import RunDialog from './features/run/RunDialog.svelte';
  import { closeRun, runView } from './stores/runView.svelte.ts';
  import MissedRunsDialog from './features/schedule/MissedRunsDialog.svelte';
  import { setLanguage, t } from './i18n/index.svelte.ts';
  import { IS_WINDOWS } from './platform/index.ts';
  import TitleBar from './platform/windows/TitleBar.svelte';
  import { loadPlatform } from './stores/platform.svelte.ts';
  import { askAutostart, autostart, loadAutostart } from './stores/autostart.svelte.ts';
  import { dropChecks, dropOverall } from './stores/checks.svelte.ts';
  import { config, loadConfig, updateConfig } from './stores/config.svelte.ts';
  import { jobs, startPolling } from './stores/jobs.svelte.ts';
  import { nav, navigate } from './stores/navigation.svelte.ts';
  import { clearError, notice, showError } from './stores/notice.svelte.ts';
  import { loadMissed, missed } from './stores/missed.svelte.ts';
  import { closeProblems, dismissNotice, openProblems, problems, watchStatus } from './stores/problems.svelte.ts';
  import { formatCount } from './utils/format.ts';
  import { mirrorClashes } from './utils/mirrorClash.ts';
  import {
    hideLost,
    hideNotice,
    installUpdate,
    openWebsite,
    startUpdates,
    updates,
  } from './stores/updates.svelte.ts';
  import { ICONS } from './utils/icons.ts';
  import { applyTheme } from './utils/theme.ts';

  onMount(() => {
    loadConfig().then(
      // The config is the source of truth for the language and theme; the local copies only bridge
      // the start. Then: were scheduled runs missed while Diskman was off? (needs the backups for
      // the names) And may Diskman start at login? (asked once, after the missed runs)
      (c) => {
        setLanguage(c.settings?.language);
        applyTheme(c.settings?.light_mode);
        loadMissed().then(loadAutostart);
      },
      (e) => showError(t('app.backendUnreachable', { message: e.message })),
    );
    loadPlatform();
    const stopUpdates = startUpdates();
    const stopPolling = startPolling();
    return () => (stopUpdates(), stopPolling());
  });
  // The release notes of a new version, opened from its notice
  let showNotes = $state(false);

  // A sync makes check results outdated: that of "Check all" whenever one starts, that of a card
  // when its backup is queued or running
  $effect(() => {
    if (jobs.job?.running || jobs.queuedIds.length) untrack(dropOverall);
  });
  $effect(() => {
    const ids = [jobs.runningId, ...jobs.queuedIds];
    untrack(() => dropChecks(ids));
  });
  // First start: ask whether Diskman may start at login, once nothing else is asked
  $effect(() => {
    if (autostart.loaded && !autostart.asked && !missed.runs.length) untrack(askAutostart);
  });
  // The archives into which entries of the backup in the editor (saved or draft) and others mirror
  // at the same time
  const edited = $derived(nav.inEditor ? (nav.editing ?? nav.draft) : null);
  const clashedArchives = $derived(edited ? mirrorClashes(edited, config.backups) : []);

  // A run that ends with problems leaves a notice
  $effect(() => {
    const status = jobs.status;
    if (jobs.statusLoaded) untrack(() => watchStatus(status));
  });
</script>

<!-- Windows draws no frame around the window, the app does (Linux: the desktop's native frame) -->
{#if IS_WINDOWS}
  <TitleBar />
{/if}

<!-- Info in the bottom left corner of the window, on every page; a second click leads back -->
<button
  class="info"
  class:active={nav.page === 'info'}
  aria-current={nav.page === 'info' ? 'page' : undefined}
  title={t('nav.info')}
  aria-label={t('nav.info')}
  onclick={() => navigate(nav.page === 'info' ? 'backups' : 'info')}
>
  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true"><path fill="currentColor" d={ICONS.info} /></svg>
</button>

<main>
  <header class="top" class:home={nav.onList}>
    {#if nav.onList}
      <h1>{t('nav.backups')}</h1>
    {:else}
      <!-- The way back to the backups; in the editor the backup's name is the title -->
      <button class="ghost back" onclick={() => navigate('backups')}>
        <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d={ICONS.back} /></svg>
        {t('app.allBackups')}
      </button>
    {/if}
    <JobBar />
  </header>

  {#if nav.page !== 'backups'}
    <h1 class="page-title">{t(`nav.${nav.page}`)}</h1>
  {/if}

  {#if notice.error}
    <div class="error" role="alert">
      <span>{notice.error}</span>
      <button class="ghost icon" onclick={clearError} title={t('common.dismiss')} aria-label={t('common.dismiss')}>✕</button>
    </div>
  {/if}

  {#each problems.notices as n (n.backupId)}
    <div class="error problem" role="status">
      <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d={ICONS.warning} /></svg>
      <span>
        {n.count
          ? t('problems.notice', { name: n.name, count: n.count, n: formatCount(n.count) })
          : t('problems.failedNotice', { name: n.name })}
      </span>
      <button class="ghost" onclick={() => openProblems(n.backupId)}>{t('problems.details')}</button>
      <button
        class="ghost icon"
        onclick={() => dismissNotice(n.backupId)}
        title={t('common.dismiss')}
        aria-label={t('common.dismiss')}
      >
        ✕
      </button>
    </div>
  {/each}

  <!-- A new version of Diskman (stores/updates.svelte.ts): install it here (not while a backup runs
       or waits), or open its release page for copies that cannot update themselves -->
  {#if updates.status?.installing}
    <div class="update" role="status">
      <span class="spinner" aria-hidden="true"></span>
      <span>
        {updates.percent != null
          ? t('updates.downloading', { percent: updates.percent })
          : t('updates.downloadingUnknown')}
      </span>
    </div>
  {:else if updates.notice}
    <div class="update" role="status">
      <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d={ICONS.update} /></svg>
      <span>{t('updates.available', { version: updates.notice, current: updates.status?.current ?? '' })}</span>
      {#if updates.status?.available?.notes}
        <button class="ghost" onclick={() => (showNotes = !showNotes)} aria-expanded={showNotes}>{t('updates.notes')}</button>
      {/if}
      {#if updates.status?.can_install}
        <button
          class="ghost strong"
          onclick={installUpdate}
          disabled={updates.blocked}
          title={updates.blocked ? t('updates.blocked') : undefined}
        >
          {t('updates.install')}
        </button>
      {:else}
        <button class="ghost strong" onclick={openWebsite}>{t('updates.download')}</button>
      {/if}
      <button class="ghost icon" onclick={hideNotice} title={t('common.dismiss')} aria-label={t('common.dismiss')}>✕</button>
    </div>
    {#if showNotes && updates.status?.available?.notes}
      <pre class="notes">{updates.status.available.notes}</pre>
    {/if}
  {/if}

  <!-- Updates no longer come from GitHub with the known key (key lost, new repository): the new
       version has to be downloaded from the website by hand -->
  {#if updates.lost}
    <div class="error problem" role="status">
      <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d={ICONS.warning} /></svg>
      <span>{t(`updates.lost.${updates.lost}`)}</span>
      <button class="ghost" onclick={openWebsite}>{t('updates.lost.open')}</button>
      <button class="ghost icon" onclick={hideLost} title={t('common.dismiss')} aria-label={t('common.dismiss')}>✕</button>
    </div>
  {/if}

  <!-- In the editor: one warning when entries mirror into the same archive as another one, or into
       a folder above or below it (utils/mirrorClash.ts). Stays until nothing clashes any more. -->
  {#if clashedArchives.length}
    <div class="error problem" role="status">
      <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d={ICONS.warning} /></svg>
      <span>{t('card.mirrorClash', { archives: clashedArchives.join(', ') })}</span>
    </div>
  {/if}

  {#if nav.page === 'info'}
    <InfoPage />
  {:else if !config.current}
    <!-- Waiting for the config -->
  {:else if nav.page === 'settings'}
    <SettingsPage onupdate={updateConfig} />
  {:else if nav.page === 'activity'}
    <ActivityPage />
  {:else if nav.inEditor}
    <EditorPage />
  {:else}
    <BackupsPage />
  {/if}
</main>

{#if problems.open}
  <ProblemsDialog backupId={problems.open} onclose={closeProblems} />
{/if}

<!-- A running sync live (from its card or the job bar), then the details of the finished run -->
{#if runView.open}
  {#if runView.open.activity}
    <ActivityDialog activity={runView.open.activity} onclose={closeRun} />
  {:else}
    <RunDialog backupId={runView.open.backupId} onclose={closeRun} />
  {/if}
{/if}

{#if missed.runs.length && config.current}
  <MissedRunsDialog />
{/if}

<ConfirmDialog />

<style>
  /* Uses the width of large screens too; the side margin grows a little with the window. The
     limit only keeps the rows of the editor from getting endlessly long on very wide screens. */
  main {
    max-width: 2000px;
    margin: 0 auto;
    padding: 20px clamp(24px, 3vw, 56px) 64px;
  }
  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 20px;
    min-height: 40px;
    flex-wrap: wrap;
  }
  /* The "Backups" title of the list has more room below it */
  .top.home {
    margin-bottom: 32px;
  }
  h1 {
    margin: 0;
    font-size: 24px;
    letter-spacing: -0.02em;
    color: var(--text);
  }
  /* Settings and info: title below the way back */
  .page-title {
    margin-bottom: 20px;
  }
  .back {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: -8px;
    padding-left: 6px;
    border-color: transparent;
    color: var(--muted);
  }
  .back:hover:not(:disabled) {
    border-color: transparent;
    color: var(--accent-text);
  }
  .error {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    background: var(--danger-soft);
    color: var(--danger);
    border-radius: var(--radius);
    padding: 8px 8px 8px 14px;
    margin-bottom: 16px;
    font-size: 14px;
  }
  /* A run with problems: icon, text, "Details" and dismiss */
  .problem {
    justify-content: flex-start;
    background: var(--warn-soft);
    color: var(--warn);
  }
  .problem svg {
    flex: none;
  }
  .problem span {
    flex: 1;
    min-width: 0;
  }
  /* A new version: like the notices above, in the accent colour (news, not a problem) */
  .update {
    display: flex;
    align-items: center;
    gap: 12px;
    background: var(--accent-soft);
    color: var(--accent-text);
    border-radius: var(--radius);
    padding: 8px 8px 8px 14px;
    margin-bottom: 16px;
    font-size: 14px;
  }
  .update svg {
    flex: none;
  }
  .update > span:not(.spinner) {
    flex: 1;
    min-width: 0;
  }
  .update .strong {
    font-weight: 600;
  }
  .update .spinner {
    flex: none;
    width: 14px;
    height: 14px;
    border: 2px solid transparent;
    border-top-color: currentColor;
    border-right-color: currentColor;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .notes {
    margin: -8px 0 16px;
    padding: 10px 14px;
    max-height: 240px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    font-family: inherit;
    font-size: 13px;
    white-space: pre-wrap;
  }
  /* Round, without frame, grey; fixed in the corner, below the dialogs (z-index 22 and up) */
  .info {
    position: fixed;
    left: 12px;
    bottom: 12px;
    z-index: 15;
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    min-height: 0;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--muted);
  }
  .info:hover:not(:disabled) {
    background: var(--surface-2);
    color: var(--text);
  }
  .info.active {
    background: var(--accent-soft);
    color: var(--accent-text);
  }
</style>
