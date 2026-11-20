<script lang="ts">
  // Editor of one backup: its name and colour, and its entries. Each entry pairs a source folder
  // (left) with its archive (right). Both are shown as folder tiles that can be dragged, copied
  // and dropped onto other slots, also in other backups (the clipboard is kept, see
  // folderCarry.svelte.ts). Everything is saved right away through onupdate, there is no save button.
  import { onMount, untrack } from 'svelte';
  import type { Archive, Backup, Id, Mode } from '../../api/types.ts';
  import DiskmanHead from '../../components/DiskmanHead.svelte';
  import { t } from '../../i18n/index.svelte.ts';
  import { confirm } from '../../stores/confirm.svelte.ts';
  import { clearError, notice } from '../../stores/notice.svelte.ts';
  import { jobs } from '../../stores/jobs.svelte.ts';
  import { relativeTime } from '../../utils/format.ts';
  import { CROSS, ICONS } from '../../utils/icons.ts';
  import { DEFAULT_VISOR, visorOf } from '../../utils/visor.ts';
  import EntrySwitch from './EntrySwitch.svelte';
  import {
    carry,
    clip,
    copyFolder,
    currentSlot,
    pickUp,
    setDropHandler,
    type Dropped,
    type Side,
    type Slot,
  } from './folderCarry.svelte.ts';
  import FolderGhost from './FolderGhost.svelte';
  import { lastSegment, parsePath, withFolder } from './folderPath.ts';
  import FolderTile from './FolderTile.svelte';
  import ModeFlow from './ModeFlow.svelte';
  import VisorPicker from './VisorPicker.svelte';

  // isDraft: not saved yet, so no status and nothing to remove
  // namePlaceholder: shown in the empty name field (a draft gets the name it would be created with)
  // onupdate(mutate): saves a change of the backup, returns true on success; onremove: deletes it
  interface Props {
    backup: Backup;
    isDraft?: boolean;
    namePlaceholder?: string;
    onupdate: (mutate: (backup: Backup) => unknown) => Promise<boolean>;
    onremove: () => void;
  }
  let { backup, isDraft = false, namePlaceholder = t('card.namePlaceholder'), onupdate, onremove }: Props = $props();

  // Slot entry id of the lane for a new entry
  const NEW = 'new';

  /** A slot to fill: an entry (or the new lane) and its side. */
  type SlotRef = Pick<Slot, 'entry' | 'side'>;
  type SlotChange = { slot: SlotRef; path: string };

  /** Saves a change of one saved entry. */
  const updateEntry = (id: Id, mutate: (archive: Archive) => unknown) =>
    onupdate((b) => {
      const a = b.archives.find((x) => x.id === id);
      if (a) mutate(a);
    });

  // The lane for a new entry only shows after "Add entry" was clicked. It becomes an entry as soon
  // as both folders are chosen; with only one it is dropped when the page is left (or with its ✕).
  let adding = $state(false);
  let newSource = $state('');
  let newPath = $state('');
  let newMode = $state<Mode>('mirror');
  // Create a folder named after the source folder inside the archive
  let createFolder = $state(true);

  const entry = (b: Backup, id: Id) => b.archives.find((a) => a.id === id);

  // Visor colour: while the colour dialog is open, the one being chosen (liveVisor)
  let liveVisor = $state<string | null>(null);
  const visor = $derived((liveVisor ?? visorOf(backup)).toLowerCase());

  /** Saves the colour; the default is not stored, so the backup keeps following it. */
  function setColor(color: string) {
    liveVisor = null;
    const c = color.toLowerCase();
    onupdate((b) => {
      if (c === DEFAULT_VISOR) delete b.color;
      else b.color = c;
    });
  }

  const folderName = $derived(lastSegment(newSource));
  const useFolder = $derived(createFolder && !!folderName);
  const finalPath = $derived(useFolder ? withFolder(newPath, folderName) : newPath.trim());
  const canAdd = $derived(!!newSource.trim() && !!newPath.trim());

  // Both folders chosen: add the entry at once. If saving fails (e.g. the archive lies inside the
  // folder), the lane stays and is tried again when one of its choices changes.
  let committing = false;
  $effect(() => {
    // Only these choices trigger it, not what saving reads (the config)
    const ready = adding && canAdd && [finalPath, newMode];
    if (ready && !committing) untrack(addEntry);
  });

  // The error with which the new entry was rejected (e.g. archive inside the folder). It goes away
  // again once the entry is no longer complete (a folder removed) or the lane is closed, unless
  // another error has replaced it meanwhile. A changed but still complete entry is simply retried.
  let addError = '';
  function dropAddError() {
    if (addError && notice.error === addError) clearError();
    addError = '';
  }
  $effect(() => {
    if (!(adding && canAdd)) untrack(dropAddError);
  });

  const field = (side: Side) => (side === 'source' ? 'source' : 'path');

  function valueOf(slot: SlotRef) {
    if (slot.entry === NEW) return slot.side === 'source' ? newSource : newPath;
    return entry(backup, slot.entry)?.[field(slot.side)] ?? '';
  }

  /** Sets several slots at once; saved entries in one update, so a swap is saved as a whole. */
  async function setSlots(changes: SlotChange[]) {
    const saved = changes.filter((c) => c.slot.entry !== NEW);
    if (saved.length) {
      const ok = await onupdate((b) => {
        for (const { slot, path } of saved) {
          const a = entry(b, slot.entry);
          if (a) a[field(slot.side)] = path;
        }
      });
      if (!ok) return false;
    }
    for (const { slot, path } of changes) {
      if (slot.entry !== NEW) continue;
      if (slot.side === 'source') newSource = path;
      else newPath = path;
    }
    return true;
  }

  const setSlot = (entryId: string, side: Side) => (path: string) => setSlots([{ slot: { entry: entryId, side }, path }]);

  /**
   * A folder was dropped onto a slot. Copies just fill the slot. Moves swap with the folder that was
   * there; a saved entry cannot be left without a folder, so moving from it onto an empty slot copies.
   */
  function drop(target: SlotRef, { path, from, copy }: Dropped) {
    if (from && from.entry === target.entry && from.side === target.side) return;
    const changes: SlotChange[] = [{ slot: target, path }];
    if (from && !copy) {
      const old = valueOf(target);
      if (old) changes.push({ slot: from, path: old });
      else if (from.entry === NEW) changes.push({ slot: from, path: '' });
    }
    setSlots(changes);
  }

  onMount(() => {
    const stopDrops = setDropHandler(drop);
    // Leaving the page drops the unfinished entry, and with it its error
    return () => (stopDrops(), dropAddError());
  });

  function closeNew() {
    adding = false;
    newSource = '';
    newPath = '';
    newMode = 'mirror';
    createFolder = true;
  }

  async function addEntry() {
    const source = newSource.trim();
    const path = finalPath;
    const mode = newMode;
    committing = true;
    const before = notice.error;
    try {
      const ok = await onupdate((b) => b.archives.push({ id: crypto.randomUUID(), source, path, mode, enabled: true }));
      if (ok) closeNew();
      else if (notice.error !== before) {
        // Failed while still complete: its error stays until the entry changes. Already incomplete
        // again (a folder was removed while saving): the error is out of date right away
        addError = notice.error;
        if (!(adding && canAdd)) dropAddError();
      }
    } finally {
      committing = false;
    }
  }

  async function removeEntry(a: Archive) {
    if (await confirm(t('card.removeEntryConfirm', { path: `${a.source} → ${a.path}` }), t('common.remove'))) {
      onupdate((b) => (b.archives = b.archives.filter((x) => x.id !== a.id)));
    }
  }

  // Ctrl+C / Ctrl+V on folder tiles (the selected one, otherwise the one below the pointer).
  // Text fields keep their normal copy and paste.
  const typing = () => !!document.activeElement?.matches?.('input[type="text"], input:not([type]), textarea');
  let pendingPaste: ReturnType<typeof setTimeout> | undefined;

  function place(path: string) {
    const slot = currentSlot();
    if (slot) drop(slot, { path, from: null, copy: true });
    else pickUp(path, { copy: true });
  }

  function onkeydown(e: KeyboardEvent) {
    if (!(e.ctrlKey || e.metaKey) || e.altKey || typing() || carry.path) return;
    const k = e.key.toLowerCase();
    if (k === 'c') {
      const slot = currentSlot();
      if (slot?.path && !getSelection()?.toString()) copyFolder(slot);
    } else if (k === 'v') {
      // The paste event carries the system clipboard (e.g. "Copy as path" from Explorer); if it
      // does not come, the internal clipboard is used
      clearTimeout(pendingPaste);
      pendingPaste = setTimeout(() => clip.path && place(clip.path), 60);
    }
  }

  function onpaste(e: ClipboardEvent) {
    if (typing() || carry.path) return;
    clearTimeout(pendingPaste);
    const path = parsePath(e.clipboardData?.getData('text/plain')) || clip.path;
    if (!path) return;
    e.preventDefault();
    place(path);
  }
