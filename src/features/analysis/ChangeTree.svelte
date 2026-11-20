<script lang="ts">
  import { untrack } from 'svelte';
  import { SvelteMap, SvelteSet } from 'svelte/reactivity';
  import { backend } from '../../api/backend.ts';
  import ContextMenu, { type MenuItem } from '../../components/ContextMenu.svelte';
  import { showError } from '../../stores/notice.svelte.ts';
  import type { Plan, Row, RowFilter, TreeLevel, TreeNode } from '../../api/types.ts';
  import { formatBytes, formatCount } from '../../utils/format.ts';
  import { t, type TextKey } from '../../i18n/index.svelte.ts';
  import { ICONS } from '../../utils/icons.ts';
  import { isFailure, issuePath, objectIcon, reasonHint, reasonLabel } from '../../utils/issues.ts';

  // The changes of one pipeline as a folder tree: every folder shows how many changes of which kind
  // lie below it, opened it shows its folders and files. The backend groups the rows
  // (src-tauri/src/sync/changes.rs) and sends one level at a time: the root first, with every folder
  // closed, the level of a folder when it is opened, so a plan with a million rows stays quick.
  // Shown as a flat list of lines, each indented by its depth.
  // A right click (or the menu key) on a line opens a menu that shows it in the file manager, in
  // the source or in the archive (commands/dialog.rs, reveal_path).
  interface Props {
    plan: Pick<Plan, 'id' | 'probed' | 'source' | 'dest'>;
    filter: RowFilter;
    search: string;
    /** Some folder is open (for the "Collapse all" button of PipelineReport). */
    canCollapse?: boolean;
    /**
     * Where the levels come from instead of the plan's analysis: the live tree of a running sync
     * (RunDialog). Filter and search do not apply to it.
     */
    levelOf?: (dir: string) => Promise<TreeLevel>;
    /** Changes when the tree has grown: it is fetched again, opened folders stay open. */
    version?: number;
    /** Shown while the tree is empty (default: no matching entries). */
    empty?: string;
  }
  let { plan, filter, search, canCollapse = $bindable(false), levelOf, version = 0, empty }: Props = $props();

  type Folder = Extract<TreeNode, { type: 'folder' }>;
  type Entry = Extract<TreeNode, { type: 'entry' }>;
  type Line =
    | { type: 'folder'; node: Folder; depth: number; open: boolean }
    | { type: 'entry'; node: Entry; depth: number }
    | { type: 'loading'; path: string; depth: number }
    | { type: 'more'; count: number; depth: number; parent: string };

  // CSS class per kind (the colours of the figure cards); the label comes from preview.kinds.<kind>
  const CLASSES: Record<Row['kind'], string> = {
    blocked: 'blk',
    copy: 'add',
    update: 'upd',
    move: 'mov',
    move_dir: 'mov',
    delete: 'del',
    delete_dir: 'del',
    create_dir: 'dir',
  };
  // What a folder holds, per class in this order, worded like the figure cards
  const TALLIES: [string, TextKey][] = [
    ['blk', 'preview.stats.blocked'],
    ['add', 'preview.stats.new'],
    ['upd', 'preview.stats.updated'],
    ['mov', 'preview.stats.moved'],
    ['del', 'preview.stats.deleted'],
    ['dir', 'preview.tree.created'],
  ];

  function tallies(node: Folder) {
    const sums: Record<string, number> = {};
    for (const [kind, n] of Object.entries(node.kinds)) {
      const cls = CLASSES[kind as Row['kind']];
      sums[cls] = (sums[cls] ?? 0) + (n ?? 0);
    }
    return TALLIES.filter(([cls]) => sums[cls]).map(([cls, label]) => ({ cls, n: sums[cls], label }));
  }

  // The root level; levels fetched when a folder was opened; the folders the user opened
  let root = $state.raw<TreeLevel | null>(null);
  const loaded = new SvelteMap<string, TreeLevel>();
  const opened = new SvelteSet<string>();
  const pending = new SvelteSet<string>();
  // Answers of an older tree (other filter, search or probe) are dropped
  let generation = 0;
  let shownPlan = '';

  /** A level of the tree; empty when the plan is gone. */
  async function request(id: string, f: RowFilter, q: string, dir: string): Promise<TreeLevel> {
    try {
      return await (levelOf ? levelOf(dir) : backend.planTree(id, f, q, dir));
    } catch {
      return { nodes: [], more: 0 };
    }
  }

  // Fetched again when plan, filter or search change (typing waits a moment first) and after the
  // probe. Opened folders stay open: their levels are fetched along with the root and all are
  // swapped at once, so nothing flickers. Only another plan starts afresh.
  $effect(() => {
    const id = plan.id;
    const f = filter;
    const q = search;
    void plan.probed; // the probe moves entries into "not transferable"
    void version; // a running sync has done more
    const gen = ++generation;
    const timer = setTimeout(
      async () => {
        const paths = id === shownPlan ? [...opened] : [];
        const [level, ...levels] = await Promise.all([request(id, f, q, ''), ...paths.map((p) => request(id, f, q, p))]);
        if (gen !== generation) return;
        if (id !== shownPlan) opened.clear();
        shownPlan = id;
        loaded.clear();
        paths.forEach((p, i) => loaded.set(p, levels[i]));
        pending.clear();
        root = level;
      },
      q ? 200 : 0,
    );
    return () => clearTimeout(timer);
  });

  async function load(path: string) {
    if (pending.has(path)) return;
    pending.add(path);
    const gen = generation;
    const level = await request(plan.id, filter, search, path);
    if (gen !== generation) return;
    loaded.set(path, level);
    pending.delete(path);
  }

  /** Fetches the level of an opened folder once its "Loading" line is shown. */
  function fetchLevel(_: HTMLElement, path: string) {
    untrack(() => load(path));
  }

  const lines = $derived.by(() => {
    const out: Line[] = [];
    const walk = (level: TreeLevel, depth: number, parent: string) => {
      for (const node of level.nodes) {
        if (node.type === 'entry') {
          out.push({ type: 'entry', node, depth });
          continue;
        }
        const open = node.count > 0 && opened.has(node.path);
        out.push({ type: 'folder', node, depth, open });
        if (!open) continue;
        const below = loaded.get(node.path);
        if (below) walk(below, depth + 1, node.path);
        else out.push({ type: 'loading', path: node.path, depth: depth + 1 });
      }
      if (level.more) out.push({ type: 'more', count: level.more, depth, parent });
    };
    if (root) walk(root, 0, '');
    return out;
  });
  $effect(() => {
    canCollapse = lines.some((l) => l.type === 'folder' && l.open);
  });

  /**
   * Opens or closes a folder. A chain ("a/b/c") counts as each of its folders, so it stays open when
   * another filter shows it split up or merged otherwise.
   */
  function toggle(node: Folder, open: boolean) {
    let path = node.path.slice(0, node.path.length - node.name.length);
    for (const part of node.name.split('/')) {
      path += part;
      if (open) opened.add(path);
      else opened.delete(path);
      path += '/';
    }
  }

  /** Closes every folder, also those that another filter or search would show. */
  export function collapseAll() {
    opened.clear();
  }

  /** Unique per line: a path can be a folder and a row (or a blocked entry and an operation) at once. */
  function key(line: Line) {
    switch (line.type) {
      case 'folder':
        return `d:${line.node.path}`;
      case 'entry':
        return `f:${line.node.row.kind}:${line.node.row.path}`;
      case 'loading':
        return `l:${line.path}`;
      case 'more':
        return `m:${line.parent}`;
    }
  }

  // The open menu of a line: where, and what it offers
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  async function reveal(root: string, paths: string[]) {
    try {
      await backend.revealPath(root, paths);
    } catch (e) {
      showError(e);
    }
  }

  /**
   * Menu of a line. `source` / `archive`: where it lies there, relative to the root; for a move
   * in the archive its new path, then its old one (before the run only that one exists).
   */
  function openMenu(e: MouseEvent, source: string, archive: string[]) {
    e.preventDefault();
    let { clientX: x, clientY: y } = e;
    // Opened with the keyboard: below the line instead of at the pointer
    if (!x && !y) {
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
      x = r.left + 24;
      y = r.bottom;
    }
    menu = {
      x,
      y,
      items: [
        { label: t('preview.tree.showSource'), icon: ICONS.folder, onselect: () => reveal(plan.source, [source]) },
        { label: t('preview.tree.showArchive'), icon: ICONS.archive, onselect: () => reveal(plan.dest, archive) },
      ],
    };
  }

  /** Where a line lies: in the source, and in the archive (a move also at its old path). */
  function places(line: Line): [string, string[]] | null {
    if (line.type === 'folder') {
      const from = line.node.row?.from;
      return [line.node.path, from ? [line.node.path, from] : [line.node.path]];
    }
    if (line.type === 'entry') {
      const { path, from } = line.node.row;
      return [path, from ? [path, from] : [path]];
    }
    return null;
  }

  const dirname = (p: string) => p.slice(0, Math.max(p.lastIndexOf('/'), 0));
  const basename = (p: string) => p.slice(p.lastIndexOf('/') + 1);
  /** Where a move comes from: only the old name when it stays in its folder. */
  const fromText = (row: Row) => (row.from && dirname(row.from) === dirname(row.path) ? basename(row.from) : row.from);
