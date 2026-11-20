<script lang="ts">
  // Schedule of one backup, as a dialog over the backup list (opened with the clock button on its
  // card): at a time of day on chosen weekdays, or every n minutes/hours. The backend runs it,
  // also while the window is closed (src-tauri/src/jobs/schedules.rs).

  // With `targets` instead of `backup` (the "schedule all" button of the toolbar) it sets one
  // schedule for several backups at once: they are chosen as capsules, the form starts empty, and
  // saving writes a copy into each of them. Nothing stays linked; each can be changed on its own.

  // backup: the one backup; targets: ids chosen at the start, choices: every backup that can be added
  // onsave(schedule | null, ids?): saves the schedule (null removes it), returns true on success
  import { fade, scale } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import type { Backup, BackupSchedule, Id, Schedule } from '../../api/types.ts';
  import DiskmanHead from '../../components/DiskmanHead.svelte';
  import { confirm, pending as confirmPending } from '../../stores/confirm.svelte.ts';
  import { t } from '../../i18n/index.svelte.ts';
  import { weekdayShort } from '../../utils/format.ts';
  import { focusTrap } from '../../utils/focusTrap.ts';
  import { CROSS, ICONS } from '../../utils/icons.ts';
  import { describe, stateText } from '../../utils/scheduleText.ts';
  import { visorOf } from '../../utils/visor.ts';

  interface Props {
    backup?: Backup | null;
    targets?: Id[] | null;
    choices?: Backup[];
    onsave: (schedule: BackupSchedule | null, ids?: Id[]) => Promise<boolean>;
    oncancel: () => void;
  }
  let { backup = null, targets = null, choices = [], onsave, oncancel }: Props = $props();

  // svelte-ignore state_referenced_locally
  const many = !!targets;
  // Chosen backups in "schedule all"; the capsules follow the order of the list
  // svelte-ignore state_referenced_locally
  let picked = $state([...(targets ?? [])]);
  const selected = $derived(choices.filter((b) => picked.includes(b.id)));
  const addable = $derived(choices.filter((b) => !picked.includes(b.id)));
  // Chosen backups that already have a schedule: saving replaces it
  const replaced = $derived(selected.filter((b) => b.schedule));

  function pick(e: Event & { currentTarget: HTMLSelectElement }) {
    const id = e.currentTarget.value;
    picked = id === '*' ? choices.map((b) => b.id) : [...picked, id];
    // Back to the "Add" line, so the same choice can be made again later
    e.currentTarget.value = '';
  }

  // 1 = Monday … 7 = Sunday; names come from the language via Intl
  const DAYS = [1, 2, 3, 4, 5, 6, 7];

  // The form keeps both kinds, so switching back and forth loses nothing; the interval is
  // entered as amount + unit (minutes or hours)
  interface Form {
    kind: Schedule['kind'];
    time: string;
    days: number[];
    /** The interval in `unit`s. */
    amount: number;
    /** Minutes per unit: 1 or 60. */
    unit: number;
    enabled: boolean;
    catchUp: boolean;
  }

  function formOf(schedule: BackupSchedule | null | undefined): Form {
    const w = schedule?.when;
    const hours = w?.kind === 'interval' && w.minutes % 60 === 0;
    return {
      kind: w?.kind ?? 'daily',
      time: w?.kind === 'daily' ? w.time : '02:00',
      // No weekday chosen in advance: the user picks them
      days: w?.kind === 'daily' ? [...w.days] : [],
      amount: w?.kind === 'interval' ? (hours ? w.minutes / 60 : w.minutes) : 1,
      unit: w?.kind === 'interval' && !hours ? 1 : 60,
      enabled: schedule?.enabled ?? true,
      // Off unless chosen: logging the schedule is only needed to catch up missed runs
      catchUp: schedule?.catch_up ?? false,
    };
  }

  // Set once when the dialog opens; later changes to the backup do not reset the form
  // svelte-ignore state_referenced_locally
  const isNew = many || !backup?.schedule;
  // svelte-ignore state_referenced_locally
  let form = $state(formOf(many ? null : backup?.schedule));
  let saving = $state(false);

  const when = $derived<Schedule>(
    form.kind === 'interval'
      ? { kind: 'interval', minutes: Math.round(Number(form.amount) * Number(form.unit)) }
      : { kind: 'daily', time: form.time, days: [...form.days].sort((a, b) => a - b) },
  );
  const valid = $derived(
    when.kind === 'interval'
      ? Number(form.amount) >= 1 && when.minutes <= 7 * 24 * 60
      : /^\d{2}:\d{2}$/.test(form.time) && form.days.length > 0,
  );

  function toggleDay(day: number) {
    form.days = form.days.includes(day) ? form.days.filter((d) => d !== day) : [...form.days, day];
  }

  async function save() {
    saving = true;
    try {
      await onsave(
        { when: $state.snapshot(when), enabled: form.enabled, catch_up: form.catchUp },
        many ? $state.snapshot(picked) : undefined,
      );
    } finally {
      saving = false;
    }
  }

  async function remove() {
    if (many) {
      const ids = replaced.map((b) => b.id);
      if (await confirm(t('scheduler.deleteManyConfirm', { count: ids.length }), t('common.delete'))) onsave(null, ids);
      return;
    }
    if (backup && (await confirm(t('scheduler.deleteConfirm', { name: backup.name }), t('common.delete')))) onsave(null);
  }

  function onkeydown(e: KeyboardEvent) {
    // A question on top (delete?) handles its own keys
    if (confirmPending.current) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      oncancel();
    }
  }
