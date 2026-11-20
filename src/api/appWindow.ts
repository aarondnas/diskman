// The app window itself and facts about the Tauri runtime. The window buttons are only used by the
// title bar the UI draws on Windows (src/platform/windows/TitleBar.svelte); they are allowed in
// src-tauri/capabilities/windows.json.
import { getTauriVersion } from '@tauri-apps/api/app';
import type { EventCallback, UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow, type PhysicalSize } from '@tauri-apps/api/window';

const win = getCurrentWindow();

export const appWindow = {
  minimize: () => win.minimize(),
  toggleMaximize: () => win.toggleMaximize(),
  /** Only hides the window; Diskman keeps running in the tray (src-tauri/src/platform/tray.rs). */
  close: () => win.close(),
  isMaximized: () => win.isMaximized(),
  /** Calls `fn` whenever the window size changes; resolves to the function that stops it. */
  onResized: (fn: EventCallback<PhysicalSize>): Promise<UnlistenFn> => win.onResized(fn),
  tauriVersion: getTauriVersion,
};
