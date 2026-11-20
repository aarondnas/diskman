// The configuration (backups, settings) as the backend has stored it. Every change goes through
// `updateConfig`: it is applied to a copy, saved, and only taken over if the backend accepts it.
import { backend } from '../api/backend.ts';
import type { Backup, Config, Id } from '../api/types.ts';
import { clearError, showError } from './notice.svelte.ts';

const state = $state<{ current: Config | null }>({ current: null });

export const config = {
  /** The whole config, null until loaded. */
  get current() {
    return state.current;
  },
  get backups(): Backup[] {
    return state.current?.backups ?? [];
  },
};

export async function loadConfig() {
  state.current = await backend.getConfig();
  return state.current;
}

/** Applies `mutate` to a copy of the config and saves it; true if the backend accepted it. */
export async function updateConfig(mutate: (config: Config) => unknown): Promise<boolean> {
  if (!state.current) return false;
  const next = $state.snapshot(state.current);
  mutate(next);
  try {
    state.current = await backend.saveConfig(next);
    clearError();
    return true;
  } catch (e) {
    showError(e);
    return false;
  }
}

/** Like `updateConfig` for one backup: `mutate` receives that backup (nothing happens without it). */
export const updateBackup = (id: Id, mutate: (backup: Backup) => unknown) =>
  updateConfig((c) => {
    const backup = c.backups.find((b) => b.id === id);
    if (backup) mutate(backup);
  });

export const findBackup = (id: Id) => state.current?.backups.find((b) => b.id === id) ?? null;

/**
 * Name of a backup created without one: "Backup-" and the number it gets in the list (count + 1);
 * if that name is taken already, the next free number.
 */
export function defaultBackupName() {
  const backups = state.current?.backups ?? [];
  const taken = new Set(backups.map((b) => b.name.trim().toLowerCase()));
  let n = backups.length + 1;
  while (taken.has(`backup-${n}`)) n++;
  return `Backup-${n}`;
}
