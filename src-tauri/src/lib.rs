mod cert_watch;
mod checkhost;
mod commands;
mod config;
mod dns_probe;
mod docker_monitor;
mod error;
mod network_checker;
mod notification_engine;
mod scheduler;
mod secrets;
mod ssh_manager;
mod state;
mod status;
mod tray;

use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

use crate::state::AppState;

pub(crate) fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

pub(crate) fn install_tls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

pub fn run() {
    install_tls_provider();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let directory = app.path().app_config_dir()?;
            app.manage(AppState::new(directory.join("targets.json"))?);
            tray::install(app)?;
            scheduler::spawn(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_targets,
            commands::list_statuses,
            commands::save_target,
            commands::delete_target,
            commands::set_target_enabled,
            commands::check_now,
            commands::get_settings,
            commands::save_settings,
            commands::send_test_notification,
        ])
        .run(tauri::generate_context!())
        .expect("IranNANternet failed to start");
}
