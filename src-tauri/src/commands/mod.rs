//! IPC layer: the Tauri commands the UI calls with `invoke` (src/api/backend.ts). They only pass
//! data between the UI and the other layers; errors become plain text for the UI.
//! The UI has a TypeScript copy of every type sent or received here (src/api/types.ts): a change
//! to such a struct or enum has to be made there as well.

pub mod activity;
pub mod admin;
pub mod catch_up;
pub mod config;
pub mod dialog;
pub mod startup;
pub mod sync;
pub mod update;

use std::sync::Arc;

use crate::state::AppState;

type Shared<'a> = tauri::State<'a, Arc<AppState>>;