</script>

{#snippet icon(path: string, cls: string)}
  <svg class="icon {cls}" viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="currentColor" d={path} /></svg>
{/snippet}

{#snippet chevron(open: boolean)}
  <svg class="chevron" class:open viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
    <path fill="currentColor" d={ICONS.next} />
  </svg>
{/snippet}

<!-- The name of a folder or file; a chain of folders with muted separators -->
{#snippet name(text: string)}
  {#each text.split('/') as part, i}{#if i}<span class="sep">/</span>{/if}{part}{/each}
{/snippet}

<!-- What a row changes: moved from where, and the note or why it cannot be transferred -->
{#snippet details(row: Row)}
  {#if row.kind === 'blocked'}
    {#if row.issue.admin}
      <svg class="admin" viewBox="0 0 24 24" width="13" height="13" role="img" aria-label={t('issues.admin.badge')}>
        <title>{t('issues.admin.badge')}</title>
        <path fill="currentColor" d={ICONS.shield} />
      </svg>
    {/if}
    {#if reasonHint(row.issue)}<em class="note">{reasonHint(row.issue)}</em>{/if}
  {:else}
    {#if row.from}
      <span class="from" title={t('preview.tree.movedFrom', { path: row.from })}>← {fromText(row)}</span>
    {/if}
    {#if row.note}<em class="note">{row.note}</em>{/if}
  {/if}
{/snippet}

{#snippet label(row: Row)}
  {#if row.kind === 'blocked'}
    <span class="kind blk">{reasonLabel(row.issue)}</span>
  {:else}
    <span class="kind {CLASSES[row.kind]}">{t(`preview.kinds.${row.kind}`)}</span>
  {/if}
{/snippet}

{#snippet folderLine(line: Extract<Line, { type: 'folder' }>)}
  {@const node = line.node}
  <span class="indent" aria-hidden="true"></span>
  {#if node.count}{@render chevron(line.open)}{:else}<span class="chevron"></span>{/if}
  {@render icon(ICONS.folder, node.row ? CLASSES[node.row.kind] : 'plain')}
  <span class="main">
    <span class="name mono">{@render name(node.name)}</span>
    {#if node.row}{@render details(node.row)}{/if}
  </span>
  <span class="tags">
    {#if node.row}{@render label(node.row)}{/if}
    {#each tallies(node) as tally (tally.cls)}
      <span class="tally {tally.cls}" title="{formatCount(tally.n)} {t(tally.label)}"><i></i>{formatCount(tally.n)}</span>
    {/each}
  </span>
  <span class="size">{node.bytes ? formatBytes(node.bytes) : ''}</span>
{/snippet}

<!-- Long trees scroll on their own, so the next pipeline stays within reach -->
<ul class="tree">
  {#each lines as line (key(line))}
    {@const at = places(line)}
    <li style:--depth={line.depth} oncontextmenu={at ? (e) => openMenu(e, ...at) : undefined}>
      {#if line.type === 'folder'}
        {#if line.node.count}
          <button class="line folder" aria-expanded={line.open} onclick={() => toggle(line.node, !line.open)}>
            {@render folderLine(line)}
          </button>
        {:else}
          <div class="line folder">{@render folderLine(line)}</div>
        {/if}
      {:else if line.type === 'entry'}
        {@const row = line.node.row}
        <div class="line" class:skipped={row.kind === 'blocked' && !isFailure(row.issue)}>
          <span class="indent" aria-hidden="true"></span>
          <span class="chevron"></span>
          {@render icon(row.kind === 'blocked' ? objectIcon(row.issue) : ICONS.document, CLASSES[row.kind])}
          <span class="main">
            <!-- Only the source folder itself has no name -->
            <span class="name mono">{line.node.name || (row.kind === 'blocked' ? issuePath(row.issue) : '')}</span>
            {@render details(row)}
          </span>
          <span class="tags">{@render label(row)}</span>
          <span class="size">
            {#if row.kind === 'blocked'}
              {t(`issues.objects.${row.issue.object}`)}{#if row.size} · {formatBytes(row.size)}{/if}
            {:else if row.kind === 'move_dir'}
              {t('preview.files', { count: row.files, n: formatCount(row.files) })} · {formatBytes(row.size)}
            {:else if !row.kind.endsWith('_dir')}
              {formatBytes(row.size)}
            {/if}
          </span>
        </div>
      {:else if line.type === 'loading'}
        <div class="line muted" use:fetchLevel={line.path}>
          <span class="indent" aria-hidden="true"></span>
          <span class="chevron"></span>
          <span class="spinner" aria-hidden="true"></span>
          <span class="main">{t('preview.tree.loading')}</span>
        </div>
      {:else}
        <div class="line muted">
          <span class="indent" aria-hidden="true"></span>
          <span class="chevron"></span>
          <span class="main">{t('preview.more', { count: formatCount(line.count) })}</span>
        </div>
      {/if}
    </li>
  {:else}
    <li class="line muted empty">{root ? (empty ?? t('preview.noEntries')) : t('preview.tree.loading')}</li>
  {/each}
</ul>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

<style>
  .tree {
    border-top: 1px solid var(--border-soft);
    --step: 18px;
    list-style: none;
    margin: 0;
    padding: 4px 6px;
    max-height: 360px;
    overflow-y: auto;
  }
  /* One line: indent | chevron | icon | name and details | kind and tallies | size */
  .line {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    min-height: 30px;
    padding: 0 8px 0 0;
    font-size: 13px;
    text-align: left;
    border-radius: 6px;
  }
  button.line {
    border: none;
    background: none;
    cursor: pointer;
  }
  button.line:hover:not(:disabled) {
    background: var(--surface-2);
  }
  button.line:focus-visible {
    outline-offset: -2px;
  }
  .line.empty {
    padding: 10px 8px;
  }
  /* One guide per level, through the middle of the chevrons above */
  .indent {
    flex: none;
    align-self: stretch;
    width: calc(var(--depth) * var(--step));
    margin-right: -6px;
    background: repeating-linear-gradient(
      to right,
      transparent 0 calc(var(--step) / 2 - 0.5px),
      var(--border) calc(var(--step) / 2 - 0.5px) calc(var(--step) / 2 + 0.5px),
      transparent calc(var(--step) / 2 + 0.5px) var(--step)
    );
  }
  .chevron {
    flex: none;
    width: var(--step);
    height: 16px;
    color: var(--muted);
    transition: transform 0.15s ease;
  }
  .chevron.open {
    transform: rotate(90deg);
  }
  @media (prefers-reduced-motion: reduce) {
    .chevron {
      transition: none;
    }
  }
  .icon {
    flex: none;
    color: var(--muted);
  }
  .main {
    flex: 1;
    min-width: 0;
    padding: 5px 0;
  }
  .name {
    word-break: break-all;
  }
  .folder .name {
    font-weight: 600;
  }
  .sep {
    margin: 0 1px;
    color: var(--muted);
    font-weight: 400;
  }
  .from {
    margin-left: 8px;
    color: var(--muted);
    font-size: 12px;
    word-break: break-all;
  }
  .note {
    display: block;
    color: var(--warn);
    font-family: var(--sans);
    font-size: 12px;
  }
  .admin {
    margin-left: 6px;
    vertical-align: -2px;
    color: var(--warn);
  }
  .tags {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .kind {
    font-size: 11.5px;
    font-weight: 600;
  }
  /* Changes below a folder: a dot in the colour of their kind and their number */
  .tally {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .tally i {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--c);
  }
  .size {
    flex: none;
    min-width: 64px;
    text-align: right;
    color: var(--muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .add {
    --c: var(--ok);
  }
  .upd {
    --c: var(--warn);
  }
  .mov {
    --c: var(--accent);
  }
  .del,
  .blk {
    --c: var(--danger);
  }
  .dir {
    --c: var(--muted);
  }
  .icon.add,
  .icon.upd,
  .icon.del,
  .icon.blk,
  .kind {
    color: var(--c);
  }
  .icon.mov,
  .kind.mov {
    color: var(--accent-text);
  }
  .icon.plain {
    color: color-mix(in srgb, var(--amber, var(--accent)) 70%, var(--muted));
  }
  /* Links are skipped on purpose: listed, but not in red */
  .skipped .kind,
  .skipped .icon,
  .skipped .note {
    color: var(--muted);
  }
  .spinner {
    flex: none;
    width: 12px;
    height: 12px;
    margin: 0 2px;
    border: 2px solid var(--accent-soft);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation-duration: 2.4s;
    }
  }
</style>
