<script lang="ts">
  // A setting as a card with a switch on the right, same card as a language choice.
  // onchange(event) receives the change event of the checkbox.
  interface Props {
    title: string;
    sub?: string;
    checked: boolean;
    disabled?: boolean;
    busy?: boolean;
    onchange: (e: Event & { currentTarget: HTMLInputElement }) => unknown;
  }
  let { title, sub = '', checked, disabled = false, busy = false, onchange }: Props = $props();
</script>

<label class="option" class:active={checked} class:busy>
  <span class="text">
    <b>{title}</b>
    {#if sub}<small>{sub}</small>{/if}
  </span>
  <input type="checkbox" role="switch" {checked} {disabled} {onchange} />
  <span class="switch" aria-hidden="true"></span>
</label>

<style>
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
    transition:
      border-color 0.12s,
      background 0.12s;
  }
  .option:hover {
    border-color: var(--muted);
  }
  .option.active {
    border-color: var(--amber);
    box-shadow: 0 0 0 1px var(--amber);
  }
  .option.busy {
    cursor: progress;
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
  /* The checkbox stays reachable by keyboard but is drawn as a switch */
  input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .option:has(input:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .switch {
    flex: none;
    position: relative;
    width: 38px;
    height: 22px;
    border-radius: 999px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    transition:
      background 0.15s,
      border-color 0.15s;
  }
  .switch::after {
    content: '';
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--muted);
    transition:
      transform 0.15s,
      background 0.15s;
  }
  input:checked + .switch {
    background: var(--amber);
    border-color: var(--amber);
  }
  input:checked + .switch::after {
    transform: translateX(16px);
    background: var(--on-amber);
  }
  input:disabled + .switch {
    opacity: 0.6;
  }
</style>
