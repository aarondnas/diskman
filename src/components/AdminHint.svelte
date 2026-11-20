<script lang="ts">
  // Hint above a list of entries that could not be read for lack of permission, with the button
  // that restarts Diskman with administrator rights. Only where that is possible (Windows, not yet
  // elevated); the backend then marks such entries with `admin`.
  import { t } from '../i18n/index.svelte.ts';
  import { admin, restartAsAdmin } from '../stores/admin.svelte.ts';
  import { platform } from '../stores/platform.svelte.ts';
  import { ICONS } from '../utils/icons.ts';

  // count: entries that administrator rights could probably make readable
  let { count }: { count: number } = $props();
</script>

{#if count > 0 && platform.canElevate}
  <div class="hint" role="note">
    <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true"><path fill="currentColor" d={ICONS.shield} /></svg>
    <span>{t('issues.admin.count', { count })}</span>
    <button class="amber" onclick={restartAsAdmin} disabled={admin.restarting} aria-busy={admin.restarting}>
      {admin.restarting ? t('issues.admin.restarting') : t('issues.admin.restart')}
    </button>
  </div>
{/if}

<style>
  .hint {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 8px 8px 12px;
    border-radius: var(--radius);
    background: var(--warn-soft);
    color: var(--warn);
    font-size: 13px;
  }
  svg {
    flex: none;
  }
  span {
    flex: 1;
    min-width: 0;
  }
  button {
    flex: none;
    white-space: nowrap;
  }
</style>
