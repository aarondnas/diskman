// Small UI preferences kept in the browser storage of the window (sort order, settings tab,
// language until the config is loaded). Everything that matters lives in the config instead,
// so a missing or blocked storage only means the default is used.

const PREFIX = 'diskman.';

/** The stored value of `key`, or `fallback` if there is none or `valid(value)` refuses it. */
export function loadPreference<T>(key: string, fallback: T, valid: (value: unknown) => value is T): T {
  let raw: string | null;
  try {
    raw = localStorage.getItem(PREFIX + key);
  } catch {
    return fallback; // No storage
  }
  if (raw == null) return fallback;
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    value = raw; // Plain text written by earlier versions
  }
  return valid(value) ? value : fallback;
}

export function savePreference(key: string, value: unknown) {
  try {
    localStorage.setItem(PREFIX + key, JSON.stringify(value));
  } catch {
    // Without storage the choice only lasts until restart
  }
}
