<script lang="ts">
  import { backend } from '../../api/backend.ts';
  import { carry, clip, copyFolder, dropOn, pickUp, slotKey, type Side } from './folderCarry.svelte.ts';
  import { ICONS } from '../../utils/icons.ts';
  import { splitPath } from './folderPath.ts';
  import { t } from '../../i18n/index.svelte.ts';

  // A slot of an entry shown as a folder: the folder name large, the parent folders small.
  // entry: entry id or "new"; side: "source" | "archive"
  // onset(path): new folder chosen, typed or cleared (""); clearable: may be left empty
  // disabled: greyed out, takes neither clicks nor dropped folders (no data-slot)
  interface Props {
    entry: string;
    side: Side;
    path?: string;
    placeholder: string;
    clearable?: boolean;
    disabled?: boolean;
    onset: (path: string) => unknown;
  }
  let { entry, side, path = '', placeholder, clearable = false, disabled = false, onset }: Props = $props();

  const key = $derived(slotKey({ entry, side }));
  const info = $derived(path ? splitPath(path) : null);
  const icon = $derived(info?.drive ? ICONS.drive : side === 'archive' ? ICONS.archive : ICONS.folder);
  const label = $derived(t(side === 'source' ? 'card.source' : 'card.archive'));
  const over = $derived(!!carry.path && carry.over === key);
  // The tile being dragged away (not copied) is shown faded at its place
  const leaving = $derived(!!carry.path && !carry.copy && slotKey(carry.from) === key);

  let editing = $state(false);
  let picking = $state(false);
  let el = $state<HTMLElement>();

  async function browse() {
    picking = true;
    try {
      const picked = await backend.pickFolder();
      if (picked) onset(picked);
    } catch {
      // Dialog unavailable → the path can still be typed in (F2 or the pencil)
    } finally {
      picking = false;
    }
  }

  function center() {
    const r = el!.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  }

  function copy(e: MouseEvent) {
    copyFolder({ entry, side, path });
    // From the button the copy hangs on the pointer right away, ready to be placed
    pickUp(path, { copy: true, at: e.detail === 0 ? center() : { x: e.clientX, y: e.clientY } });
  }

  function onpointerdown(e: PointerEvent) {
    if (disabled || !path || editing || carry.path || e.button !== 0 || (e.target as Element).closest('button, input')) return;
    const start = { x: e.clientX, y: e.clientY };
    const move = (ev: PointerEvent) => {
      if (Math.hypot(ev.clientX - start.x, ev.clientY - start.y) < 5) return;
      stop();
      pickUp(path, { from: { entry, side, path }, drag: true, copy: ev.ctrlKey, at: { x: ev.clientX, y: ev.clientY } });
    };
    const stop = () => {
      removeEventListener('pointermove', move);
      removeEventListener('pointerup', stop);
    };
    addEventListener('pointermove', move);
    addEventListener('pointerup', stop);
  }

  function onclick(e: MouseEvent) {
    // Clicks on the tile's own buttons are not meant for the tile: after ✕ the slot is already
    // empty when the click arrives here and would open the folder dialog
    if ((e.target as Element).closest('button, input')) return;
    // Clicking the tile only selects it (paste target); icon and label open the folder dialog.
    // After dragging a tile away no click arrives here (swallowClick in folderCarry)
    if (!disabled && !editing && !picking && (e.target as Element).closest('.pick')) browse();
  }

  function onkeydown(e: KeyboardEvent) {
    if (disabled || editing || e.target !== el) return;
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      if (carry.path) dropOn({ entry, side, path });
      else if (path) pickUp(path, { from: { entry, side, path }, at: center() });
      else browse();
    } else if (e.key === 'F2') {
      e.preventDefault();
      editing = true;
    } else if ((e.key === 'Delete' || e.key === 'Backspace') && clearable && path) {
      e.preventDefault();
      onset('');
    }
  }

  function commit(value: string) {
    editing = false;
    const v = value.trim();
    if (v === path || (!v && !clearable)) return;
    onset(v);
    el?.focus();
  }

  function autofocus(input: HTMLInputElement) {
    input.focus();
    input.select();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  bind:this={el}
  class="slot {side}"
  class:filled={!!path}
  class:over
  class:target={!!carry.path && !leaving && !disabled}
  class:disabled
  class:leaving
  class:copied={clip.flashed === key}
  data-slot={disabled ? undefined : ''}
  data-entry={entry}
  data-side={side}
  data-path={path}
  tabindex={disabled ? -1 : 0}
  aria-disabled={disabled || undefined}
  role="group"
  aria-label={path ? `${label}: ${path}` : `${label}: ${placeholder}`}
  title={path || undefined}
  {onpointerdown}
  {onclick}
  {onkeydown}
  ondblclick={() => !disabled && !editing && (editing = true)}
>
  {#if editing}
    <input
      type="text"
      class="mono"
      value={path}
      {placeholder}
      spellcheck="false"
      aria-label={label}
      use:autofocus
      onkeydown={(e) => {
        if (e.key === 'Enter') commit(e.currentTarget.value);
        else if (e.key === 'Escape') (e.stopPropagation(), (editing = false), el?.focus());
      }}
      onblur={(e) => editing && commit(e.currentTarget.value)}
    />
  {:else if info}
    <span class="pick">
      <svg class="icon" viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d={icon} /></svg>
      <span class="text">
        <span class="name">{info.name}</span>
        <span class="parent">{info.drive ? t('card.drive') : info.parent}</span>
      </span>
    </span>
    {#if clip.flashed === key}
      <span class="flash" role="status">{t('card.copied')}</span>
    {/if}
    <span class="tools">
      <button type="button" class="ghost tool" title={t('card.copyFolder')} aria-label={t('card.copyFolder')} onclick={copy}>
        <svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d={ICONS.copy} /></svg>
      </button>
      <button
        type="button"
        class="ghost tool"
        title={t('folder.choose')}
        aria-label={t('folder.choose')}
        disabled={picking}
        onclick={browse}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d={ICONS.browse} /></svg>
      </button>
      <button
        type="button"
        class="ghost tool"
        title={t('card.editPath')}
        aria-label={t('card.editPath')}
        onclick={() => (editing = true)}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d={ICONS.edit} /></svg>
      </button>
      {#if clearable}
        <button
          type="button"
          class="ghost tool danger"
          title={t('card.clearFolder')}
          aria-label={t('card.clearFolder')}
          onclick={() => onset('')}>✕</button
        >
      {/if}
    </span>
  {:else}
    <!-- Icon and label together open the folder dialog; the rest of the tile only selects it -->
    <span class="pick">
      <svg class="icon" viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d={ICONS.newFolder} /></svg>
      <span class="text">
        <span class="name">{picking ? '…' : placeholder}</span>
        <span class="parent">{t(carry.path ? 'card.dropHere' : 'card.slotHint')}</span>
      </span>
    </span>
    <span class="tools">
      <button
        type="button"
        class="ghost tool"
        title={t('folder.choose')}
        aria-label={t('folder.choose')}
        disabled={picking}
        onclick={browse}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d={ICONS.browse} /></svg>
      </button>
      <button
        type="button"
        class="ghost tool"
        title={t('card.editPath')}
        aria-label={t('card.editPath')}
        onclick={(e) => (e.stopPropagation(), (editing = true))}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d={ICONS.edit} /></svg>
      </button>
    </span>
  {/if}
</div>

<style>
  /* Folder shape: a body with a tab on the top left */
  .slot {
    --tint: var(--accent);
    --fill: color-mix(in srgb, var(--tint) 9%, var(--surface));
    --line: color-mix(in srgb, var(--tint) 38%, var(--border));
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    min-height: 56px;
    margin-top: 7px;
    padding: 8px 8px 8px 12px;
    background: var(--fill);
    border: 1px solid var(--line);
    border-radius: 3px 10px 10px 10px;
    user-select: none;
    cursor: grab;
    /* Border and shadow change instantly: faded, the selection frame lagged behind the tab */
    transition:
      background 0.12s,
      opacity 0.12s,
      transform 0.12s;
  }
  .slot.archive {
    --tint: var(--amber);
    --line: color-mix(in srgb, var(--tint) 60%, var(--border));
    --fill: color-mix(in srgb, var(--tint) 11%, var(--surface));
  }
  .slot::before {
    content: '';
    position: absolute;
    left: -1px;
    top: -7px;
    width: min(42%, 96px);
    height: 7px;
    background: inherit;
    border: inherit;
    border-bottom: none;
    border-radius: 6px 9px 0 0;
  }
  .slot:hover {
    box-shadow: 0 2px 8px rgb(0 0 0 / 0.08);
  }
  .slot:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  /* Selected slot, filled or empty (clicked or focused, also while its tools or the path field are
     in use): yellow frame, here Ctrl+V pastes. Not while a folder is carried, so the drop targets
     stay unambiguous */
  :global(html:not([data-carrying])) .slot:not(.disabled):focus-within {
    border-color: var(--amber);
    box-shadow: 0 0 0 2px var(--amber);
    outline: none;
  }
  /* The tab gets the frame on its left, top and right only; a ring would cut into the body */
  :global(html:not([data-carrying])) .slot:not(.disabled):focus-within::before {
    box-shadow:
      -2px -2px 0 var(--amber),
      2px -2px 0 var(--amber);
  }
  /* Empty slot: dashed outline, click to select, click the label to choose. Opaque fill so the tab
     covers the top edge below it */
  .slot:not(.filled) {
    --fill: color-mix(in srgb, var(--tint) 3%, var(--surface));
    border-style: dashed;
    cursor: default;
    color: var(--muted);
  }
  /* Click area (opens the folder dialog): icon and text side by side. The negative margin cancels
     the padding, so tile height and text position stay the same */
  .pick {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    margin: -4px -8px;
    padding: 3px 7px;
    border: 1px solid transparent;
    border-radius: 7px;
  }
  .slot:not(.disabled):not(.target):not(.leaving) .pick {
    cursor: pointer;
  }
  .slot:not(.disabled):not(.target):not(.leaving) .pick:hover {
    background: color-mix(in srgb, var(--tint) 10%, var(--surface));
    border-color: color-mix(in srgb, var(--tint) 35%, var(--border));
    color: var(--text);
  }
  /* The tab has no bottom edge; plain "dashed" would bring it back 3 px thick */
  .slot:not(.filled)::before {
    border-style: dashed dashed none;
  }
  .slot:not(.filled):hover {
    --fill: color-mix(in srgb, var(--tint) 6%, var(--surface));
    color: var(--text);
  }
  /* While a folder is carried: every slot is a target, the one below the pointer lights up */
  .slot.target {
    border-style: dashed;
    cursor: copy;
  }
  .slot.over {
    --fill: color-mix(in srgb, var(--tint) 24%, var(--surface));
    border-style: solid;
    border-color: var(--tint);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--tint) 30%, transparent);
    transform: scale(1.015);
  }
  .slot.disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .slot.disabled .tools {
    display: none;
  }
  .slot.leaving {
    opacity: 0.4;
  }
  .slot.copied {
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--ok) 35%, transparent);
  }
  .icon {
    flex: none;
    width: 30px;
    height: 30px;
    color: color-mix(in srgb, var(--tint) 80%, var(--text));
  }
  .slot:not(.filled) .icon {
    width: 24px;
    height: 24px;
    margin: 0 3px;
    color: inherit;
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
    line-height: 1.3;
  }
  .name,
  .parent {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .name {
    font-weight: 600;
    font-size: 14.5px;
  }
  .slot:not(.filled) .name {
    font-weight: 500;
    font-size: 13px;
  }
  .parent {
    font-family: var(--mono);
    font-size: 11.5px;
    color: var(--muted);
  }
  .slot:not(.filled) .parent {
    font-family: inherit;
  }
  /* Tools appear on hover and focus */
  .tools {
    display: flex;
    margin-left: auto;
    gap: 2px;
    opacity: 0;
    transition: opacity 0.12s;
  }
  .slot:hover .tools,
  .slot:focus-within .tools {
    opacity: 1;
  }
  .slot.target .tools {
    visibility: hidden;
  }
  .tool {
    display: grid;
    place-items: center;
    width: 28px;
    min-height: 28px;
    height: 28px;
    padding: 0;
    border-color: transparent;
    color: var(--muted);
    font-size: 12px;
  }
  .tool:hover:not(:disabled) {
    color: var(--text);
    background: var(--surface);
  }
  .tool svg {
    width: 16px;
    height: 16px;
  }
  .flash {
    position: absolute;
    right: 8px;
    top: -10px;
    padding: 0 8px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 600;
    background: var(--ok-soft);
    color: var(--ok);
    border: 1px solid color-mix(in srgb, var(--ok) 40%, transparent);
  }
  input {
    flex: 1;
    min-width: 0;
  }
  @media (prefers-reduced-motion: reduce) {
    .slot {
      transition: none;
    }
    .slot.over {
      transform: none;
    }
  }
</style>
