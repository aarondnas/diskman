/// <reference types="svelte" />
/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Set by the Tauri CLI while it builds the frontend for a system (vite.config.ts passes it through). */
  readonly TAURI_ENV_PLATFORM?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
