<script lang="ts">
  import { onMount } from 'svelte';
  import { appWindow } from '../../api/appWindow.ts';
  import { backend } from '../../api/backend.ts';
  import { t, type TextKey } from '../../i18n/index.svelte.ts';
  import appIcon from '../../../src-tauri/icons/icon.svg';
  // Name and version are taken from the Tauri config when the app is built, so they are there on
  // the first render; asking Tauri at runtime made the header grow and the page jump
  import { productName as name, version } from '../../../src-tauri/tauri.conf.json';
  import Wordmark from '../../components/Wordmark.svelte';
  import { IS_WINDOWS } from '../../platform/index.ts';
  import { platform } from '../../stores/platform.svelte.ts';
  import { ICONS } from '../../utils/icons.ts';

  let tauriVersion = $state('');
  let dataDir = $state('');
  let copied = $state(false);

  onMount(() => {
    // Each value on its own: one missing answer must not hide the others
    appWindow.tauriVersion().then((v) => (tauriVersion = v), () => {});
    backend.dataDir().then((v) => (dataDir = v), () => {});
  });

  async function copyDataDir() {
    try {
      await navigator.clipboard.writeText(dataDir);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // Clipboard not available: the path stays selectable
    }
  }

  // Title and text are text keys. Source and archive with the words and symbols of the folder
  // tiles in the editor (FolderTile.svelte), so both pages name them alike.
  const concepts: { title: TextKey; text: TextKey; icon: string }[] = [
    {
      title: 'card.source',
      text: 'info.concepts.sourceText',
      icon: ICONS.folder,
    },
    {
      title: 'card.archive',
      text: 'info.concepts.archiveText',
      icon: ICONS.archive,
    },
    {
      // The symbols of the two modes as in the editor (ModeFlow.svelte)
      title: 'common.mirror',
      text: 'info.concepts.mirrorText',
      icon: ICONS.mirror,
    },
    {
      title: 'common.additive',
      text: 'info.concepts.additiveText',
      icon: ICONS.plus,
    },
  ];

  // info.feature.<id> and info.feature.<id>Text, in the order a backup goes through them
  const features = ['analysis', 'issues', 'runNow', 'live', 'scheduler', 'moves', 'activity', 'updates'] as const;

  // Text keys of "Good to know"; texts that differ per system pick theirs (i18n: { windows, linux })
  const notes: TextKey[] = [
    'info.tray',
    'info.noDelete',
    'info.mirrorClash',
    'info.permissions',
    'info.links',
    'info.fatDrives',
  ];

  // Product names, the same in every language
  const systemName = IS_WINDOWS ? 'Windows' : 'Linux';
</script>

<div class="info">
  <section class="hero">
    <!-- Logo at 64 px = 4 px per logo pixel, the lettering in the same pixel size -->
    <img src={appIcon} alt="" width="64" height="64" />
    <div>
      <h2><Wordmark size={4} /></h2>
      <p class="tagline">{t('info.tagline')}</p>
      <div class="badges">
        <span class="badge">{t('info.version', { version })}</span>
        <!-- The system this build is for; administrator rights when Diskman runs with them -->
        <span class="badge system">
          <svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path fill="currentColor" d={ICONS.monitor} /></svg>
          {systemName}
        </span>
        {#if platform.elevated}
          <span class="badge admin" title={t('issues.admin.activeHint')}>
            <svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path fill="currentColor" d={ICONS.shield} /></svg>
            {t('issues.admin.active')}
          </span>
        {/if}
      </div>
    </div>
  </section>

  <section>
    <h3>{t('info.howItWorks')}</h3>
    <div class="concepts">
      {#each concepts as c (c.title)}
        <div class="concept">
          <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true"><path fill="currentColor" d={c.icon} /></svg>
          <div>
            <b>{t(c.title)}</b>
            <p>{t(c.text)}</p>
          </div>
        </div>
      {/each}
    </div>
  </section>

  <section>
    <h3>{t('info.features')}</h3>
    <dl class="features">
      {#each features as id (id)}
        <dt>{t(`info.feature.${id}`)}</dt>
        <dd>{t(`info.feature.${id}Text`)}</dd>
      {/each}
    </dl>
  </section>

  <section>
    <h3>{t('info.goodToKnow')}</h3>
    <ul class="notes">
      {#each notes as key (key)}
        <li>{t(key, { name })}</li>
      {/each}
    </ul>
  </section>

  <section>
    <h3>{t('info.data')}</h3>
    <p class="muted">{t('info.dataIntro')}</p>
    <div class="path">
      <code>{dataDir || '…'}</code>
      <button class="ghost" onclick={copyDataDir} disabled={!dataDir}>{t(copied ? 'info.copied' : 'info.copy')}</button>
    </div>
    <!-- Own text with <code> tags, no user input -->
    <p class="muted small">{@html t('info.dataFiles', { name })}</p>
  </section>

  <footer class="muted small">
    {t('info.builtWith', { tauri: tauriVersion ? `Tauri ${tauriVersion}` : 'Tauri' })}
  </footer>
</div>

<style>
  /* As wide as the page; the lists below get more columns when there is room */
  .info {
    container: info / inline-size;
  }
  section {
    margin-bottom: 28px;
  }
  h3 {
    margin: 0 0 10px;
    font-size: 13px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
  }
  .hero {
    display: flex;
    align-items: center;
    gap: 20px;
    padding: 22px 24px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: var(--shadow);
  }
  .hero img {
    flex: none;
    image-rendering: pixelated;
  }
  .hero h2 {
    margin: 0 0 10px;
  }
  .tagline {
    margin: 0 0 10px;
    color: var(--muted);
  }
  .badges {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 10px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent-text);
    font-size: 12px;
    font-weight: 600;
  }
  .badge.system {
    background: var(--surface-2);
    border: 1px solid var(--border);
    color: var(--muted);
  }
  .badge.admin {
    background: var(--warn-soft);
    color: var(--warn);
  }
  .concepts {
    display: grid;
    /* auto-fit: the four cards share the full width instead of leaving empty columns */
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: 10px;
  }
  .concept {
    display: flex;
    gap: 12px;
    padding: 14px 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
  }
  .concept svg {
    flex: none;
    color: var(--accent-text);
    margin-top: 1px;
  }
  .concept p {
    margin: 2px 0 0;
    color: var(--muted);
    font-size: 14px;
  }
  .features {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 8px 20px;
    margin: 0;
    padding: 16px 18px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
  }
  .features dt {
    font-weight: 600;
  }
  .features dd {
    margin: 0;
    color: var(--muted);
  }
  .notes {
    margin: 0;
    padding-left: 20px;
  }
  .notes li {
    margin-bottom: 6px;
    break-inside: avoid;
  }
  /* Wide: two features per row (dt and dd fill the four columns in turn), notes in two columns */
  @container info (min-width: 1100px) {
    .features {
      grid-template-columns: max-content 1fr max-content 1fr;
      column-gap: 20px;
    }
    .features dd:nth-of-type(odd) {
      margin-right: 16px;
    }
    .notes {
      columns: 2;
      column-gap: 48px;
    }
  }
  .path {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 6px 0 8px;
  }
  .path code {
    flex: 1;
    min-width: 0;
    padding: 7px 10px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow-wrap: anywhere;
    user-select: all;
  }
  code {
    font-family: var(--mono);
    font-size: 13px;
  }
  p {
    margin: 0;
  }
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: 13px;
  }
</style>
