use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use russh::keys::PrivateKey;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::adb::parse::device_id_from_serial;
use crate::adb::{locate, AdbClient, AdbInfo};
use crate::config::{AppConfig, HermesTransportKind, LogLevelSetting};
use crate::devices::DeviceRegistry;
use crate::error::{AppError, AppResult};
use crate::platform::RealEnv;
use crate::process::ProcessRunner;
use crate::streams::StreamRegistry;
use crate::termux::forward::ForwardManager;
use crate::termux::keys;
use crate::termux::known_hosts::KnownHosts;
use crate::termux::ssh::{self, PtyInput, PtySessionControl, SshPool, TermuxSshTransport};
use crate::transport::api::ApiTransport;
use crate::transport::DeviceTransport;

struct PtySessionEntry {
    serial: String,
    control: PtySessionControl,
}

#[derive(Default)]
pub struct PtySessionRegistry {
    next_id: AtomicU64,
    sessions: Mutex<HashMap<String, PtySessionEntry>>,
}

impl PtySessionRegistry {
    pub fn insert(&self, serial: &str, control: PtySessionControl) -> String {
        let id = format!("pty-{}", self.next_id.fetch_add(1, Ordering::Relaxed) + 1);
        self.sessions.lock().unwrap().insert(
            id.clone(),
            PtySessionEntry {
                serial: serial.to_string(),
                control,
            },
        );
        id
    }

    pub fn control(&self, id: &str) -> Option<PtySessionControl> {
        self.sessions
            .lock()
            .unwrap()
            .get(id)
            .map(|entry| entry.control.clone())
    }

    pub async fn close(&self, id: &str) -> bool {
        let entry = self.sessions.lock().unwrap().remove(id);
        if let Some(entry) = entry {
            let _ = entry.control.send(PtyInput::Close).await;
            true
        } else {
            false
        }
    }

    pub async fn close_device(&self, serial: &str) {
        let ids = self
            .sessions
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, entry)| entry.serial == serial)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            self.close(&id).await;
        }
    }

    pub async fn close_all(&self) {
        let ids = self
            .sessions
            .lock()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in ids {
            self.close(&id).await;
        }
    }
}

pub type LogLevelSetter = Box<dyn Fn(LogLevelSetting) + Send + Sync>;

const API_TOKEN_SERVICE: &str = "com.hermes-control-center.control-api";

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
    pub pty_sessions: Arc<PtySessionRegistry>,
    pub provisioning_runs: Mutex<HashSet<String>>,
    pub encrypted_secrets: crate::secrets::EncryptedSecrets,
}

