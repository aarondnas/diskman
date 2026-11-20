<script lang="ts">
  // Asked once after the start when schedules that catch up were due while Diskman was off
  // (stores/missed.svelte.ts, src-tauri/src/jobs/catch_up.rs): which backups, how long overdue,
  // how many runs missed. All are chosen; the chosen ones run one after another in list order.
  // "Not now" (also Escape or a click beside it) runs nothing; the schedules go on as usual.
  import { fade, scale } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import type { Backup, Id, MissedRun } from '../../api/types.ts';
  import DiskmanHead from '../../components/DiskmanHead.svelte';
  import { t } from '../../i18n/index.svelte.ts';
  import { findBackup } from '../../stores/config.svelte.ts';
  import { pending as confirmPending } from '../../stores/confirm.svelte.ts';
  import { dismissMissed, missed, runMissed } from '../../stores/missed.svelte.ts';
  import { formatDuration, formatMoment, relativeTime } from '../../utils/format.ts';
  import { focusTrap } from '../../utils/focusTrap.ts';
  import { CROSS, ICONS } from '../../utils/icons.ts';
  import { describe } from '../../utils/scheduleText.ts';
  import { visorOf } from '../../utils/visor.ts';

  // Backups that still exist, with their missed runs; taken once, so the list does not change
  // while it is open
  // svelte-ignore state_referenced_locally
  const rows = missed.runs
    .map((m) => ({ ...m, backup: findBackup(m.backup_id) }))
    .filter((r): r is MissedRun & { backup: Backup } => !!r.backup);
  // svelte-ignore state_referenced_locally
  let chosen = $state(rows.map((r) => r.backup_id));
  const now = Date.now();

  function toggle(id: Id) {
    chosen = chosen.includes(id) ? chosen.filter((c) => c !== id) : [...chosen, id];
  }

  // In the order of the list, whatever order they were ticked in
  const run = () => runMissed(rows.map((r) => r.backup_id).filter((id) => chosen.includes(id)));

  function onkeydown(e: KeyboardEvent) {
    if (confirmPending.current) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      dismissMissed();
    }
  }
</script>

<svelte:window {onkeydown} />

{#if rows.length}
  <div
    class="backdrop"
    role="presentation"
    transition:fade={{ duration: 120 }}
    onclick={(e) => e.target === e.currentTarget && dismissMissed()}
  >
    <!-- Focus starts on the run button: one Enter runs what is overdue -->
    <div
      use:focusTrap={'button.primary'}
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="missed-title"
      aria-describedby="missed-intro"
      transition:scale={{ duration: 140, start: 0.96, easing: cubicOut }}
    >
      <header>
        <span class="badge" aria-hidden="true">
          <svg viewBox="0 0 24 24" width="22" height="22"><path fill="currentColor" d={ICONS.clock} /></svg>
        </span>
        <div class="titles">
          <span class="eyebrow">{t('missed.eyebrow')}</span>
          <h2 id="missed-title">{t('missed.title', { count: rows.length })}</h2>
        </div>
        <button class="close" title={t('common.close')} aria-label={t('common.close')} onclick={dismissMissed}>
          <svg viewBox="0 0 10 10" width="10" height="10" aria-hidden="true">
            <path d={CROSS} stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
          </svg>
        </button>
      </header>

      <div class="body">
        <p id="missed-intro" class="intro">{t('missed.intro')}</p>
        <ul>
          {#each rows as r (r.backup_id)}
            <li>
              <label class:off={!chosen.includes(r.backup_id)}>
                <input type="checkbox" checked={chosen.includes(r.backup_id)} onchange={() => toggle(r.backup_id)} />
                <DiskmanHead visor={visorOf(r.backup)} size={32} />
                <span class="text">
                  <span class="line">
                    <b class="name">{r.backup.name}</b>
                    {#if r.backup.schedule}<span class="muted">{describe(r.backup.schedule.when)}</span>{/if}
                  </span>
                  <span class="overdue">
                    {t('missed.overdue', { duration: formatDuration(now - new Date(r.due_since).getTime()) })}
                    · {t('missed.count', { count: r.missed })}
                  </span>
                  <span class="muted small">
                    {t('missed.dueSince', { when: formatMoment(r.due_since) })}
                    · {r.last_run ? t('missed.lastRun', { when: relativeTime(r.last_run) }) : t('missed.neverRun')}
                  </span>
                </span>
              </label>
            </li>
          {/each}
        </ul>
      </div>

      <footer>
        <span class="spacer"></span>
        <button class="ghost" onclick={dismissMissed}>{t('missed.later')}</button>
        <button class="primary" disabled={!chosen.length} onclick={run}>
          {chosen.length ? t('missed.run', { count: chosen.length }) : t('missed.runNone')}
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  /* Like the schedule dialog: above the top bar, below the question dialog (z-index 30) */
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 25;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgb(0 0 0 / 0.45);
    backdrop-filter: blur(2px);
  }
  .dialog {
    width: min(560px, 100%);
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 14px;
    box-shadow: 0 20px 60px rgb(0 0 0 / 0.35);
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 18px 16px 14px 22px;
  }
  /* Overdue: in the warning colour of the notices after a run with problems */
  .badge {
    flex: none;
    display: grid;
    place-items: center;
    width: 42px;
    height: 42px;
    border-radius: 50%;
    background: var(--warn-soft);
    color: var(--warn);
  }
  .titles {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .eyebrow {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  .close {
    flex: none;
    align-self: flex-start;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    min-height: 0;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--muted);
  }
  .close:hover:not(:disabled) {
    background: var(--surface-2);
    color: var(--text);
  }
  .body {
    display: grid;
    gap: 14px;
    padding: 4px 22px 20px;
    overflow-y: auto;
  }
  .intro {
    margin: 0;
    font-size: 14px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }
  label {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-2);
    cursor: pointer;
    transition: opacity 0.12s;
  }
  label:hover {
    border-color: var(--amber);
  }
  /* Not chosen: faded, so it is clear what will run */
  label.off {
    opacity: 0.55;
  }
  input[type='checkbox'] {
    flex: none;
  }
  .text {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .line {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 10px;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .line .muted {
    font-size: 13px;
  }
  .overdue {
    font-size: 13px;
    font-weight: 600;
    color: var(--warn);
  }
  .small {
    font-size: 12px;
  }
  footer {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 12px 16px;
    background: var(--surface-2);
    border-top: 1px solid var(--border-soft);
  }
  .spacer {
    flex: 1;
  }
</style>
