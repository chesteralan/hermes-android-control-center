pub mod adb;
pub mod commands;
pub mod config;
pub mod config_store;
pub mod devices;
pub mod error;
pub mod hermes;
pub mod logging;
pub mod logs;
pub mod monitor;
pub mod platform;
pub mod process;
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
            monitor::start(handle);
            app.manage(commands::hermes::ActionLocks::default());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                window.state::<AppState>().shutdown.cancel();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::chat::start_hermes_chat,
            commands::adb::detect_adb,
            commands::settings::get_settings,
            commands::settings::update_settings,
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
            commands::logs::start_log_stream,
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