impl AppState {
    fn lock_reconnects(&self) -> MutexGuard<'_, HashMap<String, CancellationToken>> {
        self.reconnects.lock().unwrap_or_else(|poisoned| {
            tracing::error!("reconnect registry lock was poisoned; cancelling active retries");
            let mut reconnects = poisoned.into_inner();
            for token in reconnects.values() {
                token.cancel();
            }
            reconnects.clear();
            self.reconnects.clear_poison();
            reconnects
        })
    }

    pub fn track_reconnect(
        &self,
        serial: String,
        token: CancellationToken,
    ) -> Option<CancellationToken> {
        self.lock_reconnects().insert(serial, token)
    }

    pub fn remove_reconnect(&self, serial: &str) -> Option<CancellationToken> {
        self.lock_reconnects().remove(serial)
    }

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
            encrypted_secrets: crate::secrets::EncryptedSecrets::new(&data_dir),
            data_dir,
            ssh_key: OnceLock::new(),
            ssh_key_init: Mutex::new(()),
            forwards: ForwardManager::default(),
            ssh: SshPool::default(),
            pty_sessions: Arc::new(PtySessionRegistry::default()),
            provisioning_runs: Mutex::default(),
        }
    }

    pub fn ssh_key(&self) -> AppResult<&PrivateKey> {
        if let Some(k) = self.ssh_key.get() {
            return Ok(k);
        }
        // Serialize first use: concurrent callers must not each generate a different key.
        let _guard = self.ssh_key_init.lock().map_err(|_| {
            AppError::Io(
                "SSH key initialization is unavailable. Restart the application before retrying."
                    .into(),
            )
        })?;
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

    pub fn api_token(&self, serial: &str) -> AppResult<Option<String>> {
        let device_id = self.device_id_for(serial);
        if self.encrypted_secrets.status() != "native" {
            return self.encrypted_secrets.get(&device_id);
        }
        let entry = keyring::Entry::new(API_TOKEN_SERVICE, &device_id)
            .map_err(|error| AppError::Io(error.to_string()))?;
        match entry.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(AppError::Io(format!(
                "System credential store unavailable: {error}. Unlock your desktop keyring or explicitly enable encrypted file storage in Settings. No plaintext fallback is used."
            ))),
        }
    }

    pub fn set_api_token(&self, serial: &str, token: &str) -> AppResult<()> {
        let device_id = self.device_id_for(serial);
        if self.encrypted_secrets.status() != "native" {
            return self.encrypted_secrets.set(&device_id, token);
        }
        let entry = keyring::Entry::new(API_TOKEN_SERVICE, &device_id)
            .map_err(|error| AppError::Io(error.to_string()))?;
        entry.set_password(token).map_err(|error| {
            AppError::Io(format!("System credential store unavailable: {error}. Unlock your desktop keyring or explicitly enable encrypted file storage in Settings. No plaintext fallback is used."))
        })?;
        Ok(())
    }

    pub async fn api_transport(&self, serial: &str) -> AppResult<ApiTransport> {
        let token = self.api_token(serial)?.ok_or_else(|| {
            AppError::Config("No Control API token is saved for this device.".into())
        })?;
        let client = self.adb_client().await?;
        let port = self.config.read().await.api_port;
        let local_port = self.forwards.ensure(&client, serial, port).await?;
        Ok(ApiTransport::new(local_port, token))
    }

    pub async fn configured_hermes_transport(
        &self,
        serial: &str,
    ) -> AppResult<Arc<dyn DeviceTransport>> {
        let transport = self.config.read().await.hermes.transport;
        match transport {
            HermesTransportKind::TermuxSsh => Ok(Arc::new(self.termux_transport(serial).await?)),
            HermesTransportKind::ControlApi => Ok(Arc::new(self.api_transport(serial).await?)),
        }
    }

    /// Drop SSH sessions and port forwards for a device.
    pub async fn release_device(&self, serial: &str) {
        self.pty_sessions.close_device(serial).await;
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
        if let Some(t) = self.remove_reconnect(serial) {
            t.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::FakeRunner;

    #[test]
    fn poisoned_ssh_initialization_returns_error_without_creating_a_key() {
        let directory = std::env::temp_dir().join(format!(
            "hacc-ssh-init-failure-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        let state = Arc::new(AppState::new(
            Arc::new(FakeRunner::new()),
            AppConfig::default(),
            Box::new(|_| {}),
            directory.clone(),
        ));
        let shared = state.clone();
        let worker = std::thread::spawn(move || {
            let _guard = shared.ssh_key_init.lock().unwrap();
            panic!("synthetic initialization failure");
        });
        assert!(worker.join().is_err());
        assert!(matches!(state.ssh_key(), Err(AppError::Io(message))
            if message.contains("Restart the application")));
        assert!(!keys::key_path(&directory).exists());
    }

    #[test]
    fn poisoned_reconnect_registry_cancels_retries_and_recovers() {
        let directory =
            std::env::temp_dir().join(format!("hacc-reconnect-lock-{}", std::process::id()));
        let state = Arc::new(AppState::new(
            Arc::new(FakeRunner::new()),
            AppConfig::default(),
            Box::new(|_| {}),
            directory,
        ));
        let token = CancellationToken::new();
        state.track_reconnect("serial".into(), token.clone());
        let poison = state.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poison.reconnects.lock().unwrap();
            panic!("poison reconnect registry");
        })
        .join();

        state.cancel_reconnect("serial");
        assert!(token.is_cancelled());
        assert!(state
            .track_reconnect("serial".into(), CancellationToken::new())
            .is_none());
    }
}
