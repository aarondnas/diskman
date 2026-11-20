<script lang="ts">
  // New versions of Diskman (src-tauri/src/updates.rs): the version running, the daily check
  // (Settings::check_updates), "Check now" and what it found, with the way to install it.
  import type { Config } from '../../api/types.ts';
  import { t } from '../../i18n/index.svelte.ts';
  import { config } from '../../stores/config.svelte.ts';
  import { checkNow, installUpdate, openWebsite, updates } from '../../stores/updates.svelte.ts';
  import { relativeTime } from '../../utils/format.ts';
  import SwitchOption from './SwitchOption.svelte';

  let { onupdate }: { onupdate: (mutate: (config: Config) => unknown) => Promise<boolean> } = $props();

  const status = $derived(updates.status);
  const auto = $derived(config.current?.settings?.check_updates !== false);
  let busy = $state(false);

  async function toggle(e: Event & { currentTarget: HTMLInputElement }) {
    // currentTarget is gone after the await
    const input = e.currentTarget;
    const on = input.checked;
    busy = true;
    const ok = await onupdate((c) => {
      c.settings.check_updates = on;
    });
    if (!ok) input.checked = !on;
    busy = false;
  }
</script>

<section>
  <h2>{t('settings.updates.title')}</h2>
  <p class="hint">{t('settings.updates.hint')}</p>

  <div class="state">
    <div class="text">
      <b>{t('settings.updates.current', { version: status?.current ?? '…' })}</b>
      <small class:err={!!status?.error}>
        {#if status?.checking}
          {t('settings.updates.checking')}
        {:else if status?.error}
          {t('settings.updates.failed', { message: status.error })}
        {:else if status?.available}
          {t('updates.available', { version: status.available.version, current: status.current })}
        {:else if status?.checked_at}
          {t('settings.updates.upToDate', { time: relativeTime(status.checked_at) })}
        {:else}
          {t('settings.updates.never')}
        {/if}
      </small>
    </div>
    {#if status?.available && !status.installing && status.lost !== 'untrusted'}
      {#if status.can_install}
        <button
          class="amber"
          onclick={installUpdate}
          disabled={updates.blocked}
          title={updates.blocked ? t('updates.blocked') : undefined}
        >
          {t('updates.install')}
        </button>
      {:else}
        <button class="amber" onclick={openWebsite}>{t('updates.download')}</button>
      {/if}
    {/if}
    <button onclick={checkNow} disabled={!status || status.checking || status.installing} aria-busy={status?.checking}>
      {t('settings.updates.checkNow')}
    </button>
  </div>
  <!-- Updates no longer come this way: the website has the new version (also when the warning at
       the top was hidden) -->
  {#if status?.lost}
    <div class="lost">
      <span>{t(`updates.lost.${status.lost}`)}</span>
      <button onclick={openWebsite}>{t('updates.lost.open')}</button>
    </div>
  {/if}
  {#if status?.available?.notes && status.lost !== 'untrusted'}
    <pre class="notes">{status.available.notes}</pre>
  {/if}
  {#if status && !status.can_install}
    <p class="note">{t('settings.updates.manual')}</p>
  {/if}

  <SwitchOption
    title={t('settings.updates.toggle')}
    sub={t(auto ? 'settings.updates.on' : 'settings.updates.off')}
    checked={auto}
    disabled={!config.current || busy}
    {busy}
    onchange={toggle}
  />
  <p class="note">{t('settings.updates.note')}</p>
</section>

<style>
  h2 {
    margin: 0 0 4px;
    font-size: 17px;
  }
  .hint {
    margin: 0 0 16px;
    max-width: 960px;
    color: var(--muted);
  }
  /* The version and what the last check found, with its buttons; a card like the switch below */
  .state {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px 12px;
    margin-bottom: 12px;
    padding: 12px 14px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
  }
  .text {
    flex: 1;
    min-width: 220px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  small {
    color: var(--muted);
    font-size: 12.5px;
  }
  small.err {
    color: var(--danger);
  }
  .lost {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px 12px;
    max-width: 960px;
    margin: 0 0 12px;
    padding: 10px 14px;
    border-radius: 10px;
    background: var(--warn-soft);
    color: var(--warn);
    font-size: 13px;
  }
  .lost span {
    flex: 1;
    min-width: 220px;
  }
  .notes {
    margin: 0 0 12px;
    padding: 10px 14px;
    max-height: 240px;
    max-width: 960px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
    font-family: inherit;
    font-size: 13px;
    white-space: pre-wrap;
  }
  .note {
    margin: 10px 0 0;
    max-width: 960px;
    font-size: 13px;
    color: var(--muted);
  }
</style>
