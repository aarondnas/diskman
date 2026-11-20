<script lang="ts">
  // The home page: the toolbar and the backups as cards, with the dialogs that open over them
  // (schedule of a backup, analysis of a check).
  import { onMount } from 'svelte';
  import type { BackupSchedule, Id } from '../../api/types.ts';
  import { t } from '../../i18n/index.svelte.ts';
  import { analysis, closeAnalysis, openNext, rescan, setListShown } from '../../stores/analysis.svelte.ts';
  import { saveSchedule, saveSchedules } from '../../stores/backups.svelte.ts';
  import { config, findBackup } from '../../stores/config.svelte.ts';
  import { jobs } from '../../stores/jobs.svelte.ts';
  import { runView } from '../../stores/runView.svelte.ts';
  import AnalysisDialog from '../analysis/AnalysisDialog.svelte';
  import ScheduleDialog from '../schedule/ScheduleDialog.svelte';
  import BackupList from './BackupList.svelte';
  import BackupsToolbar from './BackupsToolbar.svelte';
  import { clearQuery, view, visibleBackups } from './listView.svelte.ts';

  // Backup whose schedule is open (clock button on its card)
  let scheduleId = $state<Id | null>(null);
  const scheduling = $derived(scheduleId ? findBackup(scheduleId) : null);
  // "Schedule all": ids chosen when it opened (the shown backups), null while closed
  let scheduleMany = $state<Id[] | null>(null);

  const shown = $derived(visibleBackups(config.backups, view));
  // Shown backups that can run now (an enabled entry, not running or waiting), in list order
  const runnable = $derived(
    shown
      .filter((b) => b.archives.some((a) => a.enabled))
      .map((b) => b.id)
      .filter((id) => id !== jobs.runningId && !jobs.queuedIds.includes(id)),
  );

  // Finished checks open their analysis by themselves only while this page is shown, one after
  // another, and not over the schedule dialog
  onMount(() => {
    setListShown(true);
    return () => setListShown(false);
  });
  $effect(() => openNext(!!scheduleId || !!scheduleMany || !!runView.open));

  async function onsave(schedule: BackupSchedule | null) {
    if (!scheduleId) return false;
    const ok = await saveSchedule(scheduleId, schedule);
    if (ok) scheduleId = null;
    return ok;
  }

  async function onsaveMany(schedule: BackupSchedule | null, ids: Id[] = []) {
    const ok = await saveSchedules(ids, schedule);
    if (ok) scheduleMany = null;
    return ok;
  }
</script>

<BackupsToolbar
  count={config.backups.length}
  {runnable}
  shown={shown.length}
  onscheduleall={() => (scheduleMany = shown.map((b) => b.id))}
/>

{#if view.query.trim() && !shown.length}
  <div class="noresults">
    <p class="muted">{t('app.noMatch', { query: view.query.trim() })}</p>
    <button class="ghost" onclick={clearQuery}>{t('app.clearSearch')}</button>
  </div>
{:else}
  <BackupList backups={shown} onschedule={(id) => (scheduleId = id)} />
{/if}

{#if scheduling}
  {#key scheduling.id}
    <ScheduleDialog backup={scheduling} {onsave} oncancel={() => (scheduleId = null)} />
  {/key}
{/if}

{#if scheduleMany}
  <ScheduleDialog targets={scheduleMany} choices={config.backups} onsave={onsaveMany} oncancel={() => (scheduleMany = null)} />
{/if}

{#if analysis.open}
  <AnalysisDialog plans={analysis.open.plans} onrefresh={() => rescan(runnable)} onclose={closeAnalysis} />
{/if}

<style>
  .noresults {
    border: 1px dashed var(--border);
    border-radius: 12px;
    padding: 24px;
    text-align: center;
  }
  .noresults p {
    margin: 0 0 10px;
  }
</style>
