// The operating system the UI runs on: 'windows' or 'linux'. Everything that differs between them
// asks here; code only for one system lives in src/platform/<os>/ (see docs/platforms.md).
//
// Known at build time: the Tauri CLI sets TAURI_ENV_PLATFORM while it builds the frontend for a
// system (vite.config.ts passes it through). A plain `vite` build in a browser has no Tauri CLI
// around it and falls back to the user agent.

export type Os = 'windows' | 'linux';

const fromBuild = import.meta.env.TAURI_ENV_PLATFORM;

export const OS: Os =
  fromBuild === 'windows' || fromBuild === 'linux' ? fromBuild : /Windows/i.test(navigator.userAgent) ? 'windows' : 'linux';

export const IS_WINDOWS = OS === 'windows';
export const IS_LINUX = OS === 'linux';
