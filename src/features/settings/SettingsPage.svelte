<script lang="ts">
  // Settings with one tab per area. A new area: write its component in this folder,
  // add an entry to TABS (text key settings.tabs.<id>) and render it below.
  import type { Config } from '../../api/types.ts';
  import { t } from '../../i18n/index.svelte.ts';
  import { ICONS } from '../../utils/icons.ts';
  import { loadPreference, savePreference } from '../../utils/storage.ts';
  import ActivitySettings from './ActivitySettings.svelte';
  import AppearanceSettings from './AppearanceSettings.svelte';
  import LanguageSettings from './LanguageSettings.svelte';
  import SafetySettings from './SafetySettings.svelte';
  import StartupSettings from './StartupSettings.svelte';
  import UpdateSettings from './UpdateSettings.svelte';

  // onupdate(mutate): saves a change to the config, returns true on success
  let { onupdate }: { onupdate: (mutate: (config: Config) => unknown) => Promise<boolean> } = $props();

  type TabId = 'language' | 'appearance' | 'startup' | 'safety' | 'activity' | 'updates';
  const TABS: { id: TabId; icon: string }[] = [
    {
      id: 'language',
      icon: ICONS.language,
    },
    {
      id: 'appearance',
      icon: ICONS.palette,
    },
    {
      id: 'startup',
      icon: ICONS.power,
    },
    {
      id: 'safety',
      icon: ICONS.shield,
    },
    {
      id: 'activity',
      icon: ICONS.history,
    },
    {
      id: 'updates',
      icon: ICONS.update,
    },
  ];

  // The last open tab is remembered
  const STORAGE_KEY = 'settingsTab';
  let active = $state(
    loadPreference(STORAGE_KEY, TABS[0].id, (id): id is TabId => TABS.some((tab) => tab.id === id)),
  );

  function select(id: TabId) {
    active = id;
    savePreference(STORAGE_KEY, id);
  }

  /** Arrow keys move between tabs, as usual for a tab list. */
  function onkeydown(e: KeyboardEvent) {
    const step = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const i = TABS.findIndex((tab) => tab.id === active);
    const next = TABS[(i + step + TABS.length) % TABS.length].id;
    select(next);
    document.getElementById(`settings-tab-${next}`)?.focus();
  }
</script>

<div>
  <div class="tabs" role="tablist" aria-label={t('nav.settings')} tabindex="-1" {onkeydown}>
    {#each TABS as tab (tab.id)}
      <button
        id="settings-tab-{tab.id}"
        class="tab"
        class:active={tab.id === active}
        role="tab"
        aria-selected={tab.id === active}
        aria-controls="settings-panel"
        tabindex={tab.id === active ? 0 : -1}
        onclick={() => select(tab.id)}
      >
        <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d={tab.icon} /></svg>
        {t(`settings.tabs.${tab.id}`)}
      </button>
    {/each}
  </div>

  <div id="settings-panel" class="panel" role="tabpanel" aria-labelledby="settings-tab-{active}">
    {#if active === 'language'}
      <LanguageSettings {onupdate} />
    {:else if active === 'appearance'}
      <AppearanceSettings {onupdate} />
    {:else if active === 'startup'}
      <StartupSettings />
    {:else if active === 'safety'}
      <SafetySettings {onupdate} />
    {:else if active === 'activity'}
      <ActivitySettings {onupdate} />
    {:else if active === 'updates'}
      <UpdateSettings {onupdate} />
    {/if}
  </div>
</div>

<style>
  .tabs {
    display: flex;
    gap: 4px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 24px;
  }
  .tab {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    margin-bottom: -1px;
    padding: 8px 14px;
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: var(--radius) var(--radius) 0 0;
    background: transparent;
    color: var(--muted);
  }
  .tab:hover:not(:disabled):not(.active) {
    border-color: transparent;
    background: var(--surface-2);
    color: var(--text);
  }
  .tab.active {
    border-bottom-color: var(--amber);
    color: var(--text);
    font-weight: 600;
  }
  .tab:focus-visible {
    outline-offset: -2px;
  }
</style>
