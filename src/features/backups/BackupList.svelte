<script lang="ts">
  // The backups as cards: name, schedule / check / run buttons, and below the last run, the
  // running sync or the result of a check. The last card starts a new backup.
  import type { ArchiveStatus, Backup, Id } from '../../api/types.ts';
  import CheckSummary from '../../components/CheckSummary.svelte';
  import DiskmanHead from '../../components/DiskmanHead.svelte';
  import { t } from '../../i18n/index.svelte.ts';
  import { checkAndShow, showAnalysis } from '../../stores/analysis.svelte.ts';
  import { cancelCheck, checks, live } from '../../stores/checks.svelte.ts';
  import { covered, statsText } from '../../utils/checkSummary.ts';
  import { cancelBackups, confirmAndRun, jobs } from '../../stores/jobs.svelte.ts';
  import { openEditor, startDraft } from '../../stores/navigation.svelte.ts';
  import { openProblems } from '../../stores/problems.svelte.ts';
  import { openLastRun, openRun } from '../../stores/runView.svelte.ts';
  import { formatBytes, formatCount, formatElapsed, relativeTime } from '../../utils/format.ts';
  import { ICONS } from '../../utils/icons.ts';
  import { describe, stateText } from '../../utils/scheduleText.ts';
  import { visorOf } from '../../utils/visor.ts';

  // backups: the cards to show, in order; onschedule(id): opens the schedule of a backup
  let { backups, onschedule }: { backups: Backup[]; onschedule: (id: Id) => void } = $props();

  const job = $derived(jobs.job);
  const runningId = $derived(jobs.runningId);
  const queuedIds = $derived(jobs.queuedIds);
  // Progress of the running backup in percent, for the bar at the bottom of its card
  const percent = $derived(job?.total_ops ? Math.round((job.done_ops / job.total_ops) * 100) : 0);

  /** Summarizes the status of all archives of a backup: an error wins, otherwise the latest run. */
  function summary(backup: Backup): { ok: boolean; text: string } | null {
    const runs = backup.archives.map((a) => jobs.status[a.id]).filter((st): st is ArchiveStatus => !!st);
    const failed = runs.find((st) => !st.ok);
    if (failed) return { ok: false, text: failed.message };
    if (!runs.length) return null;
    const last = runs.reduce((a, b) => (new Date(a.last_run) > new Date(b.last_run) ? a : b));
    return { ok: true, text: t('list.lastRun', { time: relativeTime(last.last_run) }) };
  }
</script>

