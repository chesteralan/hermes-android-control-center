//! Persistence of `AppConfig` via tauri-plugin-store (ADR-008).

use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

use crate::config::AppConfig;
use crate::error::{AppError, AppResult};

const FILE: &str = "settings.json";
const KEY: &str = "config";

pub fn load<R: Runtime>(app: &AppHandle<R>) -> AppConfig {
    match app.store(FILE) {
        Ok(store) => store.get(KEY).map(AppConfig::from_json).unwrap_or_default(),
        Err(e) => {
            tracing::error!(error = %e, "could not open settings store; using defaults");
            AppConfig::default()
        }
    }
}

pub fn save<R: Runtime>(app: &AppHandle<R>, cfg: &AppConfig) -> AppResult<()> {
    let store = app.store(FILE).map_err(|e| AppError::Io(e.to_string()))?;
    let value = serde_json::to_value(cfg).map_err(|e| AppError::Io(e.to_string()))?;
    store.set(KEY, value);
    store.save().map_err(|e| AppError::Io(e.to_string()))
}
