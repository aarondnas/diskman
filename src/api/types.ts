// What the Rust backend sends and receives (serde JSON). Each type names the Rust type it mirrors;
// a change there has to be made here as well. Times are ISO 8601 strings (chrono DateTime<Utc>).

/** Id of a backup or of an archive (a UUID). */
export type Id = string;

// ---- Configuration (src-tauri/src/model/config.rs) ----

/** `Mode`: how an archive is synced with its backup. */
export type Mode = 'mirror' | 'additive';

/** `Archive`: one entry of a backup, a source folder with its archive. */
export interface Archive {
  id: Id;
  source: string;
  path: string;
  mode: Mode;
  enabled: boolean;
}

/** `Schedule`: on the chosen weekdays (1 = Monday … 7 = Sunday) at "HH:MM", or every n minutes. */
export type Schedule = { kind: 'daily'; time: string; days: number[] } | { kind: 'interval'; minutes: number };

/** `BackupSchedule`: paused schedules keep their timing but do not run. */
export interface BackupSchedule {
  when: Schedule;
  enabled: boolean;
  catch_up: boolean;
}

/** `Backup`: a named list of entries with an optional schedule and colour. */
export interface Backup {
  id: Id;
  name: string;
  archives: Archive[];
  schedule?: BackupSchedule;
  /** "#rrggbb"; without one the amber of the logo. */
  color?: string;
}

/** `Settings` */
export interface Settings {
  mtime_tolerance_ms: number;
  /** UI language code; without one (or null) the system language. */
  language?: string | null;
  autostart_asked?: boolean;
  light_mode?: boolean;
  /** Read every copy back and compare it with its source (slower). */
  verify_copies?: boolean;
  /** How much the file lists of the activities may take, in MB; the oldest go first. */
  activity_limit_mb: number;
  /** Look for a new version once a day (on unless false). */
  check_updates?: boolean;
}

/** `UpdateStatus`: what is known about a new version of Diskman (src-tauri/src/updates.rs). */
export interface UpdateStatus {
  current: string;
  /** A newer version on GitHub, if the last check found one. */
  available: { version: string; notes: string | null } | null;
  checked_at: string | null;
  checking: boolean;
  /** Downloading and installing; Diskman ends when it is done. */
  installing: boolean;
  downloaded: number;
  total: number | null;
  /** Why the last check or install failed (the backend's text). */
  error: string | null;
  /** This copy can update itself (an installed build, not one started from the sources). */
  can_install: boolean;
  /**
   * Updates no longer come from GitHub with the known key, the new version has to come from the
   * website: `missing` – GitHub has no release (any more); `untrusted` – the update found is
   * signed with another key and was not installed.
   */
  lost: 'missing' | 'untrusted' | null;
}

/** `Config` */
export interface Config {
  backups: Backup[];
  settings: Settings;
}

// ---- Checks (src-tauri/src/sync/planner.rs, issues.rs) ----

/** `Reason`: why an entry is not (or was not) transferred. */
export type Reason =
  | 'link'
  | 'special'
  | 'access_denied'
  | 'locked'
  | 'invalid_name'
  | 'name_not_allowed'
  | 'case_conflict'
  | 'too_large'
  | 'encrypted'
  | 'disk_full'
  | 'vanished'
  | 'other';

/** `Object`: kind of the entry. */
export type ObjectKind = 'file' | 'folder' | 'link' | 'special';

/** `Issue`: one entry that is not (or was not) transferred. */
export interface Issue {
  /** Relative to the source, with "/"; "" is the source folder itself. */
  path: string;
  object: ObjectKind;
  reason: Reason;
  /** The system's error message, or the other path of a case conflict. */
  detail?: string;
  size: number;
  /** Administrator rights may solve it. */
  admin: boolean;
}

/** `OpKind` */
export type OpKind = 'delete' | 'move_dir' | 'move' | 'delete_dir' | 'create_dir' | 'copy' | 'update';

/** `Op`: one operation of a plan. */
export interface Op {
  kind: OpKind;
  path: string;
  /** Original path in the archive for moves. */
  from?: string;
  size: number;
  files: number;
  note?: string;
}

