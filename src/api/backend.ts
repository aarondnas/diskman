// The only place that talks to the Rust backend: one function per Tauri command
// (src-tauri/src/commands). Errors arrive as Error with the backend's message.
import { invoke, type InvokeArgs } from '@tauri-apps/api/core';
import type {
  Activity,
  ActivityQuery,
  Autostart,
  CheckProgress,
  Config,
  EntryDetail,
  Id,
  JobProgress,
  MissedRun,
  Plan,
  PlanStorage,
  PlatformInfo,
  RowFilter,
  RunProblem,
  StatusMap,
  TreeLevel,
  UpdateStatus,
} from './types.ts';

async function call<T>(command: string, args?: InvokeArgs): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (e) {
    throw new Error(typeof e === 'string' ? e : ((e as Error | null)?.message ?? String(e)));
  }
}

export const backend = {
  // Configuration (commands/config.rs)
  getConfig: () => call<Config>('get_config'),
  saveConfig: (config: Config) => call<Config>('save_config', { config }),
  dataDir: () => call<string>('get_data_dir'),

  // Checks, runs and their progress (commands/sync.rs)
  status: () => call<StatusMap>('get_status'),
  runProblems: (backupId: Id) => call<RunProblem[]>('get_run_problems', { backupId }),
  /**
   * The plans of a check, or null if it was cancelled (`cancelChecks`). `batch`: number of the
   * "Check all" it belongs to (its backups share scans), ended with `endCheckBatch`.
   */
  preview: (backupId: Id, batch: number | null = null) => call<Plan[] | null>('preview', { backupId, batch }),
  endCheckBatch: (batch: number) => call<void>('end_check_batch', { batch }),
  /** Second step: opens the files to copy; the plans with what it found, or null (cancelled, replaced). */
  probePlans: (backupId: Id) => call<Plan[] | null>('probe_plans', { backupId }),
  /** One level (folder `dir`, "" = the root) of the tree of changes of a plan the backend keeps. */
  planTree: (planId: string, filter: RowFilter, query: string, dir: string) =>
    call<TreeLevel>('plan_tree', { planId, filter, query, dir }),
  /** One level of the tree of what the running sync has done so far in an entry (it grows). */
  runTree: (planId: string, dir: string) => call<TreeLevel>('run_tree', { planId, dir }),
  /** The backend may forget the plans of these backups. */
  dropPlans: (backupIds: Id[]) => call<void>('drop_plans', { backupIds }),
  cancelChecks: (backupIds: Id[]) => call<void>('cancel_checks', { backupIds }),
  /** Files and folders the running checks of these backups have read so far (others are left out). */
  checkProgress: (backupIds: Id[]) => call<Record<Id, CheckProgress>>('check_progress', { backupIds }),
  runBackup: (backupId: Id) => call<JobProgress>('run_backup', { backupId }),
  job: () => call<JobProgress>('get_job'),
  cancel: () => call<void>('cancel_job'),
  cancelBackups: (backupIds: Id[]) => call<JobProgress>('cancel_backups', { backupIds }),

  // Analyses and runs of the past (commands/activity.rs)
  activities: (query: ActivityQuery) => call<Activity[]>('get_activities', { query }),
  /**
   * Loads the stored plans of an activity for its entries; their trees come from `planTree` until
   * `closeActivity` or the next `openActivity`.
   */
  openActivity: (id: number, entries: number[]) => call<EntryDetail[]>('open_activity', { id, entries }),
  closeActivity: () => call<void>('close_activity'),
  activityStorage: () => call<PlanStorage>('activity_storage'),
  /** Deletes the whole history, every analysis and run (one still running stays); how many went. */
  clearActivities: () => call<number>('clear_activities'),
  /** Deletes every stored file list (the activities stay); what is left afterwards. */
  clearActivityPlans: () => call<PlanStorage>('clear_activity_plans'),

  // Runs missed while Diskman was off (commands/catch_up.rs)
  missedRuns: () => call<MissedRun[]>('get_missed_runs'),
  runMissed: (backupIds: Id[]) => call<void>('run_missed', { backupIds }),
  dismissMissed: () => call<void>('dismiss_missed'),

  // Starting at login (commands/startup.rs)
  autostart: () => call<Autostart>('get_autostart'),
  setAutostart: (enabled: boolean) => call<Autostart>('set_autostart', { enabled }),

  // System dialogs (commands/dialog.rs)
  /** The chosen folder, or null when the dialog was cancelled. */
  pickFolder: () => call<string | null>('pick_folder'),
  /**
   * Shows the first of `paths` (relative to the folder `root`, with "/") that exists in the file
   * manager, selected; if none exists, the nearest folder above the first one.
   */
  revealPath: (root: string, paths: string[]) => call<void>('reveal_path', { root, paths }),

  // New versions of Diskman (commands/update.rs)
  updateStatus: () => call<UpdateStatus>('get_update_status'),
  /** Asks GitHub now. */
  checkForUpdate: () => call<UpdateStatus>('check_for_update'),
  /** Installs the update found; Diskman ends and starts again. Only returns (rejects) if it failed. */
  installUpdate: () => call<void>('install_update'),
  /** The website, where new versions are downloaded and installed over the old one by hand. */
  openWebsite: () => call<void>('open_website'),

  // The system and administrator rights (commands/admin.rs)
  platform: () => call<PlatformInfo>('get_platform'),
  /** False if Windows' permission prompt was declined. */
  restartAsAdmin: () => call<boolean>('restart_as_admin'),
};
