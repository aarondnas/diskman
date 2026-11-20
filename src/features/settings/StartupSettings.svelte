<script lang="ts">
  // Starting Diskman at login (stores/autostart.svelte.ts): a switch that reads and writes the
  // system's own entry, so a change in the system settings shows here too.
  import { onMount } from 'svelte';
  import { t } from '../../i18n/index.svelte.ts';
  import { autostart, loadAutostart, setAutostart } from '../../stores/autostart.svelte.ts';
  import SwitchOption from './SwitchOption.svelte';

  let busy = $state(false);

  // Read again when the tab opens: the entry may have been changed in the system meanwhile
  onMount(loadAutostart);

  async function toggle(e: Event & { currentTarget: HTMLInputElement }) {
    // currentTarget is gone after the await
    const input = e.currentTarget;
    busy = true;
    // On failure the switch goes back to what the system has
    if (!(await setAutostart(input.checked))) input.checked = autostart.enabled;
    busy = false;
  }
</script>

<section>
  <h2>{t('settings.startup.title')}</h2>
  <p class="hint">{t('settings.startup.hint')}</p>

  <SwitchOption
    title={t('settings.startup.toggle')}
    sub={t(autostart.enabled ? 'settings.startup.on' : 'settings.startup.off')}
    checked={autostart.enabled}
    disabled={!autostart.loaded || busy}
    {busy}
    onchange={toggle}
  />

  <ul class="points">
    <li>{t('settings.startup.pointBackground')}</li>
    <li>{t('settings.startup.pointCatchUp')}</li>
    <li>{t('settings.startup.pointSystem')}</li>
  </ul>
</section>

<style>
  h2 {
    margin: 0 0 4px;
    font-size: 17px;
  }
  .hint {
    margin: 0 0 16px;
    color: var(--muted);
  }
  .points {
    margin: 18px 0 0;
    padding-left: 18px;
    max-width: 960px;
    font-size: 13px;
    color: var(--muted);
    display: grid;
    gap: 6px;
  }
</style>
