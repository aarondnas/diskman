import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Started by Tauri (see src-tauri/tauri.conf.json)
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  // TAURI_ENV_PLATFORM (set by the Tauri CLI) tells the UI which system it is built for (src/platform)
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  // WebView2 (Windows) and WebKitGTK 2.40+ (Linux) both handle ES2022
  build: { target: 'es2022' },
});
