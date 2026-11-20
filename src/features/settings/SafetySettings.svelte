<script lang="ts">
  // What protects the archives when a run is interrupted (always on, listed so it is known), and
  // the one choice: reading every copy back to compare it with its source (sync/executor.rs).
  import type { Config } from '../../api/types.ts';
  import { t } from '../../i18n/index.svelte.ts';
  import { config } from '../../stores/config.svelte.ts';
  import SwitchOption from './SwitchOption.svelte';

  let { onupdate }: { onupdate: (mutate: (config: Config) => unknown) => Promise<boolean> } = $props();

  const verify = $derived(!!config.current?.settings?.verify_copies);
  let busy = $state(false);

  async function toggle(e: Event & { currentTarget: HTMLInputElement }) {
    // currentTarget is gone after the await
    const input = e.currentTarget;
    const on = input.checked;
    busy = true;
    const ok = await onupdate((c) => {
      c.settings.verify_copies = on;
    });
    if (!ok) input.checked = !on;
    busy = false;
  }
</script>

<section>
  <h2>{t('settings.safety.title')}</h2>
  <p class="hint">{t('settings.safety.hint')}</p>

  <ul class="points">
    <li>{t('settings.safety.pointCopies')}</li>
    <li>{t('settings.safety.pointTrash')}</li>
    <li>{t('settings.safety.pointSource')}</li>
    <li>{t('settings.safety.pointPower')}</li>
    <li>{t('settings.safety.pointResume')}</li>
  </ul>

  <SwitchOption
    title={t('settings.safety.toggle')}
    sub={t(verify ? 'settings.safety.on' : 'settings.safety.off')}
    checked={verify}
    disabled={!config.current || busy}
    {busy}
    onchange={toggle}
  />
  <p class="note">{t('settings.safety.note')}</p>
</section>

<style>
  h2 {
    margin: 0 0 4px;
    font-size: 17px;
  }
  .hint {
    margin: 0 0 12px;
    color: var(--muted);
  }
  .points {
    margin: 0 0 20px;
    padding-left: 18px;
    max-width: 960px;
    font-size: 13px;
    color: var(--muted);
    display: grid;
    gap: 6px;
  }
  .note {
    margin: 10px 0 0;
    max-width: 960px;
    font-size: 13px;
    color: var(--muted);
  }
</style>
