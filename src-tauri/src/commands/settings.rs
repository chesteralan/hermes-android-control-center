use tauri::{AppHandle, Manager, Runtime, State};

use crate::config::AppConfig;
use crate::config_store;
use crate::error::AppError;
use crate::state::AppState;

#[tauri::command]
pub fn get_secret_storage_state(state: State<'_, AppState>) -> String {
    state.encrypted_secrets.status().into()
}

#[tauri::command]
pub async fn unlock_secret_storage<R: Runtime>(
    app: AppHandle<R>,
    passphrase: String,
    opted_in: bool,
) -> Result<(), AppError> {
    let passphrase = zeroize::Zeroizing::new(passphrase);
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>()
            .encrypted_secrets
            .unlock(&passphrase, opted_in)
    })
    .await
    .map_err(|_| AppError::Io("Secret storage unlock failed.".into()))?
}

#[tauri::command]
pub fn lock_secret_storage(state: State<'_, AppState>) {
    state.encrypted_secrets.lock();
}

#[tauri::command]
pub fn supports_in_app_updates() -> bool {
    crate::platform::supports_in_app_updates(
        crate::platform::current_os(),
        std::env::var_os("APPIMAGE").is_some(),
    )
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppConfig, AppError> {
    Ok(state.config.read().await.clone())
}

#[tauri::command]
pub async fn update_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    config: AppConfig,
) -> Result<AppConfig, AppError> {
    config.validate()?;
    let adb_changed = state.config.read().await.adb_path != config.adb_path;
    config_store::save(&app, &config)?;
    (state.set_log_level)(config.log_level);
    *state.config.write().await = config.clone();
    if adb_changed {
        state.invalidate_adb().await;
    }
    Ok(config)
}