</script>

<svelte:window {onkeydown} />

<div
  class="backdrop"
  role="presentation"
  transition:fade={{ duration: 120 }}
  onclick={(e) => e.target === e.currentTarget && oncancel()}
>
  <!-- Focus starts on the chosen timing and goes back to the clock button when it closes -->
  <div
    use:focusTrap={'button.active'}
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="schedule-title"
    transition:scale={{ duration: 140, start: 0.96, easing: cubicOut }}
  >
    <header>
      <span class="badge" aria-hidden="true">
        <svg viewBox="0 0 24 24" width="22" height="22">
          <path fill="currentColor" d={ICONS.clock} />
        </svg>
      </span>
      <div class="titles">
        {#if many}
          <span class="eyebrow">{t('scheduler.manyTitle', { count: picked.length })}</span>
          <h2 id="schedule-title">{t('scheduler.many')}</h2>
        {:else}
          <span class="eyebrow">{t(isNew ? 'scheduler.newSchedule' : 'scheduler.editSchedule')}</span>
          <h2 id="schedule-title">{backup?.name}</h2>
        {/if}
      </div>
      <button class="close" title={t('common.close')} aria-label={t('common.close')} onclick={oncancel}>
        <svg viewBox="0 0 10 10" width="10" height="10" aria-hidden="true">
          <path d={CROSS} stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
        </svg>
      </button>
    </header>

    <div class="body">
      {#if many}
        <!-- The backups the schedule is written into, as capsules; more are added from the list -->
        <div class="field">
          <span class="label">{t('scheduler.appliesTo')}</span>
          <div class="targets">
            {#each selected as b (b.id)}
              <span class="target" title={b.schedule ? t('scheduler.hasSchedule', { when: describe(b.schedule.when) }) : b.name}>
                <DiskmanHead visor={visorOf(b)} size={16} />
                <span class="tname">{b.name}</span>
                {#if b.schedule}
                  <svg class="has" viewBox="0 0 24 24" width="12" height="12" aria-hidden="true"><path fill="currentColor" d={ICONS.clock} /></svg>
                {/if}
                <button
                  class="unpick"
                  title={t('scheduler.unpick', { name: b.name })}
                  aria-label={t('scheduler.unpick', { name: b.name })}
                  onclick={() => (picked = picked.filter((id) => id !== b.id))}
                >
                  <svg viewBox="0 0 10 10" width="8" height="8" aria-hidden="true">
                    <path d={CROSS} stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
                  </svg>
                </button>
              </span>
            {/each}
            {#if addable.length}
              <select class="pick" value="" onchange={pick} aria-label={t('scheduler.addBackup')}>
                <option value="" disabled>+ {t('scheduler.addBackup')}</option>
                {#if addable.length > 1}
                  <option value="*">{t('scheduler.addAll')}</option>
                {/if}
                {#each addable as b (b.id)}
                  <option value={b.id}>{b.name}</option>
                {/each}
              </select>
            {/if}
          </div>
          {#if !picked.length}
            <span class="muted hint">{t('scheduler.noneSelected')}</span>
          {:else if replaced.length}
            <span class="muted hint">{t('scheduler.replaces', { count: replaced.length })}</span>
          {/if}
        </div>
      {/if}

      <div class="field">
        <span class="label">{t('scheduler.timing')}</span>
        <div class="segmented">
          <button class:active={form.kind === 'daily'} aria-pressed={form.kind === 'daily'} onclick={() => (form.kind = 'daily')}>
            {t('scheduler.atTimeOfDay')}
          </button>
          <button class:active={form.kind === 'interval'} aria-pressed={form.kind === 'interval'} onclick={() => (form.kind = 'interval')}>
            {t('scheduler.atInterval')}
          </button>
        </div>
        {#if form.kind === 'daily'}
          <div class="when">
            <input type="time" bind:value={form.time} aria-label={t('scheduler.time')} />
            <div class="days" role="group" aria-label={t('scheduler.weekdays')}>
              {#each DAYS as day (day)}
                <button class="day" class:on={form.days.includes(day)} aria-pressed={form.days.includes(day)} onclick={() => toggleDay(day)}>
                  {weekdayShort(day)}
                </button>
              {/each}
            </div>
          </div>
        {:else}
          <div class="when">
            <span>{t('scheduler.every')}</span>
            <input class="amount" type="number" min="1" bind:value={form.amount} aria-label={t('scheduler.interval')} />
            <select bind:value={form.unit} aria-label={t('scheduler.unit')}>
              <option value={1}>{t('scheduler.minutes')}</option>
              <option value={60}>{t('scheduler.hours')}</option>
            </select>
          </div>
        {/if}
      </div>

      <!-- Can only be switched on once the timing is complete (time + weekday, or an interval);
           until then it shows as off, the choice in form.enabled is kept for when it is -->
      <label class="check" class:disabled={!valid}>
        <input
          type="checkbox"
          checked={valid && form.enabled}
          disabled={!valid}
          onchange={(e) => (form.enabled = e.currentTarget.checked)}
        />
        {t('scheduler.enabled')}
      </label>

      <!-- Logs the schedule, so that runs missed while Diskman was off are offered on the next start
           (src-tauri/src/jobs/catch_up.rs, MissedRunsDialog.svelte) -->
      <label class="check catch-up">
        <input type="checkbox" bind:checked={form.catchUp} aria-describedby="catch-up-hint" />
        <span>
          {t('scheduler.catchUp')}
          <span id="catch-up-hint" class="muted hint">{t('scheduler.catchUpHint')}</span>
        </span>
      </label>

      <!-- What the schedule will do, in words; always there, so the dialog does not change height -->
      <p class="summary" class:invalid={!valid}>
        {#if valid}
          <b>{describe(when)}</b> · {stateText({ when, enabled: form.enabled })}
        {:else}
          {t(form.kind === 'daily' ? 'scheduler.incompleteDaily' : 'scheduler.incompleteInterval')}
        {/if}
      </p>

      {#if many}
        <p class="muted info">{t('scheduler.manyInfo')}</p>
      {/if}
      <p class="muted info">{t('scheduler.info')}</p>
    </div>

    <footer>
      {#if many ? replaced.length : !isNew}
        <button class="danger" onclick={remove}>{many ? t('scheduler.deleteMany') : t('common.delete')}</button>
      {/if}
      <span class="spacer"></span>
      <button class="ghost" onclick={oncancel}>{t('common.cancel')}</button>
      <button class="primary" disabled={!valid || saving || (many && !picked.length)} onclick={save}>
        {many ? t('scheduler.apply') : isNew ? t('scheduler.create') : t('common.save')}
      </button>
    </footer>
  </div>
</div>

<style>
  /* Above the top bar, below the delete question (ConfirmDialog.svelte, z-index 30) */
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
    width: min(520px, 100%);
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
  .badge {
    flex: none;
    display: grid;
    place-items: center;
    width: 42px;
    height: 42px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--amber) 22%, var(--surface));
    color: color-mix(in srgb, var(--amber) 70%, var(--text));
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
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
    gap: 18px;
    padding: 4px 22px 20px;
    overflow-y: auto;
  }
  .field {
    display: grid;
    gap: 10px;
    justify-items: start;
  }
  .label {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .when {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  input[type='time'],
  input[type='number'] {
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 6px 10px;
    min-height: 34px;
    font: inherit;
    color: inherit;
  }
  .amount {
    width: 80px;
  }
  .days {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .day {
    min-width: 40px;
    padding: 4px 8px;
    min-height: 32px;
    font-size: 13px;
    color: var(--muted);
  }
  .day.on {
    background: color-mix(in srgb, var(--amber) 22%, var(--surface));
    border-color: var(--amber);
    color: var(--text);
    font-weight: 600;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    justify-self: start;
    cursor: pointer;
  }
  .check.disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }
  /* Checkbox beside a title with an explanation below it */
  .catch-up {
    align-items: flex-start;
    margin-top: -8px;
  }
  .catch-up input {
    margin-top: 3px;
  }
  .catch-up .hint {
    display: block;
  }
  .summary {
    margin: 0;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: var(--surface-2);
    border: 1px solid var(--border-soft);
    font-size: 13px;
  }
  .summary.invalid {
    color: var(--muted);
  }
  .info {
    margin: 0;
    font-size: 12px;
  }
  /* Chosen backups as capsules, each with its rack, its name and a cross to take it out */
  .targets {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .target {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: 100%;
    min-height: 28px;
    padding: 2px 3px 2px 8px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface-2);
    font-size: 13px;
  }
  .tname {
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Has a schedule already, which saving replaces */
  .has {
    flex: none;
    color: var(--amber);
  }
  .unpick {
    flex: none;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    min-height: 0;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--muted);
  }
  .unpick:hover:not(:disabled) {
    background: var(--danger);
    color: white;
  }
  /* "Add" as a dashed capsule, like the empty card that starts a new backup */
  .pick {
    min-height: 28px;
    padding: 2px 10px;
    border: 1px dashed var(--border);
    border-radius: 999px;
    background: transparent;
    color: var(--muted);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }
  .pick:hover {
    border-color: var(--amber);
    color: var(--text);
  }
  .hint {
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
