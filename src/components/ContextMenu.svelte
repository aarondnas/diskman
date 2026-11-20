<script lang="ts" module>
  /** One entry of the menu. */
  export interface MenuItem {
    label: string;
    /** Path of a 24×24 icon (utils/icons.ts). */
    icon?: string;
    onselect: () => void;
  }
</script>

<script lang="ts">
  // A menu at the pointer (right click), or at an element (menu key): its entries one below the
  // other. It stays inside the window, takes the focus (↑ / ↓ move, Enter chooses) and closes with
  // a choice, Escape, a click elsewhere, scrolling or when the window loses the focus. Escape only
  // closes the menu, not a dialog below it.
  import { onMount } from 'svelte';

  interface Props {
    /** Where it opens (window coordinates); it moves left or up if it would not fit. */
    x: number;
    y: number;
    items: MenuItem[];
    onclose: () => void;
  }
  let { x, y, items, onclose }: Props = $props();

  let menu = $state<HTMLElement>();
  let left = $state(0);
  let top = $state(0);
  $effect.pre(() => {
    left = x;
    top = y;
  });

  const buttons = () => [...(menu?.querySelectorAll<HTMLButtonElement>('button') ?? [])];

  onMount(() => {
    if (!menu) return;
    const { width, height } = menu.getBoundingClientRect();
    left = Math.max(4, Math.min(x, innerWidth - width - 4));
    top = Math.max(4, Math.min(y, innerHeight - height - 4));
    buttons()[0]?.focus();

    // Captured on the window: before the keys reach a dialog below
    const onkey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' || e.key === 'Tab') {
        e.preventDefault();
        e.stopPropagation();
        onclose();
      } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        e.stopPropagation();
        const all = buttons();
        const i = all.indexOf(document.activeElement as HTMLButtonElement);
        all[(i + (e.key === 'ArrowDown' ? 1 : -1) + all.length) % all.length]?.focus();
      }
    };
    const outside = (e: Event) => {
      if (!menu?.contains(e.target as Node)) onclose();
    };
    const close = () => onclose();
    addEventListener('keydown', onkey, true);
    addEventListener('pointerdown', outside, true);
    addEventListener('scroll', close, true);
    addEventListener('resize', close);
    addEventListener('blur', close);
    return () => {
      removeEventListener('keydown', onkey, true);
      removeEventListener('pointerdown', outside, true);
      removeEventListener('scroll', close, true);
      removeEventListener('resize', close);
      removeEventListener('blur', close);
    };
  });

  function choose(item: MenuItem) {
    onclose();
    item.onselect();
  }
</script>

<div class="menu" role="menu" bind:this={menu} style:left="{left}px" style:top="{top}px">
  {#each items as item (item.label)}
    <button role="menuitem" onclick={() => choose(item)}>
      {#if item.icon}
        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="currentColor" d={item.icon} /></svg>
      {/if}
      {item.label}
    </button>
  {/each}
</div>

<style>
  /* Above the dialogs (22–25) and the question dialog (30) */
  .menu {
    position: fixed;
    z-index: 40;
    min-width: 200px;
    padding: 4px;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 10px 30px rgb(0 0 0 / 0.25);
  }
  button {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 32px;
    padding: 0 12px 0 10px;
    border: none;
    border-radius: 6px;
    background: none;
    color: var(--text);
    font-size: 13px;
    text-align: left;
    white-space: nowrap;
  }
  button svg {
    flex: none;
    color: var(--muted);
  }
  button:hover:not(:disabled),
  button:focus-visible {
    background: var(--accent-soft);
    border: none;
    outline: none;
  }
  button:hover svg,
  button:focus-visible svg {
    color: var(--accent-text);
  }
</style>
