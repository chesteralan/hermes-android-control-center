use tauri::State;

use crate::adb::AdbInfo;
use crate::error::AppError;
use crate::state::AppState;

/// Re-runs ADB detection (Settings → Detect).
#[tauri::command]
pub async fn detect_adb(state: State<'_, AppState>) -> Result<AdbInfo, AppError> {
    state.invalidate_adb().await;
    state.detect_adb().await
}

#[tauri::command]
pub async fn restart_adb_server(state: State<'_, AppState>) -> Result<(), AppError> {
    state.adb_client().await?.restart_server().await
}