/** `Summary`: totals of a plan. */
export interface PlanSummary {
  copy_files: number;
  copy_bytes: number;
  update_files: number;
  update_bytes: number;
  move_files: number;
  move_bytes: number;
  dirs_moved: number;
  delete_files: number;
  delete_bytes: number;
  dirs_created: number;
  dirs_deleted: number;
  unchanged: number;
  protected: number;
  blocked: number;
}

/** `Warning`: something about a plan worth knowing before running it (worded by the UI). */
export type PlanWarning =
  | { kind: 'archive_missing' }
  | { kind: 'source_unreadable' }
  | { kind: 'archive_unreadable'; count: number }
  | { kind: 'source_empty' }
  | { kind: 'many_deletes'; count: number; total: number };

/** `ScanMode`: how a scan came to its result (src-tauri/src/sync/tree.rs). */
export type ScanMode = 'full' | 'incremental' | 'shared';

/** The filters of the list in the analysis (`ROW_FILTERS`). */
export type RowFilter = 'all' | 'copy' | 'update' | 'move' | 'delete' | 'dirs' | 'blocked';

/**
 * `PlanView`: what a sync of one archive would change. The backend keeps the plan itself; its
 * operations and blocked entries are fetched as a tree, level by level (`backend.planTree`).
 */
export interface Plan {
  id: string;
  backup_id: Id;
  archive_id: Id;
  backup_name: string;
  source: string;
  dest: string;
  mode: Mode;
  summary: PlanSummary;
  warnings: PlanWarning[];
  /** Would (probably unintentionally) empty the whole archive. */
  dangerous: boolean;
  scan_ms: number;
  diff_ms: number;
  created_at: string;
  src_scan_ms: number;
  src_scan: ScanMode;
  dst_scan_ms: number;
  /** null: the archive does not exist yet */
  dst_scan: ScanMode | null;
  /** Files and folders (below the root) of source and archive that were compared. */
  src_files: number;
  src_dirs: number;
  dst_files: number;
  dst_dirs: number;
  /** The files to copy were opened to find locked and unreadable ones (second step of a check). */
  probed: boolean;
  probe_ms: number | null;
  /** Number of operations. */
  ops: number;
  /** Rows of the list per filter. */
  counts: Record<RowFilter, number>;
  /** Blocked entries that mean something is missing (not links or special files). */
  blocked_failures: number;
  /** Blocked entries administrator rights may make readable. */
  blocked_admin: number;
  blocked_bytes: number;
}

/** `CheckProgress`: files and folders a running check has read so far (commands/sync.rs). */
export interface CheckProgress {
  files: number;
  dirs: number;
}

/** A row of the list: an operation, or an entry that cannot be transferred. */
export type Row = Op | { kind: 'blocked'; path: string; size: number; from?: undefined; issue: Issue };

/** `TreeLevel`: one level of the tree of changes (src-tauri/src/sync/changes.rs). */
export interface TreeLevel {
  /** Folders first, then files, each sorted by name. */
  nodes: TreeNode[];
  /** Entries of this level that are not listed (too many). */
  more: number;
}

/** `TreeNode`: a folder with what changes below it, or one row of the list. */
export type TreeNode =
  | {
      type: 'folder';
      /** Relative to its level; a chain of folders that only hold the next one is one node ("a/b/c"). */
      name: string;
      /** Relative to the root, to ask for its level (`backend.planTree`). */
      path: string;
      /** The change of the folder itself. */
      row?: Row;
      /** Rows below it, their size and their number per kind. */
      count: number;
      bytes: number;
      kinds: Partial<Record<Row['kind'], number>>;
    }
  | {
      type: 'entry';
      /** File name; "" for the source folder itself. */
      name: string;
      row: Row;
    };

// ---- Runs (src-tauri/src/jobs, storage/history.rs) ----

/** `JobProgress`: the running sync and the waiting queue. */
export interface JobProgress {
  running: boolean;
  origin: 'manual' | 'schedule' | 'catch_up' | '';
  backup_id: Id;
  queued: Id[];
  label: string;
  total_ops: number;
  done_ops: number;
  /** Bytes to copy and copied so far (new and updated files). */
  total_bytes: number;
  done_bytes: number;
  /** What could not be transferred so far. */
  error_count: number;
  /** The activity of this run (its details once it is done); null if not stored. */
  run_id: number | null;
  started_at: string | null;
  /** When the scans were done and the operations began (speed and time left count from here). */
  ops_started_at: string | null;
  /** Path of the operation being carried out, relative to its entry. */
  current: string;
  /** The entries of the run in order; empty while scanning. */
  entries: LiveEntry[];
}

