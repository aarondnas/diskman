<script lang="ts">
  // Subpage of the list: editing a backup, or creating one. A new backup is a draft until its first
  // entry is complete; then it is saved and edited like any other (stores/backups.svelte.ts). One
  // editor for both, so it stays the same when the draft becomes a backup.
  import { t } from '../../i18n/index.svelte.ts';
  import { removeBackup, updateDraft } from '../../stores/backups.svelte.ts';
  import { defaultBackupName, updateBackup } from '../../stores/config.svelte.ts';
  import { nav } from '../../stores/navigation.svelte.ts';
  import BackupEditor from './BackupEditor.svelte';

  const editing = $derived(nav.editing);
  // App.svelte shows this page only in the editor, so one of them is there
  const backup = $derived(editing ?? nav.draft);
</script>

{#if backup}
  <BackupEditor
    {backup}
    isDraft={!editing}
    namePlaceholder={editing ? undefined : defaultBackupName()}
    onupdate={(mutate) => (editing ? updateBackup(editing.id, mutate) : updateDraft(mutate))}
    onremove={() => editing && removeBackup(editing)}
  />
{/if}

{#if !editing}
  <div class="welcome">
    <!-- Own texts with <b> tags, no user input -->
    <p>{@html t('welcome.intro')}</p>
    <p class="muted">{@html t('welcome.modes')}</p>
  </div>
{/if}

<style>
  .welcome {
    margin-top: 20px;
    border: 1px dashed var(--border);
    border-radius: 12px;
    padding: 28px;
    text-align: center;
  }
  .welcome p {
    max-width: 640px;
    margin: 10px auto;
  }
</style>
