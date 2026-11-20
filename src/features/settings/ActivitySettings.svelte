<script lang="ts">
  // How much room the file lists of the activities may take in the history (Settings::activity_limit_mb,
  // src-tauri/src/storage/plan_store.rs), what they take now, and deleting them all.
  import { onMount } from 'svelte';
  import { backend } from '../../api/backend.ts';
  import type { Config, PlanStorage } from '../../api/types.ts';
  import { t } from '../../i18n/index.svelte.ts';
  import { config } from '../../stores/config.svelte.ts';
  import { confirm } from '../../stores/confirm.svelte.ts';
  import { showError } from '../../stores/notice.svelte.ts';
  import { formatBytes, formatCount } from '../../utils/format.ts';

  let { onupdate }: { onupdate: (mutate: (config: Config) => unknown) => Promise<boolean> } = $props();

  /** The choices, in MB (the backend accepts 64 MB … 100 GB). */
  const LIMITS = [256, 512, 1024, 2048, 5120, 10240];
  const DEFAULT = 1024;

  const limit = $derived(config.current?.settings?.activity_limit_mb ?? DEFAULT);
  // A limit set by hand in config.json stays selectable
  const choices = $derived(LIMITS.includes(limit) ? LIMITS : [...LIMITS, limit].sort((a, b) => a - b));
  let storage = $state<PlanStorage | null>(null);
  let busy = $state(false);

  async function loadStorage() {
    try {
      storage = await backend.activityStorage();
    } catch (e) {
      showError(e);
    }
  }
  onMount(loadStorage);

  async function choose(e: Event & { currentTarget: HTMLSelectElement }) {
    const select = e.currentTarget;
    const mb = Number(select.value);
    busy = true;
    const ok = await onupdate((c) => {
      c.settings.activity_limit_mb = mb;
    });
    if (!ok) select.value = String(limit);
    busy = false;
    // A smaller limit removes the oldest lists right away
    await loadStorage();
  }

  async function clear() {
    if (!(await confirm(t('settings.activity.clearConfirm'), t('settings.activity.clearOk')))) return;
    busy = true;
    try {
      storage = await backend.clearActivityPlans();
    } catch (e) {
      showError(e);
    } finally {
      busy = false;
    }
  }
</script>

<section>
  <h2>{t('settings.activity.title')}</h2>
  <p class="hint">{t('settings.activity.hint')}</p>

  <div class="row">
    <label for="activity-limit">{t('settings.activity.limit')}</label>
    <select id="activity-limit" value={String(limit)} onchange={choose} disabled={!config.current || busy}>
      {#each choices as mb (mb)}
        <option value={String(mb)}>{formatBytes(mb * 1024 * 1024)}</option>
      {/each}
    </select>
  </div>

  <div class="row">
    <span class="muted">
      {#if storage}
        {t('settings.activity.used', {
          size: formatBytes(storage.bytes),
          lists: t('settings.activity.lists', { count: storage.plans, n: formatCount(storage.plans) }),
        })}
      {:else}
        &nbsp;
      {/if}
    </span>
    <button class="danger" onclick={clear} disabled={busy || !storage?.plans}>{t('settings.activity.clear')}</button>
  </div>
  <p class="note">{t('settings.activity.note')}</p>
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
  .row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 12px;
    margin-bottom: 12px;
    max-width: 640px;
  }
  .row > :first-child {
    flex: 1;
    min-width: 200px;
  }
  select {
    min-width: 140px;
  }
  .note {
    margin: 10px 0 0;
    max-width: 960px;
    font-size: 13px;
    color: var(--muted);
  }
</style>
