mod commands;
mod tray;

use std::sync::Arc;
use std::sync::Mutex;

use tauri::Manager;
use tracker_core::db::{open_db, Db};

pub struct AppState {
    pub db: Mutex<Db>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let db = open_db().expect("failed to open tracker db");
            app.manage(Arc::new(AppState {
                db: Mutex::new(db),
            }));
            tray::install(&app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_projects,
            commands::get_project,
            commands::update_project,
            commands::run_sync,
            commands::run_discover,
            commands::get_hook_status,
            commands::install_hooks,
            commands::uninstall_hooks,
            commands::recent_active,
            commands::open_in_finder,
            commands::open_url,
            commands::list_terminals,
            commands::get_preferred_terminal,
            commands::set_preferred_terminal,
            commands::start_claude,
            commands::check_release_notes,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
