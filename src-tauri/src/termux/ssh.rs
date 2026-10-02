//! `TermuxSshTransport`: runs commands as the Termux user over SSH through an adb-forwarded port.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use russh::client::{self, Handle};
use russh::keys::{HashAlg, PrivateKey, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{ChannelMsg, Sig};
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use super::known_hosts::{KnownHosts, Trust};
use crate::error::{AppError, AppResult};
use crate::transport::lines::LineSplitter;
use crate::transport::{CommandResult, DeviceTransport, StreamEvent, TransportKind};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub struct HostKeyCheck {
    expected: Option<String>,
    seen: Arc<StdMutex<Option<String>>>,
}

impl client::Handler for HostKeyCheck {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let fp = match key {
            PublicKeyOrCertificate::PublicKey { key, .. } => {
                key.fingerprint(HashAlg::Sha256).to_string()
            }
            PublicKeyOrCertificate::Certificate(c) => {
                c.public_key().fingerprint(HashAlg::Sha256).to_string()
            }
        };
        *self.seen.lock().unwrap() = Some(fp.clone());
        Ok(self.expected.as_ref().is_none_or(|e| *e == fp))
    }
}

pub type SshHandle = Arc<Handle<HostKeyCheck>>;

fn unavailable(reason: impl Into<String>) -> AppError {
    AppError::TermuxUnavailable {
        reason: reason.into(),
    }
}

/// Connects and authenticates; pins the host key on first use.
pub async fn connect(
    port: u16,
    user: &str,
    key: &PrivateKey,
    known_hosts: &KnownHosts,
    device_id: &str,
) -> AppResult<SshHandle> {
    let seen = Arc::new(StdMutex::new(None));
    let handler = HostKeyCheck {
        expected: known_hosts.expected(device_id),
        seen: seen.clone(),
    };
    let config = Arc::new(client::Config {
        inactivity_timeout: None,
        keepalive_interval: Some(Duration::from_secs(15)),
        ..Default::default()
    });
    let connecting = client::connect(config, ("127.0.0.1", port), handler);
    let mut handle = match tokio::time::timeout(CONNECT_TIMEOUT, connecting).await {
        Err(_) => {
            return Err(AppError::Timeout {
                operation: "Connecting to Termux sshd".into(),
                after_ms: CONNECT_TIMEOUT.as_millis() as u64,
            })
        }
        Ok(Err(e)) => {
            let seen_fp = seen.lock().unwrap().clone();
            if let (Some(fp), Some(expected)) = (seen_fp, known_hosts.expected(device_id)) {
                if fp != expected {
                    return Err(unavailable(format!(
                        "Termux SSH host key changed (expected {expected}, got {fp}). If you reinstalled Termux, forget the host key in the Termux setup and verify again."
                    )));
                }
            }
            tracing::debug!(error = %e, "ssh connect failed");
            return Err(unavailable(
                "Could not reach sshd in Termux. Open Termux and run `sshd` (it must listen on 127.0.0.1:8022).",
            ));
        }
        Ok(Ok(h)) => h,
    };
    let auth = handle
        .authenticate_publickey(
            user,
            PrivateKeyWithHashAlg::new(Arc::new(key.clone()), None),
        )
        .await
        .map_err(|e| unavailable(format!("SSH authentication error: {e}")))?;
    if !auth.success() {
        return Err(unavailable(
            "Termux rejected the app's SSH key. Add the public key from Termux setup to ~/.ssh/authorized_keys.",
        ));
    }
    if let Some(fp) = seen.lock().unwrap().clone() {
        if known_hosts.check(device_id, &fp) == Trust::New {
            known_hosts.pin(device_id, &fp)?;
        }
    }
    Ok(Arc::new(handle))
}

/// One authenticated SSH session per device, reused across commands.
#[derive(Default)]
pub struct SshPool {
    sessions: Mutex<HashMap<String, SshHandle>>,
}