/** `EntryState`: where an entry of the running sync stands (jobs/mod.rs). */
export type EntryState = 'waiting' | 'running' | 'done' | 'cancelled' | 'skipped';

/** `LiveEntry`: one entry of the running sync; its first `done_ops` operations are done. */
export interface LiveEntry {
  plan_id: string;
  archive_id: Id;
  source: string;
  dest: string;
  mode: Mode;
  state: EntryState;
  summary: PlanSummary;
  total_ops: number;
  done_ops: number;
  total_bytes: number;
  done_bytes: number;
  error_count: number;
}

/** `ArchiveStatus`: latest result of an archive. */
export interface ArchiveStatus {
  last_run: string;
  ok: boolean;
  message: string;
}

/** Latest result per archive id. */
export type StatusMap = Record<Id, ArchiveStatus>;

/** `RunProblem`: latest result of an archive that did not end well. */
export interface RunProblem {
  archive_id: Id;
  dest: string;
  /** interrupted: Diskman ended during the run (quit, crash, power cut). */
  outcome: 'errors' | 'cancelled' | 'skipped' | 'failed' | 'interrupted';
  message: string;
  finished_at: string;
  /** All failures; `issues` holds only some of them. */
  error_count: number;
  issues: Issue[];
}

// ---- Activities (src-tauri/src/storage/history.rs, commands/activity.rs) ----

/** How an activity came about: started by hand, by its schedule, caught up after a missed one. */
export type ActivityOrigin = 'manual' | 'schedule' | 'catch_up' | 'import';

/** Result of an activity or of one of its entries. */
export type ActivityOutcome = 'ok' | 'errors' | 'cancelled' | 'skipped' | 'failed' | 'interrupted';

/** `ActivityEntry`: one entry (source → archive) of an activity. */
export interface ActivityEntry {
  /** Id of its result, to open its plan and errors (`backend.openActivity`). */
  id: number;
  archive_id: Id;
  source: string;
  dest: string;
  mode: Mode | null;
  outcome: ActivityOutcome;
  /** The backend's text (English), e.g. why it failed. */
  message: string;
  started_at: string;
  finished_at: string;
  diff_ms: number | null;
  copy_ms: number | null;
  ops_total: number;
  ops_done: number;
  bytes_total: number;
  bytes_done: number;
  summary: PlanSummary;
  error_count: number;
  /** Its file list is still stored (the oldest go when the limit is reached). */
  has_plan: boolean;
}

/** `Activity`: an analysis (check) or a run (sync) of a backup. */
export interface Activity {
  id: number;
  kind: 'sync' | 'check';
  origin: ActivityOrigin;
  backup_id: Id;
  /** As of the activity. */
  backup_name: string;
  started_at: string;
  /** null while it runs */
  finished_at: string | null;
  /** null while it runs; 'errors' also when an entry was skipped */
  outcome: Exclude<ActivityOutcome, 'skipped'> | null;
  scan_ms: number | null;
  entries: ActivityEntry[];
}

/** `ActivityQuery`: which activities to list. */
export interface ActivityQuery {
  backup_id?: Id | null;
  kind?: Activity['kind'] | null;
  /** Only older than this activity (the next page). */
  before?: number | null;
  limit: number;
}

/** `EntryDetail`: an entry of an opened activity. */
export interface EntryDetail {
  id: number;
  /** null: its file list is no longer stored, or there never was one (it failed). */
  plan: Plan | null;
  errors: Issue[];
}

/** `PlanStorage`: what the stored file lists take. */
export interface PlanStorage {
  bytes: number;
  plans: number;
}

/** `MissedRun`: a backup whose schedule was due while Diskman was not running. */
export interface MissedRun {
  backup_id: Id;
  due_since: string;
  missed: number;
  last_run: string | null;
}

// ---- System (src-tauri/src/commands/startup.rs, admin.rs) ----

/** `Autostart` */
export interface Autostart {
  enabled: boolean;
  asked: boolean;
}

/** `PlatformInfo` */
export interface PlatformInfo {
  os: string;
  elevated: boolean;
  can_elevate: boolean;
}
