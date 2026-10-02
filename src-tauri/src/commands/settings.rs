use tauri::{AppHandle, Runtime, State};

use crate::config::AppConfig;
use crate::config_store;
use crate::error::AppError;
use crate::state::AppState;

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
