<script lang="ts">
  // Colour of a backup (the status lights of its Diskman rack): round swatches, the last one opens the
  // system colour dialog for any other colour.
  // value: the colour shown as chosen; onpreview(colour): while the colour dialog is open;
  // onchange(colour): chosen
  import { t } from '../../i18n/index.svelte.ts';
  import { VISORS } from '../../utils/visor.ts';

  interface Props {
    value: string;
    onpreview: (colour: string) => void;
    onchange: (colour: string) => void;
  }
  let { value, onpreview, onchange }: Props = $props();

  const custom = $derived(!VISORS.some(([c]) => c === value));
</script>

<div class="colors" role="radiogroup" aria-label={t('card.color')}>
  <span class="label">{t('card.color')}</span>
  {#each VISORS as [color, key] (color)}
    <button
      type="button"
      class="swatch"
      class:on={value === color}
      style:--c={color}
      role="radio"
      aria-checked={value === color}
      title={t(`card.colors.${key}`)}
      aria-label={t(`card.colors.${key}`)}
      onclick={() => onchange(color)}
    ></button>
  {/each}
  <label class="swatch custom" class:on={custom} style:--c={custom ? value : null} title={t('card.colors.custom')}>
    <input
      type="color"
      {value}
      aria-label={t('card.colors.custom')}
      oninput={(e) => onpreview(e.currentTarget.value)}
      onchange={(e) => onchange(e.currentTarget.value)}
    />
  </label>
</div>

<style>
  /* Right below the name of the backup */
  .colors {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: -12px;
  }
  .label {
    margin-right: 4px;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .swatch {
    position: relative;
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    min-height: 0;
    padding: 0;
    border: 2px solid var(--surface);
    border-radius: 50%;
    background: var(--c);
    box-shadow: 0 0 0 1px var(--border);
    cursor: pointer;
    transition:
      transform 0.15s ease,
      box-shadow 0.15s ease;
  }
  .swatch:hover:not(:disabled) {
    border-color: var(--surface);
    transform: scale(1.12);
  }
  /* Chosen: a ring in its own colour */
  .swatch.on {
    box-shadow: 0 0 0 2px var(--c);
  }
  /* Own colour: a colour wheel until one is chosen */
  .custom {
    background: var(--c, conic-gradient(#e5604d, #f2b84b, #5fbf6e, #3fb8a8, #4b9be6, #9b7be0, #e46aa6, #e5604d));
  }
  .custom input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    cursor: pointer;
  }
  .custom:focus-within {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  @media (prefers-reduced-motion: reduce) {
    .swatch {
      transition: none;
    }
  }
</style>
