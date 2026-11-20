<script lang="ts">
  import type { Config } from '../../api/types.ts';
  import {
    i18n,
    LANGUAGES,
    setLanguage,
    SYSTEM,
    systemLanguage,
    t,
    type Language,
    type LanguagePreference,
  } from '../../i18n/index.svelte.ts';
  import { ICONS } from '../../utils/icons.ts';

  let { onupdate }: { onupdate: (mutate: (config: Config) => unknown) => Promise<boolean> } = $props();

  const current = $derived(i18n.preference);
  const nativeName = (code: Language) => LANGUAGES.find((l) => l.code === code)?.name ?? code;

  /** Name of a language in the language currently shown, e.g. "Französisch" for fr in German. */
  function localName(code: Language) {
    try {
      const name = new Intl.DisplayNames([i18n.locale], { type: 'language' }).of(code);
      return name ? name[0].toLocaleUpperCase(i18n.locale) + name.slice(1) : '';
    } catch {
      return '';
    }
  }

  /** Switches at once; the choice is stored in the config and reverted if saving fails. */
  async function choose(pref: LanguagePreference) {
    const previous = i18n.preference;
    if (pref === previous) return;
    setLanguage(pref);
    const ok = await onupdate((c) => {
      c.settings.language = pref === SYSTEM ? null : pref;
    });
    if (!ok) setLanguage(previous);
  }

  const options = $derived<{ value: LanguagePreference; title: string; sub: string; code?: Language }[]>([
    {
      value: SYSTEM,
      title: t('settings.language.system'),
      sub: t('settings.language.systemHint', { name: nativeName(systemLanguage()) }),
    },
    ...LANGUAGES.map((l) => {
      const local = localName(l.code);
      return { value: l.code, title: l.name, sub: local && local !== l.name ? local : '', code: l.code };
    }),
  ]);
</script>

<section>
  <h2>{t('settings.language.title')}</h2>
  <p class="hint">{t('settings.language.hint')}</p>

  <div class="options" role="radiogroup" aria-label={t('settings.language.title')}>
    {#each options as o (o.value)}
      <label class="option" class:active={current === o.value} class:system={o.value === SYSTEM}>
        <input type="radio" name="language" value={o.value} checked={current === o.value} onchange={() => choose(o.value)} />
        <span class="icon" aria-hidden="true">
          {#if o.value === SYSTEM}
            <svg viewBox="0 0 24 24" width="18" height="18"
              ><path fill="currentColor" d={ICONS.monitor} /></svg
            >
          {:else}
            {o.code?.toUpperCase()}
          {/if}
        </span>
        <span class="text">
          <b>{o.title}</b>
          {#if o.sub}<small>{o.sub}</small>{/if}
        </span>
        <svg class="check" viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
          <path fill="currentColor" d={ICONS.done} />
        </svg>
      </label>
    {/each}
  </div>

  <p class="note">{t('settings.language.note')}</p>
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
  .options {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(230px, 1fr));
    gap: 10px;
  }
  /* "System language" spans the full width above the individual languages */
  .option.system {
    grid-column: 1 / -1;
  }
  .option {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    cursor: pointer;
    transition: border-color 0.12s, background 0.12s;
  }
  .option:hover {
    border-color: var(--muted);
  }
  .option.active {
    border-color: var(--amber);
    box-shadow: 0 0 0 1px var(--amber);
  }
  /* The radio stays reachable by keyboard but is visually replaced by the card */
  .option input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .option:has(input:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .icon {
    flex: none;
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: 50%;
    background: var(--surface-2);
    border: 1px solid var(--border-soft);
    color: var(--muted);
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.04em;
  }
  .active .icon {
    background: var(--amber);
    border-color: var(--amber);
    color: var(--on-amber);
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .text small {
    color: var(--muted);
    font-size: 12.5px;
  }
  .check {
    flex: none;
    color: var(--on-amber);
    background: var(--amber);
    border-radius: 50%;
    padding: 2px;
    opacity: 0;
    transform: scale(0.6);
    transition: opacity 0.12s, transform 0.12s;
  }
  .active .check {
    opacity: 1;
    transform: none;
  }
  .note {
    margin: 18px 0 0;
    font-size: 13px;
    color: var(--muted);
  }
</style>