</script>

<svelte:window {onkeydown} {onpaste} />

<div class="editor">
  <!-- The name is the title of the page and can be edited in place -->
  <header>
    <DiskmanHead visor={visor} size={48} />
    <input
      class="name"
      value={backup.name}
      placeholder={namePlaceholder}
      oninput={(e) => isDraft && onupdate((b) => (b.name = e.currentTarget.value))}
      onchange={(e) => onupdate((b) => (b.name = e.currentTarget.value.trim()))}
      aria-label={t('card.nameLabel')}
    />
    {#if !isDraft}
      <div class="actions">
        <button class="danger" onclick={onremove}>{t('card.removeBackup')}</button>
      </div>
    {/if}
  </header>

  <!-- Colour of the status lights of the rack (on the card in the list and above). Amber unless another one
       is chosen; it is preselected like the name and not stored. -->
  <VisorPicker value={visor} onpreview={(c) => (liveVisor = c)} onchange={setColor} />

  <section>
    <h2>{t('card.entries')}</h2>
    <!-- Hint between the title and the list, not as a row inside it -->
    {#if !backup.archives.length}
      <p class="empty">{t('card.noEntry')}</p>
    {/if}

    <div class="wrap">
      <div class="card entries">
        {#each backup.archives as a (a.id)}
          {@const st = jobs.status[a.id]}
          <div class="lane" class:off={!a.enabled}>
            <EntrySwitch on={a.enabled} onchange={(on) => updateEntry(a.id, (x) => (x.enabled = on))} />
            <!-- Label row of the entry; the on/off dot and the remove cross sit on its height -->
            <div class="labels" aria-hidden="true">
              <span>{t('card.source')}</span>
              <span></span>
              <span>{t('card.archive')}</span>
            </div>
            <FolderTile entry={a.id} side="source" path={a.source} placeholder={t('card.chooseSource')} onset={setSlot(a.id, 'source')} />
            <ModeFlow mode={a.mode} on={a.enabled} onselect={(mode) => updateEntry(a.id, (x) => (x.mode = mode))} />
            <FolderTile entry={a.id} side="archive" path={a.path} placeholder={t('card.chooseArchive')} onset={setSlot(a.id, 'archive')} />
            <div class="controls">
              {#if !isDraft}
                <!-- Last run as a dot; time and result on hover -->
                <span
                  class="state"
                  class:err={st && !st.ok}
                  class:never={!st}
                  role="img"
                  title={st ? `${relativeTime(st.last_run)} · ${st.message}` : t('common.neverSynced')}
                  aria-label={st ? `${relativeTime(st.last_run)} · ${st.message}` : t('common.neverSynced')}
                ></span>
              {/if}
            </div>
            <!-- Counterpart of the on/off dot: small in the top right corner -->
            <button
              class="remove"
              title={t('card.removeEntry')}
              aria-label={t('card.removeEntry')}
              onclick={() => removeEntry(a)}
            >
              <!-- Drawn instead of the ✕ character, so it sits exactly in the middle -->
              <svg viewBox="0 0 10 10" width="9" height="9" aria-hidden="true">
                <path d={CROSS} stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
              </svg>
            </button>
          </div>
        {/each}

        {#if !adding}
          <!-- The whole last row is the button; it comes out of the list when hovered -->
          <button type="button" class="add-row" onclick={() => (adding = true)}>
            <span class="plus" aria-hidden="true">
              <svg viewBox="0 0 24 24" width="18" height="18">
                <path fill="currentColor" d={ICONS.plus} />
              </svg>
            </span>
            {t('card.addEntry')}
          </button>
        {:else}
        <div class="lane add">
          <span class="add-label">{t('card.newEntry')}</span>
          <!-- Discards the new entry and shows the "Add entry" button again -->
          <button type="button" class="remove" title={t('common.cancel')} aria-label={t('common.cancel')} onclick={closeNew}>
            <svg viewBox="0 0 10 10" width="9" height="9" aria-hidden="true">
              <path d={CROSS} stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
            </svg>
          </button>
          <FolderTile entry={NEW} side="source" path={newSource} placeholder={t('card.chooseSource')} clearable onset={setSlot(NEW, 'source')} />
          <!-- Mode and archive only once the folder to back up is chosen -->
          <ModeFlow mode={newMode} on={canAdd} disabled={!newSource.trim()} onselect={(mode) => (newMode = mode)} />
          <FolderTile
            entry={NEW}
            side="archive"
            path={newPath}
            placeholder={t('card.chooseArchive')}
            clearable
            disabled={!newSource.trim()}
            onset={setSlot(NEW, 'archive')}
          />
          <div class="extra">
            <label
              class="create-folder"
              title={folderName ? t('card.createFolderHint', { name: folderName }) : t('card.createFolderFirst')}
            >
              <input type="checkbox" bind:checked={createFolder} disabled={!folderName} />
              {t('card.createFolder')}{#if folderName}<span class="muted">"{folderName}"</span>{/if}
            </label>
            {#if useFolder && newPath.trim()}
              <span class="result" title={finalPath}>→ {finalPath}</span>
            {/if}
          </div>
        </div>
        {/if}
      </div>
    </div>
  </section>
</div>

<FolderGhost />

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 24px;
  }
  .card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: var(--shadow);
  }
  header {
    display: flex;
    gap: 14px;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
  }
  h2 {
    margin: 0 0 10px;
    font-size: 15px;
  }
  /* Looks like the page titles (h1 in App.svelte); the field only shows on hover and focus */
  .name {
    flex: 1 1 320px;
    min-width: 0;
    min-height: 44px;
    font-size: 24px;
    font-weight: 700;
    letter-spacing: -0.02em;
    border-color: transparent;
    background: transparent;
    padding: 2px 8px;
    margin-left: -9px;
  }
  .name::placeholder {
    color: var(--muted);
    font-weight: 600;
  }
  .name:hover,
  .name:focus {
    border-color: var(--border);
    background: var(--input);
  }
  .actions {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .wrap {
    container: entries / inline-size;
  }
  /* All lanes share the columns, so the folders line up: source, flow, archive, settings */
  .entries {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr) auto;
  }
  /* Room in the top left corner for the on/off dot (EntrySwitch.svelte) */
  .lane {
    position: relative;
    grid-column: 1 / -1;
    display: grid;
    grid-template-columns: subgrid;
    gap: 6px 10px;
    align-items: center;
    padding: 10px 18px 14px 46px;
    border-bottom: 1px solid var(--border-soft);
  }
  /* 16 px high, so its middle is 18 px below the top of the entry (padding 10 px) */
  .labels,
  .add-label {
    grid-column: 1 / -1;
    height: 16px;
    line-height: 16px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
  }
  .labels {
    display: grid;
    grid-template-columns: subgrid;
  }
  .remove {
    position: absolute;
    /* Middle on the label row (18 px) */
    right: 8px;
    top: 8px;
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    min-height: 0;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--muted);
    opacity: 0.6;
    transition:
      opacity 0.12s,
      color 0.12s,
      background 0.12s;
  }
  .lane:hover .remove,
  .remove:focus-visible {
    opacity: 1;
  }
  .remove:hover:not(:disabled) {
    color: var(--danger);
    background: var(--danger-soft);
  }
  .controls {
    display: flex;
    align-items: center;
    gap: 10px;
    justify-content: flex-end;
  }
  .extra {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .state {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--ok);
    flex-shrink: 0;
  }
  .state.err {
    background: var(--danger);
  }
  .state.never {
    background: transparent;
    border: 1.5px solid var(--border);
  }
  .create-folder {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    cursor: pointer;
    white-space: nowrap;
  }
  .create-folder input {
    margin: 0;
  }
  .result {
    flex: 1 1 0;
    min-width: 0;
    font-family: var(--mono);
    font-size: 12px;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .lane.off > :global(:not(.controls):not(.dot):not(.remove)),
  .lane.off .controls > :global(:not(.danger)) {
    opacity: 0.5;
  }
  .lane.add {
    background: var(--surface-2);
    border-bottom: none;
    border-radius: 0 0 12px 12px;
  }
  /* Last row of the list, as wide as the entries above it (11 px = card radius minus its border) */
  .add-row {
    position: relative;
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 54px;
    padding: 10px 18px 10px 40px;
    border: none;
    border-radius: 0 0 11px 11px;
    background: transparent;
    color: var(--accent-text);
    font-weight: 600;
    text-align: left;
    transition:
      transform 0.22s cubic-bezier(0.34, 1.56, 0.64, 1),
      box-shadow 0.22s ease,
      background 0.22s ease,
      border-radius 0.22s ease;
  }
  /* Without entries it is the only row of the list */
  .add-row:first-child {
    border-radius: 11px;
  }
  .plus {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: var(--accent-soft);
    transition:
      transform 0.22s ease,
      background 0.22s ease,
      color 0.22s ease;
  }
  /* Hover: the row lifts out of the list (a little larger, with a shadow and a frame) and the
     plus turns into a filled circle */
  .add-row:hover:not(:disabled) {
    z-index: 1;
    transform: scale(1.02);
    border-radius: 12px;
    background: var(--surface);
    box-shadow:
      0 0 0 1px var(--accent),
      0 8px 22px rgb(0 0 0 / 0.14);
  }
  .add-row:hover:not(:disabled) .plus {
    transform: rotate(90deg);
    background: var(--accent);
    color: white;
  }
  @media (prefers-reduced-motion: reduce) {
    .add-row,
    .plus {
      transition: none;
    }
    .add-row:hover:not(:disabled) {
      transform: none;
    }
    .add-row:hover:not(:disabled) .plus {
      transform: none;
    }
  }
  .empty {
    margin: -2px 0 10px;
    color: var(--muted);
    font-size: 13px;
  }
  /* Narrower: settings move below the folders */
  @container entries (max-width: 760px) {
    .entries {
      grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
    }
    .controls {
      grid-column: 1 / -1;
    }
  }
  /* Very narrow: archive below its source, the flow between them */
  @container entries (max-width: 480px) {
    .entries {
      grid-template-columns: minmax(0, 1fr);
    }
    /* One column: only "Backup" stays on the label row */
    .labels span:not(:first-child) {
      display: none;
    }
  }
</style>
