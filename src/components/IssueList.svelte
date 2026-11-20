<script lang="ts">
  // Files and folders that were not transferred, each with its reason, what helps and its size.
  // `total`: how many there were in all; the backend stores only some of them, the rest is counted.
  // Used by the problems of the last run (ProblemsDialog) and by the activities (ActivityDialog).
  import type { Issue } from '../api/types.ts';
  import { t } from '../i18n/index.svelte.ts';
  import { formatBytes, formatCount } from '../utils/format.ts';
  import { ICONS } from '../utils/icons.ts';
  import { issuePath, objectIcon, reasonHint, reasonLabel } from '../utils/issues.ts';

  let { issues, total = issues.length }: { issues: Issue[]; total?: number } = $props();
</script>

<ul>
  {#each issues as issue, i (i)}
    <li>
      <svg class="obj" viewBox="0 0 24 24" width="16" height="16" role="img" aria-label={t(`issues.objects.${issue.object}`)}>
        <path fill="currentColor" d={objectIcon(issue)} />
      </svg>
      <span class="kind">{reasonLabel(issue)}</span>
      <span class="path mono">
        {issuePath(issue)}
        {#if issue.admin}
          <svg class="admin" viewBox="0 0 24 24" width="13" height="13" role="img" aria-label={t('issues.admin.badge')}>
            <title>{t('issues.admin.badge')}</title>
            <path fill="currentColor" d={ICONS.shield} />
          </svg>
        {/if}
        {#if reasonHint(issue)}<em class="note">{reasonHint(issue)}</em>{/if}
      </span>
      {#if issue.size}<span class="size">{formatBytes(issue.size)}</span>{/if}
    </li>
  {/each}
  {#if total > issues.length}
    <li class="muted">{t('preview.more', { count: formatCount(total - issues.length) })}</li>
  {/if}
</ul>

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0 14px;
    max-height: 360px;
    overflow-y: auto;
  }
  li {
    display: flex;
    gap: 10px;
    align-items: baseline;
    padding: 6px 0;
    border-bottom: 1px solid var(--border-soft);
    font-size: 13px;
  }
  li:last-child {
    border-bottom: none;
  }
  .obj {
    flex: none;
    align-self: center;
    color: var(--muted);
  }
  .kind {
    flex: 0 0 132px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--danger);
  }
  .path {
    flex: 1;
    min-width: 0;
    word-break: break-all;
  }
  .note {
    display: block;
    color: var(--muted);
    font-family: var(--sans);
    font-size: 12px;
    word-break: normal;
  }
  .admin {
    margin-left: 6px;
    vertical-align: -2px;
    color: var(--warn);
  }
  .size {
    flex-shrink: 0;
    color: var(--muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
</style>
