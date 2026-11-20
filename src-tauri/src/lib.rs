//! Diskman backend. Layers, from the UI inwards:
//!
//! - `commands`  IPC: the Tauri commands the UI calls
//! - `jobs`      running backups: queue, schedules, the run of one backup
//! - `sync`      the sync engine: scan, plan, execute
//! - `model`     the configuration as data and its rules
//! - `storage`   data folder, config.json and the SQLite history
//! - `platform`  everything OS-specific (Windows, Linux): window, tray, rights, paths, file systems
//! - `updates`   new versions of Diskman itself, from the releases on GitHub
//!
//! `state::AppState` holds what they share while the app runs.

mod commands;
mod jobs;
mod model;
mod platform;
mod state;
mod storage;
mod sync;
mod updates;

use std::sync::Arc;

use tauri::{Manager, WindowEvent};

use crate::platform::autostart::BACKGROUND_ARG;
use crate::platform::{elevation, tray, window};

pub fn run() {
    // Restarted with administrator rights: let the old instance quit first (see platform/elevation.rs)
    elevation::wait_for_replaced_instance();
    // Started at login (platform/autostart.rs): no window, missed runs are caught up right away
    let background = std::env::args().any(|a| a == BACKGROUND_ARG);
    let state = Arc::new(state::AppState::load().expect("Diskman data folder not available"));
    let shared = state.clone();
    tauri::Builder::default()
        // Must be the first plugin: starting Diskman a second time only brings the running window
        // to the front instead of starting a second instance with its own schedules. A second
        // start at login (e.g. after a sign-out without quitting) changes nothing.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if !args.iter().any(|a| a == BACKGROUND_ARG) {
                tray::show_main_window(app);
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state)
        .setup(move |app| {
            app.manage(updates::Updates::new(app.package_info().version.to_string()));
            if !background {
                window::show_main(app.handle());
            }
            tray::create(app.handle())?;
            // Only here, after the single-instance check: a second start must not touch the runs
            // of the instance that is running
            {
                let config = shared.config.lock().unwrap().clone();
                if let Err(e) = shared.history.close_interrupted(&config) {
                    eprintln!("Interrupted runs not closed: {e:#}");
                }
            }
            // Shutdown or sign-out regardless of a running sync: stop it in the time left
            let on_end = shared.clone();
            platform::power::on_session_end(move || {
                jobs::shutdown::stop_all(&on_end, jobs::shutdown::SESSION_END_WAIT);
            });
            jobs::queue::spawn(shared.clone());
            // Before the scheduler writes its log again: what was missed since it last ran
            let missed = jobs::catch_up::find(&shared);
            if background {
                // Nobody to ask: run them and say so in a notification
                if let Some((title, body)) = jobs::catch_up::run_all(&shared, &missed) {
                    platform::notification::show(app.handle(), &title, &body);
                }
            } else {
                *shared.missed.lock().unwrap() = missed;
            }
            jobs::schedules::spawn(shared);
            updates::spawn(app.handle().clone());
            Ok(())
        })
        .on_window_event(|win, event| match event {
            // Closing only hides the window; Diskman keeps running in the tray (see platform/tray.rs)
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = win.hide();
            }
            // The app keeps its own theme (dark or light from the settings, not the system's mode);
            // set the frame colors again in case Windows reset them
            WindowEvent::ThemeChanged(_) => window::apply_frame(win, window::light_mode(win.app_handle())),
            // New scaling: load the icon in the matching size
            WindowEvent::ScaleFactorChanged { .. } => window::apply_icon(win),
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::config::get_config,
            commands::config::save_config,
            commands::config::get_data_dir,
            commands::sync::get_status,
            commands::sync::get_run_problems,
            commands::sync::preview,
            commands::sync::run_backup,
            commands::sync::get_job,
            commands::sync::cancel_job,
            commands::sync::cancel_backups,
            commands::sync::cancel_checks,
            commands::sync::end_check_batch,
            commands::sync::probe_plans,
            commands::sync::check_progress,
            commands::sync::plan_tree,
            commands::sync::run_tree,
            commands::sync::drop_plans,
            commands::activity::get_activities,
            commands::activity::open_activity,
            commands::activity::close_activity,
            commands::activity::activity_storage,
            commands::activity::clear_activity_plans,
            commands::activity::clear_activities,
            commands::catch_up::get_missed_runs,
            commands::catch_up::run_missed,
            commands::catch_up::dismiss_missed,
            commands::startup::get_autostart,
            commands::startup::set_autostart,
            commands::dialog::pick_folder,
            commands::dialog::reveal_path,
            commands::update::get_update_status,
            commands::update::check_for_update,
            commands::update::install_update,
            commands::update::open_website,
            commands::admin::get_platform,
            commands::admin::restart_as_admin,
        ])
        .run(tauri::generate_context!())
        .expect("Diskman could not be started");
}
