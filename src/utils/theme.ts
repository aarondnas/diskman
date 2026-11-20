// Light or dark theme: Diskman is dark by default, whatever the system's mode; light when chosen in
// the settings (config `settings.light_mode`). The light colors apply under
// :root[data-theme='light'] (styles/global.css); the backend switches the window frame along when
// the config is saved (src-tauri/src/platform/window.rs).
// The local copy only bridges the start until the config is loaded, so the window does not flash
// in the other theme.
import { loadPreference, savePreference } from './storage.ts';

const STORAGE_KEY = 'lightMode';

/** The theme of the last session, until the config is loaded. */
export const storedLightMode = () => loadPreference(STORAGE_KEY, false, (v): v is boolean => typeof v === 'boolean');

/** Shows the light (true) or dark (false) theme. */
export function applyTheme(light: boolean | undefined) {
  if (light) document.documentElement.dataset.theme = 'light';
  else delete document.documentElement.dataset.theme;
  savePreference(STORAGE_KEY, !!light);
}
