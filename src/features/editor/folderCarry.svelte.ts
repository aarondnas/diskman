// A folder tile "carried" on the mouse pointer until it is dropped onto a slot, plus the folder
// clipboard. Slots are elements with data-slot, data-entry (entry id or "new") and data-side
// ("source" | "archive"). The state lives in this module, so a copied folder can also be placed
// in another backup.
//
// Two ways of carrying:
// - click: the tile hangs on the pointer (after choosing a folder, copying, pasting); the next
//   click drops it on the slot below, anywhere else it is discarded.
// - drag: a placed tile is dragged with the mouse button held and dropped on release; with Ctrl it
//   is copied instead of moved.

export type Side = 'source' | 'archive';

/** A place a folder can be dropped on: an entry's source or archive (entry "new" for a new one). */
export interface Slot {
  entry: string;
  side: Side;
  /** The folder placed there, "" if empty. */
  path: string;
}

/** What is dropped: the folder, where it was taken from (null if not from a slot), copy or move. */
export interface Dropped {
  path: string;
  from: Slot | null;
  copy: boolean;
}

export const carry = $state({
  path: '',
  from: null as Slot | null,
  copy: false,
  drag: false,
  x: 0,
  y: 0,
  over: '',
});
/** path: copied folder; flashed: slot key that shows "Copied" for a moment. */
export const clip = $state({ path: '', flashed: '' });

const pointer = { x: 0, y: 0 };
addEventListener(
  'pointermove',
  (e) => {
    pointer.x = e.clientX;
    pointer.y = e.clientY;
  },
  { capture: true, passive: true },
);

type DropHandler = (slot: Slot, dropped: Dropped) => void;
let onDrop: DropHandler | null = null;

/** The open backup editor receives drops. Returns the cleanup. */
export function setDropHandler(fn: DropHandler) {
  onDrop = fn;
  return () => {
    if (onDrop === fn) onDrop = null;
    cancel();
  };
}

export const slotKey = (slot: Pick<Slot, 'entry' | 'side'> | null | undefined) => (slot ? `${slot.entry}:${slot.side}` : '');

export function slotOf(el: EventTarget | Element | null | undefined): Slot | null {
  const s = (el as Element | null)?.closest?.<HTMLElement>('[data-slot]');
  return s ? { entry: s.dataset.entry ?? '', side: s.dataset.side as Side, path: s.dataset.path ?? '' } : null;
}

export const slotAt = (x: number, y: number) => slotOf(document.elementFromPoint(x, y));

/** The selected (focused) slot, otherwise the one under the mouse (for Ctrl+C / Ctrl+V). */
export const currentSlot = () => slotOf(document.activeElement) ?? slotAt(pointer.x, pointer.y);

interface PickUpOptions {
  from?: Slot | null;
  copy?: boolean;
  drag?: boolean;
  at?: { x: number; y: number };
}

export function pickUp(path: string, { from = null, copy = false, drag = false, at = pointer }: PickUpOptions = {}) {
  if (!path) return;
  cancel();
  Object.assign(carry, { path, from, copy, drag, x: at.x, y: at.y, over: slotKey(slotAt(at.x, at.y)) });
  document.documentElement.dataset.carrying = '';
  for (const [type, fn] of listeners) addEventListener(type, fn as EventListener, true);
}

export function cancel() {
  if (!carry.path) return;
  Object.assign(carry, { path: '', from: null, over: '' });
  delete document.documentElement.dataset.carrying;
  for (const [type, fn] of listeners) removeEventListener(type, fn as EventListener, true);
}

/** Drops the carried folder onto the slot; without a slot it is discarded. */
export function dropOn(slot: Slot | null) {
  const c: Dropped = { path: carry.path, from: carry.from, copy: carry.copy };
  cancel();
  if (slot && c.path) onDrop?.(slot, c);
}

/** Copies a placed folder; also into the system clipboard so it can be pasted elsewhere. */
export function copyFolder(slot: Slot | null) {
  if (!slot?.path) return;
  clip.path = slot.path;
  const key = slotKey(slot);
  clip.flashed = key;
  setTimeout(() => clip.flashed === key && (clip.flashed = ''), 1200);
  navigator.clipboard?.writeText(slot.path).catch(() => {
    // Without access only the internal clipboard holds it
  });
}

/** After a drag ends the browser still sends a click; it must not reach the element below. */
function swallowClick() {
  const stop = (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
  };
  addEventListener('click', stop, true);
  setTimeout(() => removeEventListener('click', stop, true));
}

type Listener = [type: keyof WindowEventMap, fn: (e: never) => unknown];

/** A typed entry of `listeners`: the handler gets the event of its type. */
const on = <K extends keyof WindowEventMap>(type: K, fn: (e: WindowEventMap[K]) => unknown): Listener => [type, fn];

const listeners: Listener[] = [
  on(
    'pointermove',
    (e) => {
      // Button released outside the window: the drop is lost
      if (carry.drag && !e.buttons) return cancel();
      carry.x = e.clientX;
      carry.y = e.clientY;
      carry.over = slotKey(slotAt(e.clientX, e.clientY));
      if (carry.drag && carry.from) carry.copy = e.ctrlKey;
    },
  ),
  on('pointerdown', (e) => !carry.drag && e.preventDefault()),
  on(
    'click',
    (e) => {
      if (carry.drag) return;
      e.preventDefault();
      e.stopPropagation();
      // Clicks from the keyboard (Enter on a button) have no position
      dropOn(e.detail === 0 ? slotOf(e.target) : slotAt(e.clientX, e.clientY));
    },
  ),
  on(
    'pointerup',
    (e) => {
      if (!carry.drag) return;
      swallowClick();
      dropOn(slotAt(e.clientX, e.clientY));
    },
  ),
  on(
    'keydown',
    (e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        cancel();
      } else if (e.key === 'Control' && carry.drag && carry.from) carry.copy = true;
    },
  ),
  on('keyup', (e) => e.key === 'Control' && carry.drag && carry.from && (carry.copy = false)),
  on(
    'contextmenu',
    (e) => {
      e.preventDefault();
      cancel();
    },
  ),
  on('blur', () => cancel()),
];
