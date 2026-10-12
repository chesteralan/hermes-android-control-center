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

#[tauri::command]
pub async fn list_device_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<crate::config::DeviceProfile>, AppError> {
    let mut profiles: Vec<_> = state.config.read().await.devices.values().cloned().collect();
    profiles.sort_by(|a, b| a.device_id.cmp(&b.device_id));
    Ok(profiles)
}

#[tauri::command]
pub async fn get_device_profile(
    state: State<'_, AppState>,
    device_id: String,
) -> Result<crate::config::DeviceProfile, AppError> {
    state
        .config
        .read()
        .await
        .devices
        .get(&device_id)
        .cloned()
        .ok_or_else(|| AppError::DeviceNotFound { serial: device_id })
}

#[tauri::command]
pub async fn set_device_profile<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    device_id: String,
    update: crate::config::DeviceProfileUpdate,
) -> Result<crate::config::DeviceProfile, AppError> {
    let mut config = state.config.read().await.clone();
    let profile = config
        .devices
        .get_mut(&device_id)
        .ok_or_else(|| AppError::DeviceNotFound {
            serial: device_id.clone(),
        })?;
    profile.apply_update(update);
    config.validate()?;
    config_store::save(&app, &config)?;
    tracing::info!(device_id = %device_id, "updated device profile");
    let updated_profile = config.devices.get(&device_id).cloned().unwrap();
    *state.config.write().await = config;
    Ok(updated_profile)
}

#[tauri::command]
pub async fn delete_device_profile<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    device_id: String,
) -> Result<(), AppError> {
    let mut config = state.config.read().await.clone();
    if config.devices.remove(&device_id).is_none() {
        return Err(AppError::DeviceNotFound { serial: device_id });
    }
    config_store::save(&app, &config)?;
    tracing::info!(device_id = %device_id, "deleted device profile");
    *state.config.write().await = config;
    Ok(())
}

#[tauri::command]
pub async fn get_effective_device_config(
    state: State<'_, AppState>,
    device_id: String,
) -> Result<crate::config::EffectiveDeviceConfig, AppError> {
    Ok(state.config.read().await.effective_config(&device_id))
}
