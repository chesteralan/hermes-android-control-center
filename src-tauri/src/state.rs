use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use russh::keys::PrivateKey;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::adb::parse::device_id_from_serial;
use crate::adb::{locate, AdbClient, AdbInfo};
use crate::config::{AppConfig, LogLevelSetting};
use crate::devices::DeviceRegistry;
use crate::error::AppResult;
use crate::platform::RealEnv;
use crate::process::ProcessRunner;
use crate::streams::StreamRegistry;
use crate::termux::forward::ForwardManager;
use crate::termux::keys;
use crate::termux::known_hosts::KnownHosts;
use crate::termux::ssh::{self, SshPool, TermuxSshTransport};

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
    pub data_dir: PathBuf,
    ssh_key: OnceLock<PrivateKey>,
    ssh_key_init: Mutex<()>,
    pub known_hosts: KnownHosts,
    pub forwards: ForwardManager,
    pub ssh: SshPool,
}

impl AppState {
    pub fn new(
        runner: Arc<dyn ProcessRunner>,
        config: AppConfig,
        set_log_level: LogLevelSetter,
        data_dir: PathBuf,
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
            known_hosts: KnownHosts::load(&data_dir),
            data_dir,
            ssh_key: OnceLock::new(),
            ssh_key_init: Mutex::new(()),
            forwards: ForwardManager::default(),
            ssh: SshPool::default(),
        }
    }

    pub fn ssh_key(&self) -> AppResult<&PrivateKey> {
        if let Some(k) = self.ssh_key.get() {
            return Ok(k);
        }
        // Serialize first use: concurrent callers must not each generate a different key.
        let _guard = self.ssh_key_init.lock().unwrap();
        if let Some(k) = self.ssh_key.get() {
            return Ok(k);
        }
        let k = keys::load_or_create(&self.data_dir)?;
        Ok(self.ssh_key.get_or_init(|| k))
    }

    /// Stable identity for pinning; falls back to the serial when unknown.
    pub fn device_id_for(&self, serial: &str) -> String {
        self.devices
            .get(serial)
            .and_then(|d| d.device_id)
            .or_else(|| device_id_from_serial(serial))
            .unwrap_or_else(|| serial.to_string())
    }

    pub async fn termux_transport(&self, serial: &str) -> AppResult<TermuxSshTransport> {
        let client = self.adb_client().await?;
        let cfg = self.config.read().await.termux.clone();
        let handle = self
            .ssh
            .get_or_connect(serial, || async {
                let port = self.forwards.ensure(&client, serial, cfg.ssh_port).await?;
                let key = self.ssh_key()?;
                ssh::connect(
                    port,
                    &cfg.ssh_user,
                    key,
                    &self.known_hosts,
                    &self.device_id_for(serial),
                )
                .await
            })
            .await?;
        Ok(TermuxSshTransport::new(handle))
    }

    /// Drop SSH sessions and port forwards for a device.
    pub async fn release_device(&self, serial: &str) {
        self.ssh.drop_device(serial).await;
        if let Ok(client) = self.adb_client().await {
            self.forwards.remove_device(&client, serial).await;
        }
    }
}

impl AppState {
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