impl SshPool {
    pub async fn get_or_connect<F, Fut>(&self, serial: &str, connect: F) -> AppResult<SshHandle>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = AppResult<SshHandle>>,
    {
        let mut map = self.sessions.lock().await;
        if let Some(h) = map.get(serial) {
            if !h.is_closed() {
                return Ok(h.clone());
            }
        }
        let h = connect().await?;
        map.insert(serial.to_string(), h.clone());
        Ok(h)
    }

    pub async fn drop_device(&self, serial: &str) {
        self.sessions.lock().await.remove(serial);
    }
}

pub struct TermuxSshTransport {
    handle: SshHandle,
}

impl TermuxSshTransport {
    pub fn new(handle: SshHandle) -> Self {
        Self { handle }
    }
}

fn ssh_err(e: russh::Error) -> AppError {
    unavailable(format!("SSH session error: {e}"))
}

#[async_trait]
impl DeviceTransport for TermuxSshTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::TermuxSsh
    }

    async fn execute(&self, command: &str, timeout: Duration) -> AppResult<CommandResult> {
        if command.trim().is_empty() {
            return Err(AppError::Config("Enter a command to run.".into()));
        }
        let started = Instant::now();
        let mut channel = self.handle.channel_open_session().await.map_err(ssh_err)?;
        channel.exec(true, command).await.map_err(ssh_err)?;
        let (mut stdout, mut stderr, mut code) = (Vec::new(), Vec::new(), None);
        let collect = async {
            while let Some(msg) = channel.wait().await {
                match msg {
                    ChannelMsg::Data { data } => stdout.extend_from_slice(&data),
                    ChannelMsg::ExtendedData { data, ext: 1 } => stderr.extend_from_slice(&data),
                    ChannelMsg::ExitStatus { exit_status } => code = Some(exit_status as i32),
                    _ => {}
                }
            }
        };
        if tokio::time::timeout(timeout, collect).await.is_err() {
            return Err(AppError::Timeout {
                operation: "Termux command".into(),
                after_ms: timeout.as_millis() as u64,
            });
        }
        Ok(CommandResult {
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
            exit_code: code,
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }

    async fn stream(
        &self,
        command: &str,
        cancel: CancellationToken,
    ) -> AppResult<mpsc::Receiver<StreamEvent>> {
        if command.trim().is_empty() {
            return Err(AppError::Config("Enter a command to run.".into()));
        }
        let started = Instant::now();
        let mut channel = self.handle.channel_open_session().await.map_err(ssh_err)?;
        channel.exec(true, command).await.map_err(ssh_err)?;
        let (tx, rx) = mpsc::channel(512);
        tokio::spawn(async move {
            let (mut out, mut err) = (LineSplitter::default(), LineSplitter::default());
            let mut code = None;
            loop {
                let msg = tokio::select! {
                    _ = cancel.cancelled() => {
                        let _ = channel.signal(Sig::TERM).await;
                        let _ = channel.close().await;
                        break;
                    }
                    m = channel.wait() => m,
                };
                let events: Vec<StreamEvent> = match msg {
                    None => break,
                    Some(ChannelMsg::Data { data }) => out
                        .push(&data)
                        .into_iter()
                        .map(|line| StreamEvent::Stdout { line })
                        .collect(),
                    Some(ChannelMsg::ExtendedData { data, ext: 1 }) => err
                        .push(&data)
                        .into_iter()
                        .map(|line| StreamEvent::Stderr { line })
                        .collect(),
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
                        code = Some(exit_status as i32);
                        vec![]
                    }
                    Some(_) => vec![],
                };
                for e in events {
                    if tx.send(e).await.is_err() {
                        return;
                    }
                }
            }
            if let Some(line) = out.finish() {
                let _ = tx.send(StreamEvent::Stdout { line }).await;
            }
            if let Some(line) = err.finish() {
                let _ = tx.send(StreamEvent::Stderr { line }).await;
            }
            let _ = tx
                .send(StreamEvent::Exit {
                    code,
                    duration_ms: started.elapsed().as_millis() as u64,
                })
                .await;
        });
        Ok(rx)
    }
}
