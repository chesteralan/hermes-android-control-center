use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::adb::{locate, AdbClient, AdbInfo};
use crate::config::{AppConfig, LogLevelSetting};
use crate::devices::DeviceRegistry;
use crate::error::AppResult;
use crate::platform::RealEnv;
use crate::process::ProcessRunner;
use crate::streams::StreamRegistry;

pub type LogLevelSetter = Box<dyn Fn(LogLevelSetting) + Send + Sync>;

pub struct AppState {
    pub runner: Arc<dyn ProcessRunner>,
    pub config: RwLock<AppConfig>,
    adb: RwLock<Option<AdbInfo>>,
    pub devices: Arc<DeviceRegistry>,
    pub reconnects: Mutex<HashMap<String, CancellationToken>>,
    pub qr_sessions: Mutex<HashMap<String, CancellationToken>>,
    pub streams: Arc<StreamRegistry>,
    pub shutdown: CancellationToken,
    pub set_log_level: LogLevelSetter,
}

impl AppState {
    pub fn new(
        runner: Arc<dyn ProcessRunner>,
        config: AppConfig,
        set_log_level: LogLevelSetter,
    ) -> Self {
        let shutdown = CancellationToken::new();
        Self {
            runner,
            config: RwLock::new(config),
            adb: RwLock::new(None),
            devices: Arc::new(DeviceRegistry::new()),
            reconnects: Mutex::default(),
            qr_sessions: Mutex::default(),
            streams: Arc::new(StreamRegistry::new(shutdown.clone())),
            shutdown,
            set_log_level,
        }
    }

    pub async fn detect_adb(&self) -> AppResult<AdbInfo> {
        let configured = self.config.read().await.adb_path.clone();
        let result = locate::detect(configured.as_ref(), self.runner.clone(), &RealEnv, |p| {
            p.is_file()
        })
        .await;
        *self.adb.write().await = result.as_ref().ok().cloned();
        result
    }

    pub async fn adb_client(&self) -> AppResult<AdbClient> {
        if let Some(info) = self.adb.read().await.clone() {
            return Ok(AdbClient::new(info.path, self.runner.clone()));
        }
        let info = self.detect_adb().await?;
        Ok(AdbClient::new(info.path, self.runner.clone()))
    }

    pub async fn invalidate_adb(&self) {
        *self.adb.write().await = None;
    }

    pub fn cancel_reconnect(&self, serial: &str) {
        if let Some(t) = self.reconnects.lock().unwrap().remove(serial) {
            t.cancel();
        }
    }
}
