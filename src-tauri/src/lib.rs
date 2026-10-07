pub mod adb;
pub mod commands;
pub mod config;
pub mod config_store;
pub mod devices;
pub mod error;
pub mod hermes;
pub mod lifecycle;
pub mod logging;
pub mod logs;
pub mod monitor;
pub mod platform;
pub mod process;
pub mod provision;
pub mod secrets;
pub mod state;
pub mod streams;
pub mod termux;
pub mod transport;

use std::sync::Arc;

use tauri::Manager;

use crate::process::TokioRunner;
use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            let cfg = config_store::load(&handle);
            let log_dir = app.path().app_log_dir()?;
            let set_level = logging::init(&log_dir, cfg.log_level);
            tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                "starting Hermes Control Center"
            );
            app.manage(AppState::new(
                Arc::new(TokioRunner),
                cfg,
                set_level,
                app.path().app_data_dir()?,
            ));
            monitor::start(handle.clone());
            app.manage(commands::hermes::ActionLocks::default());
            lifecycle::install_app_menu(&handle)?;
            let tray_available = lifecycle::install_tray(&handle);
            app.manage(lifecycle::AppLifecycle::new(tray_available));
            Ok(())
        })
        .on_window_event(|window, event| {
            let lifecycle = window.state::<lifecycle::AppLifecycle>();
            match event {
                tauri::WindowEvent::CloseRequested { api, .. }
                    if lifecycle.should_hide_on_close() =>
                {
                    api.prevent_close();
                    let _ = window.hide();
                }
                tauri::WindowEvent::Focused(false) | tauri::WindowEvent::Resized(_)
                    if lifecycle
                        .should_hide_on_minimize(window.is_minimized().unwrap_or(false)) =>
                {
                    let _ = window.hide();
                }
                _ => {}
            }
            if let tauri::WindowEvent::Destroyed = event {
                let state = window.state::<AppState>();
                state.shutdown.cancel();
                let pty_sessions = state.pty_sessions.clone();
                tauri::async_runtime::spawn(async move {
                    pty_sessions.close_all().await;
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::chat::start_hermes_chat,
            commands::sessions::list_hermes_sessions,
            commands::sessions::get_hermes_session_messages,
            commands::adb::detect_adb,
            commands::adb::restart_adb_server,
            commands::settings::get_settings,
            commands::settings::get_secret_storage_state,
            commands::settings::unlock_secret_storage,
            commands::settings::lock_secret_storage,
            commands::settings::supports_in_app_updates,
            commands::settings::update_settings,
            commands::control_api::preview_control_api_install,
            commands::control_api::install_control_api,
            commands::control_api::test_control_api,
            commands::diagnostics::export_diagnostics,
            commands::provision::list_provision_recipes,
            commands::provision::get_provision_recipe_source,
            commands::provision::save_provision_recipe,
            commands::provision::import_provision_recipe,
            commands::provision::export_provision_recipe,
            commands::provision::get_provision_plan,
            commands::provision::reset_provision_progress,
            commands::provision::run_provision,
            commands::provision::cancel_provision,
            commands::device::list_devices,
            commands::device::connect_device,
            commands::device::disconnect_device,
            commands::device::retry_connection,
            commands::device::pair_device,
            commands::device::discover_devices,
            commands::device::get_device_info,
            commands::device::start_qr_pairing,
            commands::device::cancel_qr_pairing,
            commands::terminal::execute_command,
            commands::terminal::stream_command,
            commands::terminal::cancel_stream,
            commands::terminal::start_terminal_pty,
            commands::terminal::write_terminal_pty,
            commands::terminal::resize_terminal_pty,
            commands::terminal::close_terminal_pty,
            commands::terminal::export_terminal_text,
            commands::logs::start_log_stream,
            commands::logs::export_logs,
            commands::termux::get_termux_public_key,
            commands::termux::check_termux,
            commands::termux::forget_termux_host_key,
            commands::hermes::get_hermes_status,
            commands::hermes::detect_hermes,
            commands::hermes::hermes_action,
            commands::hermes::run_hermes_tool,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Hermes Control Center");
}
