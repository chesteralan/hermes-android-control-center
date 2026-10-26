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
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
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
            // Unix: SIGTERM/SIGINT must take the graceful quit path so streamed
            // `adb` children (track-devices, logcat, PTYs) are reaped (M10-T3).
            #[cfg(unix)]
            {
                let quit_handle = handle.clone();
                tauri::async_runtime::spawn(async move {
                    use tokio::signal::unix::{signal, SignalKind};
                    let mut term = match signal(SignalKind::terminate()) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::warn!(error = %e, "SIGTERM handler unavailable");
                            return;
                        }
                    };
                    let mut interrupt = match signal(SignalKind::interrupt()) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::warn!(error = %e, "SIGINT handler unavailable");
                            return;
                        }
                    };
                    tokio::select! {
                        _ = term.recv() => tracing::info!("SIGTERM received; quitting gracefully"),
                        _ = interrupt.recv() => tracing::info!("SIGINT received; quitting gracefully"),
                    }
                    lifecycle::request_quit(quit_handle);
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            let lifecycle = window.state::<lifecycle::AppLifecycle>();
            match event {
                tauri::WindowEvent::CloseRequested { api, .. }
                    if lifecycle.should_hide_on_close() =>
                {
                    api.prevent_close();
                    // Best-effort: nothing further to do if the OS refuses to hide.
                    let _ = window.hide();
                }
                tauri::WindowEvent::Focused(false) | tauri::WindowEvent::Resized(_)
                    if lifecycle
                        .should_hide_on_minimize(window.is_minimized().unwrap_or(false)) =>
                {
                    // Best-effort: nothing further to do if the OS refuses to hide.
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
            commands::settings::list_device_profiles,
            commands::settings::get_device_profile,
            commands::settings::set_device_profile,
            commands::settings::delete_device_profile,
            commands::settings::get_effective_device_config,
            commands::control_api::preview_control_api_install,
            commands::control_api::install_control_api,
            commands::control_api::test_control_api,
            commands::diagnostics::export_diagnostics,
            commands::provision::list_provision_recipes,
            commands::provision::get_provision_recipe_source,
            commands::provision::save_provision_recipe,
            commands::provision::get_hermes_config,
            commands::provision::save_hermes_config,
            commands::provision::apply_hermes_setup,
            commands::provision::import_provision_recipe,
            commands::provision::export_provision_recipe,
            commands::provision::get_provision_plan,
            commands::provision::uninstall_incompatible_termux,
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
            commands::termux::repair_termux_ssh_bridge,
            commands::hermes::get_hermes_status,
            commands::hermes::detect_hermes,
            commands::hermes::hermes_action,
            commands::hermes::run_hermes_tool,
        ])
        // Tauri's build() returns only on a fatal startup error; there is no
        // recoverable alternative, so a panic is the correct response here.
        .build(tauri::generate_context!())
        .expect("error while building Hermes Control Center");
    app.run(|handle, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            // Every exit route (Cmd+Q, terminate, last window) must pass
            // through the graceful path so streamed `adb` children
            // (track-devices, logcat, PTYs) are reaped before the process
            // dies (M10-T5: ⌘Q used to orphan them). The first request owns
            // the shutdown; further requests stay deferred until cleanup
            // reports finished, then the final exit is allowed through.
            let lifecycle = handle.state::<lifecycle::AppLifecycle>();
            let defer_exit =
                lifecycle::request_quit(handle.clone()) || !lifecycle.exit_cleanup_finished();
            if defer_exit {
                api.prevent_exit();
            }
        }
    });
}

#[cfg(test)]
mod security_review {
    use serde_json::Value;

    fn read_json(path: &str) -> Value {
        let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
        let text = std::fs::read_to_string(&full)
            .unwrap_or_else(|error| panic!("read {}: {error}", full.display()));
        serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("parse {}: {error}", full.display()))
    }

    // M10-T2 capability review (2026-10-14): the reviewed webview grant set.
    // The frontend never invokes `plugin:dialog` — every file dialog opens from
    // Rust (`app.dialog()`), which is not gated by capabilities — so no
    // `dialog:*` permission is granted. Each remaining grant is used by shipped
    // frontend code: core IPC/events/window state, the three notification calls
    // in `nativeNotifications.ts`, `updater:default` for `check`/
    // `downloadAndInstall`, and `process:allow-restart` for `relaunch`.
    #[test]
    fn capability_grants_match_reviewed_set() {
        let capability = read_json("capabilities/default.json");
        let mut permissions = capability["permissions"]
            .as_array()
            .expect("capabilities/default.json must list permissions")
            .iter()
            .map(|value| value.as_str().expect("permission must be a string"))
            .collect::<Vec<_>>();
        permissions.sort_unstable();
        let expected = [
            "core:default",
            "notification:allow-is-permission-granted",
            "notification:allow-notify",
            "notification:allow-request-permission",
            "process:allow-restart",
            "updater:default",
        ];
        assert_eq!(
            permissions, expected,
            "capability set changed; update SECURITY.md §3 and this test together"
        );
        assert_eq!(
            capability["windows"],
            serde_json::json!(["main"]),
            "capabilities must stay scoped to the main window"
        );
    }

    // M10-T2 CSP review (2026-10-14): policy fixed as documented in
    // SECURITY.md §3; no remote script/style/font/image origins and no
    // `unsafe-eval`, matching a frontend with zero fetch/XHR/WebSocket usage.
    #[test]
    fn csp_matches_reviewed_policy() {
        let config = read_json("tauri.conf.json");
        let csp = config["app"]["security"]["csp"]
            .as_str()
            .expect("tauri.conf.json must set app.security.csp");
        assert_eq!(
            csp,
            "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src ipc: http://ipc.localhost"
        );
        assert_eq!(
            config["app"]["withGlobalTauri"],
            serde_json::json!(false),
            "withGlobalTauri must stay disabled"
        );
    }
}
