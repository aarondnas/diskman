<script lang="ts">
  // Light or dark theme (utils/theme.ts): dark by default, whatever the system's mode.
  import type { Config } from '../../api/types.ts';
  import { t } from '../../i18n/index.svelte.ts';
  import { config } from '../../stores/config.svelte.ts';
  import { applyTheme } from '../../utils/theme.ts';
  import SwitchOption from './SwitchOption.svelte';

  let { onupdate }: { onupdate: (mutate: (config: Config) => unknown) => Promise<boolean> } = $props();

  const light = $derived(!!config.current?.settings?.light_mode);
  let busy = $state(false);

  /** Switches at once; the choice is stored in the config and reverted if saving fails. */
  async function toggle(e: Event & { currentTarget: HTMLInputElement }) {
    // currentTarget is gone after the await
    const input = e.currentTarget;
    const on = input.checked;
    busy = true;
    applyTheme(on);
    const ok = await onupdate((c) => {
      c.settings.light_mode = on;
    });
    if (!ok) {
      applyTheme(!on);
      input.checked = !on;
    }
    busy = false;
  }
</script>

<section>
  <h2>{t('settings.appearance.title')}</h2>
  <p class="hint">{t('settings.appearance.hint')}</p>

  <SwitchOption
    title={t('settings.appearance.toggle')}
    sub={t(light ? 'settings.appearance.on' : 'settings.appearance.off')}
    checked={light}
    disabled={!config.current || busy}
    {busy}
    onchange={toggle}
  />
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
</style>
