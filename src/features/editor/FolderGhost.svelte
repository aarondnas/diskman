<script lang="ts">
  import { carry } from './folderCarry.svelte.ts';
  import { ICONS } from '../../utils/icons.ts';
  import { splitPath } from './folderPath.ts';
  import { t } from '../../i18n/index.svelte.ts';

  // The folder hanging on the mouse pointer; it takes the color of the slot below it
  const info = $derived(carry.path ? splitPath(carry.path) : null);
  const overSide = $derived(carry.over.split(':')[1] ?? '');
</script>

{#if info}
  <div class="ghost {overSide}" style:translate="{carry.x + 14}px {carry.y + 12}px" aria-hidden="true">
    <div class="tile">
      <svg viewBox="0 0 24 24"><path fill="currentColor" d={info.drive ? ICONS.drive : ICONS.folder} /></svg>
      <span>{info.name}</span>
      {#if carry.copy && carry.from}<b class="plus">+</b>{/if}
    </div>
    {#if !carry.drag}
      <div class="tip">{t('card.carryHint')}</div>
    {/if}
  </div>
{/if}

<style>
  .ghost {
    --tint: var(--accent);
    position: fixed;
    left: 0;
    top: 0;
    z-index: 1000;
    pointer-events: none;
  }
  .ghost.archive {
    --tint: var(--amber);
  }
  .tile {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: 240px;
    padding: 7px 12px 7px 9px;
    background: color-mix(in srgb, var(--tint) 18%, var(--surface));
    border: 1px solid var(--tint);
    border-radius: 3px 9px 9px 9px;
    box-shadow: 0 8px 22px rgb(0 0 0 / 0.22);
    transform: rotate(-3deg);
    font-weight: 600;
    font-size: 13.5px;
    transition: background 0.12s, border-color 0.12s;
  }
  .tile::before {
    content: '';
    position: absolute;
    left: -1px;
    top: -6px;
    width: 40%;
    height: 6px;
    background: inherit;
    border: inherit;
    border-bottom: none;
    border-radius: 5px 8px 0 0;
  }
  svg {
    flex: none;
    width: 22px;
    height: 22px;
    color: color-mix(in srgb, var(--tint) 80%, var(--text));
  }
  span {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .plus {
    position: absolute;
    right: -8px;
    top: -8px;
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--ok);
    color: white;
    font-size: 13px;
    line-height: 1;
  }
  .tip {
    margin: 8px 0 0 6px;
    width: max-content;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 11.5px;
    background: var(--text);
    color: var(--bg);
    opacity: 0.85;
  }
</style>