<!-- The arrow at the end of a capsule that opens more -->
{#snippet more()}
  <svg class="more" viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
    <path fill="currentColor" d="M10 6 8.6 7.4l4.6 4.6-4.6 4.6L10 18l6-6z" />
  </svg>
{/snippet}

<!-- Replaces the spinner while the pointer is on a button that cancels -->
{#snippet stop()}
  <svg class="stop" viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d={ICONS.stop} /></svg>
{/snippet}

{#if !backups.length}
  <p class="empty">{t('list.empty')}</p>
{/if}
<ul class="grid">
    {#each backups as backup (backup.id)}
      {@const st = summary(backup)}
      {@const running = backup.id === runningId}
      {@const place = queuedIds.indexOf(backup.id) + 1}
      {@const chk = checks[backup.id]}
      {@const showCheck = !running && !place && chk}
      {@const sched = backup.schedule}
      {@const runnable = backup.archives.some((a) => a.enabled)}
      {@const checkCancel = !!chk?.loading && !chk.batch}
      {@const runCancel = (running || place > 0) && !jobs.inBatch(backup.id)}
      <li class="tile" class:running class:queued={!running && place > 0}>
        <!-- Click area over the whole card; the buttons sit on top of it -->
        <button class="open" aria-label={t('list.edit', { name: backup.name })} onclick={() => openEditor(backup.id)}></button>
        <div class="head">
          <!-- The Diskman rack with its lights in the colour of the backup -->
          <span class="icon" style:--visor={visorOf(backup)} aria-hidden="true">
            <DiskmanHead visor={visorOf(backup)} />
          </span>
          <span class="title">
            <span class="name" title={backup.name}>{backup.name}</span>
            <span class="count">{t('list.entryCount', { count: backup.archives.length })}</span>
          </span>
          <!-- Schedule, check and run next to the title; the footer keeps its full width for the status -->
          <div class="actions">
        <!-- Schedule of this backup: amber while one is active, only an amber ring while it is paused -->
        <button
          class="round run sched"
          class:on={sched?.enabled}
          class:paused={sched && !sched.enabled}
          title={sched ? `${describe(sched.when)} · ${stateText(sched)}` : t('scheduler.none')}
          aria-label={t('scheduler.cardLabel', { name: backup.name })}
          onclick={() => onschedule(backup.id)}
        >
          <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d={ICONS.clock} /></svg>
        </button>
        <!-- While its own check runs it cancels it; one of "Check all" is cancelled there -->
        <button
          class="round run check"
          class:cancel={checkCancel}
          title={checkCancel ? t('list.cancelCheck') : chk?.loading ? t('list.checking') : t('list.check')}
          aria-label={checkCancel
            ? t('list.cancelCheckLabel', { name: backup.name })
            : t('list.checkLabel', { name: backup.name })}
          aria-busy={!!chk?.loading}
          disabled={chk?.loading ? !checkCancel : running || place > 0 || !runnable}
          onclick={() => (checkCancel ? cancelCheck(backup.id) : checkAndShow(backup.id))}
        >
          {#if chk?.loading}
            <span class="spinner"></span>
            {#if checkCancel}{@render stop()}{/if}
          {:else}
            <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d={ICONS.check} /></svg>
          {/if}
        </button>
        <!-- While it runs or waits it cancels it, unless "Run all" started it (cancelled there) -->
        <button
          class="round run"
          class:running
          class:queued={place > 0}
          class:cancel={runCancel}
          title={running
            ? t(runCancel ? 'list.cancelSync' : 'common.syncing')
            : place
              ? t(runCancel ? 'list.cancelWaiting' : 'list.waiting', { place })
              : t('common.runNow')}
          aria-label={runCancel
            ? t('list.cancelRunLabel', { name: backup.name })
            : running
              ? t('list.isSyncing', { name: backup.name })
              : place
                ? t('list.isWaiting', { name: backup.name })
                : t('list.run', { name: backup.name })}
          aria-busy={running || place > 0}
          disabled={running || place > 0 ? !runCancel : !runnable}
          onclick={() => (runCancel ? cancelBackups([backup.id]) : confirmAndRun(backup.id))}
        >
          {#if running}
            <span class="spinner"></span>
            {#if runCancel}{@render stop()}{/if}
          {:else if place}
            <span class="waiting"></span>
            {#if runCancel}{@render stop()}{/if}
          {:else}
            <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d={ICONS.play} /></svg>
          {/if}
        </button>
          </div>
        </div>
        <div class="foot">
        <span
          class="status"
          class:err={showCheck ? !!(chk.error || chk.dangerous) : !running && !place && st && !st.ok}
          class:active={running}
          class:queued={!running && place > 0}
        >
          {#if running}
            <!-- Opens the run live: its figures and a tree of what it has done so far -->
            <button
              class="capsule"
              title={t('run.live')}
              aria-label={t('run.liveLabel', { name: backup.name })}
              onclick={() => openRun(backup.id)}
            >
              <span class="dot"></span>{t('common.syncing')}
              {#if job?.total_ops}
                <span class="progress">{job.done_ops}/{job.total_ops}</span>
              {/if}
              {@render more()}
            </button>
          {:else if place}
            {t('list.pending')}<span class="dots" aria-hidden="true"><i></i><i></i><i></i></span>
          {:else if showCheck && chk.loading}
            <span class="muted">{t('list.checking')}</span>
          {:else if showCheck && chk.error}
            <span class="dot"></span>{chk.error}
          {:else if showCheck && chk.plans}
            <!-- The result opens the whole analysis (the preview with every file change) -->
            <CheckSummary
              summary={chk}
              label={t('list.showAnalysisLabel', { name: backup.name })}
              onclick={() => showAnalysis(backup.id, chk.plans)}
            />
          {:else if st && !st.ok}
            <!-- The last run had problems: a click shows what was not transferred and why -->
            <button
              class="problem"
              title={t('problems.show')}
              aria-label={t('problems.showLabel', { name: backup.name })}
              onclick={() => openProblems(backup.id)}
            >
              <span class="dot"></span>{st.text}
            </button>
          {:else if st}
            <!-- The last run again in detail: what it changed, file by file -->
            <button
              class="capsule"
              title={t('run.lastRun')}
              aria-label={t('run.lastRunLabel', { name: backup.name })}
              onclick={() => openLastRun(backup.id)}
            >
              <span class="dot"></span>{st.text}
              {@render more()}
            </button>
          {:else}
            <span class="muted">{t('common.neverSynced')}</span>
          {/if}
        </span>
        <!-- Time of the check and the files and folders it read: live while it runs, then the end result.
             For a running sync its time and the data copied so far. -->
        {#if running && job}
          <span class="stats live">
            <span class="sr">{t('list.runStats')}: </span>{formatElapsed(
              job.started_at ? jobs.now - Date.parse(job.started_at) : 0,
              true,
            )}{#if job.total_bytes}{' · '}{t('run.of', { done: formatBytes(job.done_bytes), total: formatBytes(job.total_bytes) })}{/if}{#if job.error_count}{' · '}{t('activity.errors', { count: job.error_count, n: formatCount(job.error_count) })}{/if}
          </span>
        {:else if showCheck && chk.loading}
          <span class="stats live">
            <span class="sr">{t('list.scanStats')}: </span>{statsText(
              live.now - chk.started,
              live.progress[backup.id] ?? { files: 0, dirs: 0 },
              true,
            )}
          </span>
        {:else if showCheck && chk.plans}
          <span class="stats">
            <span class="sr">{t('list.scanStats')}: </span>{statsText(chk.elapsedMs, covered(chk.plans))}
          </span>
        {/if}
        </div>
        {#if running}
          <div class="bar" aria-hidden="true"><div style="width: {percent}%"></div></div>
        {/if}
      </li>
    {/each}
    <!-- Like "Add entry" in the editor: an empty card that starts a new backup -->
    <li class="add">
      <button onclick={startDraft}>
        <!-- A greyed-out Diskman rack, as the new backup will get one; it takes its colour on hover -->
        <span class="ghost" aria-hidden="true">
          <DiskmanHead />
        </span>
        {t('list.newBackup')}
      </button>
    </li>
  </ul>

<style>
  /* As many columns as fit, each card at least 300 px wide (name next to the three buttons) */
  .grid {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 14px;
  }
  .tile {
    position: relative;
    display: flex;
    flex-direction: column;
    min-width: 0;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: var(--shadow);
    overflow: hidden;
    transition:
      border-color 0.12s,
      box-shadow 0.12s,
      transform 0.12s;
  }
  .tile:hover {
    border-color: var(--amber);
    box-shadow: 0 4px 14px rgb(0 0 0 / 0.08);
    transform: translateY(-1px);
  }
  .tile.running,
  .tile.queued {
    border-color: var(--accent);
  }
  .tile.queued {
    border-style: dashed;
  }
  @media (prefers-reduced-motion: reduce) {
    .tile:hover {
      transform: none;
    }
  }
  .open {
    position: absolute;
    inset: 0;
    min-height: 0;
    padding: 0;
    border: none;
    border-radius: 12px;
    background: transparent;
  }
  .open:focus-visible {
    outline-offset: -2px;
  }
  /* Only the buttons react to the mouse; everything else lets the click through to .open */
  .tile > :not(.open) {
    pointer-events: none;
  }
  /* Fills the card, so the footers of cards in one row line up */
  .head {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 16px;
  }
  .icon {
    flex: none;
    display: grid;
    place-items: center;
    width: 42px;
    height: 42px;
    border-radius: 10px;
    /* A light tint of the visor colour behind the rack */
    background: color-mix(in srgb, var(--visor) 16%, var(--surface));
  }
  .tile.running .icon {
    background: color-mix(in srgb, var(--visor) 45%, var(--surface));
  }
  .title {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .actions {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .count {
    font-size: 12px;
    color: var(--muted);
  }
  /* Status of the last run or check over the full width */
  .foot {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    row-gap: 4px;
    min-height: 46px;
    padding: 8px 16px;
    border-top: 1px solid var(--border-soft);
    background: var(--surface-2);
  }
  .actions > .run {
    position: relative;
    pointer-events: auto;
    color: var(--accent-text);
  }
  .run:hover:not(:disabled) {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
  }
  .run.running,
  .run.queued {
    opacity: 1;
    cursor: progress;
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .run.queued {
    border-style: dashed;
  }
  /* Running or waiting and cancellable: the spinner stays until the pointer is on it, then a
     stop symbol in red takes its place */
  .run.cancel {
    cursor: pointer;
  }
  .run .stop {
    display: none;
  }
  .run.cancel:hover:not(:disabled) .stop,
  .run.cancel:focus-visible .stop {
    display: block;
  }
  .run.cancel:hover:not(:disabled) :is(.spinner, .waiting),
  .run.cancel:focus-visible :is(.spinner, .waiting) {
    display: none;
  }
  .run.cancel:hover:not(:disabled) {
    background: var(--danger-soft);
    border-color: var(--danger);
    border-style: solid;
    color: var(--danger);
  }
  /* Schedule button: grey without a schedule, amber while one is active, only an amber ring when paused */
  .run.sched {
    color: var(--muted);
  }
  .run.sched.on {
    overflow: hidden;
    background: color-mix(in srgb, var(--amber) 22%, var(--surface));
    border-color: var(--amber);
    color: var(--text);
    animation: sched-glow 3s ease-in-out infinite;
  }
  /* Active: a soft beam of light sweeps round inside like a glowing clock hand, while the
     button slowly breathes a warm glow */
  .run.sched.on::before {
    content: '';
    position: absolute;
    inset: -1px;
    border-radius: inherit;
    background: conic-gradient(
      from 0deg,
      transparent 0deg 250deg,
      color-mix(in srgb, var(--amber) 25%, transparent) 320deg,
      color-mix(in srgb, var(--amber) 70%, transparent) 358deg,
      transparent 360deg
    );
    animation: sched-sweep 4s linear infinite;
    pointer-events: none;
  }
  .run.sched.on svg {
    position: relative;
  }
  .run.sched.on:hover:not(:disabled) {
    animation: none;
  }
  .run.sched.on:hover:not(:disabled)::before {
    display: none;
  }
  @keyframes sched-sweep {
    to {
      transform: rotate(360deg);
    }
  }
  @keyframes sched-glow {
    0%,
    100% {
      box-shadow: 0 0 0 0 color-mix(in srgb, var(--amber) 0%, transparent);
    }
    50% {
      box-shadow: 0 0 10px 1px color-mix(in srgb, var(--amber) 55%, transparent);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .run.sched.on,
    .run.sched.on::before {
      animation: none;
    }
    .run.sched.on::before {
      display: none;
    }
  }
  .run.sched.paused {
    border-color: var(--amber);
  }
  /* Hover like the other buttons on the card (grey); with an active schedule in its amber */
  .run.sched:hover:not(:disabled) {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
  }
  .run.sched.on:hover:not(:disabled) {
    background: var(--amber);
    border-color: var(--amber);
    color: var(--on-amber);
  }
  .spinner {
    width: 16px;
    height: 16px;
    border: 2px solid transparent;
    border-top-color: var(--accent);
    border-right-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  /* Waiting: dotted ring that rotates slowly */
  .waiting {
    width: 16px;
    height: 16px;
    border: 2px dotted var(--accent);
    border-radius: 50%;
    animation: spin 2.4s linear infinite;
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
    .waiting {
      animation-duration: 6s;
    }
  }
  .name {
    font-size: 15px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Longer texts (e.g. an error message) may take two lines */
  .status {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    /* A line of its own: with a basis of 0 the stats below (flex-basis 100%) would share its line
       and squeeze it to no width at all */
    flex: 1 1 100%;
    min-width: 0;
    font-size: 12px;
    color: var(--muted);
    overflow: hidden;
  }
  .status .dot {
    display: inline-block;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--ok);
    margin-right: 6px;
    vertical-align: 1px;
  }
  .status.active {
    color: var(--accent-text);
    font-weight: 600;
  }
  .status.active .dot {
    background: var(--accent);
    animation: pulse 1.2s ease-in-out infinite;
  }
  .status.queued {
    color: var(--accent-text);
  }
  /* Three dots that bounce up and down one after another */
  .dots {
    display: inline-flex;
    gap: 3px;
    margin-left: 6px;
    vertical-align: 1px;
  }
  .dots i {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--accent);
    animation: hop 1.2s ease-in-out infinite;
  }
  .dots i:nth-child(2) {
    animation-delay: 0.15s;
  }
  .dots i:nth-child(3) {
    animation-delay: 0.3s;
  }
  @keyframes hop {
    0%,
    60%,
    100% {
      transform: translateY(0);
      opacity: 0.35;
    }
    30% {
      transform: translateY(-3px);
      opacity: 1;
    }
  }
  /* Time, files and folders of the check: its own line below the status */
  .stats {
    flex-basis: 100%;
    min-width: 0;
    font-size: 11.5px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .stats.live {
    color: var(--accent-text);
  }
  /* Only for screen readers */
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  .progress {
    margin-left: 4px;
    font-weight: 400;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  @keyframes pulse {
    50% {
      opacity: 0.3;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .status.active .dot,
    .dots i {
      animation: none;
    }
  }
  .status.err {
    color: var(--danger);
  }
  .status.err .dot {
    background: var(--danger);
  }
  /* Error status as a link to its details; looks like the text it replaces */
  .problem {
    position: relative;
    pointer-events: auto;
    display: inline;
    min-height: 0;
    padding: 0;
    border: none;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    text-decoration: underline dotted;
    text-underline-offset: 3px;
    cursor: pointer;
  }
  .problem:hover:not(:disabled) {
    border: none;
    background: none;
    text-decoration-style: solid;
  }
  /* The running sync and the last run as a clickable capsule, like the result of a check
     (CheckSummary); amber on hover */
  .capsule {
    position: relative;
    pointer-events: auto;
    display: inline-flex;
    align-items: center;
    max-width: 100%;
    min-height: 26px;
    padding: 2px 6px 2px 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    color: inherit;
    font: inherit;
    white-space: nowrap;
    overflow: hidden;
    cursor: pointer;
  }
  .capsule:hover:not(:disabled) {
    border-color: var(--amber);
    background: color-mix(in srgb, var(--amber) 12%, var(--surface));
    color: var(--text);
  }
  .capsule .dot {
    flex: none;
  }
  .more {
    flex: none;
    margin-left: 2px;
    opacity: 0.6;
  }
  .capsule:hover .more {
    opacity: 1;
  }
  /* Progress of the running backup along the bottom edge of its card */
  .bar {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 3px;
    background: var(--accent-soft);
  }
  .bar div {
    height: 100%;
    background: var(--accent);
    transition: width 0.3s;
  }
  .empty {
    margin: 0 0 14px;
    color: var(--muted);
  }
  /* Empty, dashed card; stretches to the height of the cards in its row */
  .add {
    display: flex;
    min-height: 128px;
  }
  .add button {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    border: 1.5px dashed var(--border);
    border-radius: 12px;
    background: transparent;
    color: var(--muted);
    font-weight: 600;
    transition:
      border-color 0.12s,
      color 0.12s,
      background 0.12s;
  }
  .add button:hover:not(:disabled) {
    border-color: var(--amber);
    color: var(--text);
    background: color-mix(in srgb, var(--amber) 6%, transparent);
  }
  /* Same box as the head on a card, but grey and faded like the rest of the empty card */
  .ghost {
    display: grid;
    place-items: center;
    width: 42px;
    height: 42px;
    border-radius: 10px;
    background: var(--surface);
    border: 1px solid var(--border);
    transition:
      background 0.12s,
      border-color 0.12s;
  }
  .ghost :global(svg) {
    filter: grayscale(1);
    opacity: 0.4;
    transition:
      filter 0.12s,
      opacity 0.12s;
  }
  .add button:hover .ghost {
    background: color-mix(in srgb, var(--amber) 16%, var(--surface));
    border-color: var(--amber);
  }
  .add button:hover .ghost :global(svg) {
    filter: none;
    opacity: 1;
  }
</style>
