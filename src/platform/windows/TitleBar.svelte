<script lang="ts">
  // Windows only: title bar across the top of the window, app name in the middle, the window
  // buttons on the right. On Windows the window has no native frame (tauri.windows.conf.json:
  // decorations: false), so this bar replaces it: its empty parts drag the window, a double click
  // maximizes. The window buttons replicate the Windows 11 caption buttons (glyphs of Segoe Fluent
  // Icons, red close). On Linux the desktop draws the native frame instead.
  import { onMount } from 'svelte';
  import { appWindow as win } from '../../api/appWindow.ts';
  import Wordmark from '../../components/Wordmark.svelte';
  import { t } from '../../i18n/index.svelte.ts';
  import { platform } from '../../stores/platform.svelte.ts';
  import { ICONS } from '../../utils/icons.ts';

  let maximized = $state(false);

  onMount(() => {
    const sync = async () => {
      try {
        maximized = await win.isMaximized();
      } catch {
        // Without an answer the last known glyph stays
      }
    };
    sync();
    const unlisten = win.onResized(sync);
    return () => unlisten.then((stop) => stop());
  });
</script>

<!-- data-tauri-drag-region only works on the element that carries it, so every empty part has it -->
<header class="topbar" data-tauri-drag-region>
  <!-- Running with administrator rights (restarted for unreadable files): always in sight -->
  {#if platform.elevated}
    <span class="elevated" title={t('issues.admin.activeHint')}>
      <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path fill="currentColor" d={ICONS.shield} /></svg>
      {t('issues.admin.active')}
    </span>
  {/if}
  <!-- Empty space up to the buttons, so all of it drags the window -->
  <div class="fill" data-tauri-drag-region></div>
  <!-- Lettering in the middle of the whole bar; it lets clicks through (pointer-events: none), so
       it drags the window too -->
  <span class="brand"><Wordmark /></span>

  <div class="controls">
    <button class="control" title={t('titlebar.minimize')} aria-label={t('titlebar.minimize')} onclick={() => win.minimize()}>
      &#xE921;
    </button>
    <button
      class="control"
      title={t(maximized ? 'titlebar.restore' : 'titlebar.maximize')}
      aria-label={t(maximized ? 'titlebar.restore' : 'titlebar.maximize')}
      onclick={() => win.toggleMaximize()}
    >
      {maximized ? '' : ''}
    </button>
    <button class="control close" title={t('titlebar.close')} aria-label={t('titlebar.close')} onclick={() => win.close()}>
      &#xE8BB;
    </button>
  </div>
</header>

<style>
  .topbar {
    position: sticky;
    top: 0;
    z-index: 20;
    display: flex;
    align-items: stretch;
    height: 46px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    user-select: none;
  }
  .fill {
    flex: 1;
    min-width: 0;
  }
  /* Lets clicks through like the lettering, so it drags the window too */
  .elevated {
    display: flex;
    align-items: center;
    gap: 5px;
    margin-left: 14px;
    font-size: 12px;
    font-weight: 600;
    color: var(--warn);
    pointer-events: none;
  }
  /* Centred on the bar, not on the space left of the buttons */
  .brand {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    display: flex;
    pointer-events: none;
  }
  /* Window buttons on the right edge */
  .controls {
    flex: none;
    display: flex;
    align-items: stretch;
  }
  /* Width, glyphs and colors as the Windows 11 caption buttons, stretched to the bar's height */
  .control {
    width: 46px;
    min-height: 0;
    padding: 0;
    border: none;
    border-radius: 0;
    background: transparent;
    /* Grey like the other buttons of the bar, full text color on hover */
    color: var(--muted);
    font-family: 'Segoe Fluent Icons', 'Segoe MDL2 Assets', sans-serif;
    font-size: 10px;
    transition: none;
    cursor: default;
  }
  .control:hover:not(:disabled) {
    border-color: transparent;
    background: var(--caption-hover);
    color: var(--text);
  }
  .control:active {
    background: var(--caption-press);
  }
  .control.close:hover:not(:disabled) {
    background: #c42b1c;
    color: #ffffff;
  }
  .control.close:active {
    background: #c84031;
    color: #ffffff;
  }
  .control:focus-visible {
    outline-offset: -2px;
  }
</style>
