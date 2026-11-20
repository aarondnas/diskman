<script lang="ts">
  import type { Mode } from '../../api/types.ts';
  import { t, type TextKey } from '../../i18n/index.svelte.ts';
  import { ICONS } from '../../utils/icons.ts';

  // Connection from the source tile to the archive tile. The two modes are stacked as stations;
  // the animated line branches off the source, runs through the chosen one (which lights up) and
  // joins again towards the archive. on: entry active, otherwise the line stands still.
  // disabled: greyed out until the entry can be used (new entry without a source folder)
  interface Props {
    mode: Mode;
    on?: boolean;
    disabled?: boolean;
    onselect: (mode: Mode) => void;
  }
  let { mode, on = true, disabled = false, onselect }: Props = $props();

  const MODES: { id: Mode; label: TextKey; hint: TextKey; icon: string }[] = [
    { id: 'mirror', label: 'common.mirror', hint: 'card.mirrorHint', icon: ICONS.mirror },
    { id: 'additive', label: 'common.additive', hint: 'card.additiveHint', icon: ICONS.plus },
  ];

  // Geometry of the connectors in px: two stations of 30 px with 6 px between them
  const H = 66;
  const MID = H / 2;
  const Y = [15, 51];
  const inPath = (y: number) => `M0 ${MID} C12 ${MID} 10 ${y} 22 ${y}`;
  const outPath = (y: number) => `M0 ${y} C12 ${y} 10 ${MID} 22 ${MID}`;

  let buttons = $state<HTMLButtonElement[]>([]);

  function onkeydown(e: KeyboardEvent) {
    if (disabled) return;
    const i = MODES.findIndex((m) => m.id === mode);
    const steps: Record<string, number> = { ArrowUp: i - 1, ArrowLeft: i - 1, ArrowDown: i + 1, ArrowRight: i + 1 };
    const next = steps[e.key];
    if (next === undefined) return;
    e.preventDefault();
    const m = MODES[(next + MODES.length) % MODES.length];
    onselect(m.id);
    buttons[MODES.indexOf(m)]?.focus();
  }
</script>

<div class="flow" class:on class:disabled role="radiogroup" aria-label={t('card.mode')} tabindex="-1" {onkeydown}>
  <svg class="wire in" width="22" height={H} viewBox="0 0 22 {H}" aria-hidden="true">
    {#each MODES as m, i (m.id)}
      <path d={inPath(Y[i])} class:lit={m.id === mode} />
    {/each}
  </svg>

  <div class="stations">
    {#each MODES as m, i (m.id)}
      <button
        bind:this={buttons[i]}
        type="button"
        class="station"
        class:lit={m.id === mode}
        role="radio"
        aria-checked={m.id === mode}
        tabindex={m.id === mode ? 0 : -1}
        title={t(m.hint)}
        {disabled}
        onclick={() => onselect(m.id)}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d={m.icon} /></svg>
        <span>{t(m.label)}</span>
      </button>
    {/each}
  </div>

  <svg class="wire out" width="30" height={H} viewBox="0 0 30 {H}" aria-hidden="true">
    {#each MODES as m, i (m.id)}
      <path d={outPath(Y[i])} class:lit={m.id === mode} />
    {/each}
    <path class="head" d="M21 {MID - 5} L29 {MID} L21 {MID + 5} Z" />
  </svg>
</div>

<style>
  .flow {
    --from: var(--accent-text);
    --to: var(--amber);
    --glow: color-mix(in srgb, var(--amber) 55%, transparent);
    display: flex;
    align-items: center;
    justify-content: center;
    margin-top: 7px;
    outline: none;
  }
  .wire {
    flex: none;
    overflow: visible;
  }
  .wire path {
    fill: none;
    stroke: var(--border);
    stroke-width: 1.5;
    stroke-dasharray: 4 4;
    transition:
      stroke 0.25s,
      stroke-width 0.25s;
  }
  .wire.in path.lit {
    stroke: var(--from);
  }
  .wire.out path.lit {
    stroke: var(--to);
  }
  .wire path.lit {
    stroke-width: 2.5;
  }
  .head {
    fill: var(--border);
    stroke: none !important;
    transition: fill 0.25s;
  }
  .flow.on .head {
    fill: var(--to);
  }
  /* Dashes run in direction of the backup while the entry is active */
  .flow.on .wire path.lit {
    animation: run 0.7s linear infinite;
  }
  .flow:not(.on) .wire path.lit {
    stroke: var(--muted);
    stroke-dasharray: none;
  }
  @keyframes run {
    to {
      stroke-dashoffset: -8;
    }
  }

  .stations {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .station {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 30px;
    height: 30px;
    padding: 0 12px 0 9px;
    border-radius: 999px;
    border: 1px dashed var(--border);
    background: var(--surface);
    color: var(--muted);
    font-size: 12.5px;
    white-space: nowrap;
    overflow: hidden;
    transition:
      color 0.25s,
      border-color 0.25s,
      box-shadow 0.25s,
      opacity 0.25s;
  }
  .flow.disabled {
    opacity: 0.4;
  }
  .station:not(.lit) {
    opacity: 0.7;
  }
  .station:not(.lit):hover {
    opacity: 1;
    color: var(--text);
  }
  .station svg {
    flex: none;
    width: 15px;
    height: 15px;
  }
  /* Chosen station: solid frame from source to archive color and a glow */
  .station.lit {
    border: 1px solid transparent;
    background:
      linear-gradient(var(--surface), var(--surface)) padding-box,
      linear-gradient(90deg, var(--from), var(--to)) border-box;
    color: var(--text);
    font-weight: 600;
  }
  .station.lit svg {
    color: var(--to);
  }
  .flow.on .station.lit {
    box-shadow: 0 0 10px var(--glow);
  }
  @media (prefers-reduced-motion: reduce) {
    .flow.on .wire path.lit {
      animation: none;
    }
  }
</style>
