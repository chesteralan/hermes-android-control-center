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
        let Ok(mut seen) = self.seen.lock() else {
            tracing::error!("SSH host-key verification lock was poisoned; rejecting key");
            return Ok(false);
        };
        *seen = Some(fp.clone());
        Ok(self.expected.as_ref().is_none_or(|e| *e == fp))
    }
}

pub type SshHandle = Arc<Handle<HostKeyCheck>>;

fn unavailable(reason: impl Into<String>) -> AppError {
    AppError::TermuxUnavailable {
        reason: reason.into(),
    }
}

fn seen_fingerprint(seen: &StdMutex<Option<String>>) -> AppResult<Option<String>> {
    seen.lock()
        .map(|fingerprint| fingerprint.clone())
        .map_err(|_| AppError::Io("SSH host-key verification state is unavailable.".into()))
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
        expected: known_hosts.expected(device_id)?,
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
            let seen_fp = seen_fingerprint(&seen)?;
            if let (Some(fp), Some(expected)) = (seen_fp, known_hosts.expected(device_id)?) {
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
    if let Some(fp) = seen_fingerprint(&seen)? {
        if known_hosts.check(device_id, &fp)? == Trust::New {
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

#[derive(Debug)]
pub enum PtyInput {
    Data(Vec<u8>),
    Resize { columns: u32, rows: u32 },
    Close,
}

#[derive(Clone)]
pub struct PtySessionControl {
    sender: mpsc::Sender<PtyInput>,
}

impl PtySessionControl {
    pub async fn send(&self, input: PtyInput) -> AppResult<()> {
        self.sender
            .send(input)
            .await
            .map_err(|_| unavailable("The interactive Termux session has closed."))
    }
}

impl TermuxSshTransport {
    pub fn new(handle: SshHandle) -> Self {
        Self { handle }
    }

    pub async fn open_pty(
        &self,
        columns: u32,
        rows: u32,
    ) -> AppResult<(mpsc::Receiver<Vec<u8>>, PtySessionControl)> {
        let mut channel = self.handle.channel_open_session().await.map_err(ssh_err)?;
        channel
            .request_pty(
                true,
                "xterm-256color",
                columns.clamp(1, 500),
                rows.clamp(1, 300),
                0,
                0,
                &[],
            )
            .await
            .map_err(ssh_err)?;
        channel.request_shell(true).await.map_err(ssh_err)?;

        let (output_sender, output_receiver) = mpsc::channel(64);
        let (input_sender, mut input_receiver) = mpsc::channel(64);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    input = input_receiver.recv() => match input {
                        Some(PtyInput::Data(data)) => {
                            if channel.data(&data[..]).await.is_err() { break; }
                        }
                        Some(PtyInput::Resize { columns, rows }) => {
                            if channel
                                .window_change(columns.clamp(1, 500), rows.clamp(1, 300), 0, 0)
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        Some(PtyInput::Close) | None => break,
                    },
                    message = channel.wait() => match message {
                        Some(ChannelMsg::Data { data }) => {
                            if output_sender.send(data.to_vec()).await.is_err() { break; }
                        }
                        Some(ChannelMsg::Eof | ChannelMsg::Close) | None => break,
                        _ => {}
                    },
                }
            }
            let _ = channel.eof().await;
            let _ = channel.close().await;
        });

        Ok((
            output_receiver,
            PtySessionControl {
                sender: input_sender,
            },
        ))
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

#[cfg(test)]
mod tests {
    use super::*;
    use russh::server::{self, Auth, ChannelOpenHandle, Msg, Server as _, Session};
    use russh::{Channel, ChannelId, Pty, Sig};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::net::TcpListener;
    use tokio::sync::{mpsc as tokio_mpsc, oneshot};

    #[test]
    fn poisoned_seen_fingerprint_returns_error() {
        let seen = Arc::new(StdMutex::new(Some("SHA256:known".into())));
        let poison = seen.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poison.lock().unwrap();
            panic!("poison SSH seen fingerprint");
        })
        .join();

        assert!(matches!(
            seen_fingerprint(&seen),
            Err(AppError::Io(message)) if message.contains("host-key verification")
        ));
    }

    #[derive(Clone)]
    struct TestServer {
        signals: tokio_mpsc::UnboundedSender<Sig>,
        pty_requests: tokio_mpsc::UnboundedSender<(String, u32, u32)>,
        pty_input: tokio_mpsc::UnboundedSender<Vec<u8>>,
        pty_resize: tokio_mpsc::UnboundedSender<(u32, u32)>,
    }

    impl server::Server for TestServer {
        type Handler = Self;

        fn new_client(&mut self, _peer_addr: Option<std::net::SocketAddr>) -> Self::Handler {
            self.clone()
        }
    }

    impl server::Handler for TestServer {
        type Error = russh::Error;

        async fn auth_publickey(
            &mut self,
            _user: &str,
            _key: &russh::keys::ssh_key::PublicKey,
        ) -> Result<Auth, Self::Error> {
            Ok(Auth::Accept)
        }

        async fn channel_open_session(
            &mut self,
            _channel: Channel<Msg>,
            reply: ChannelOpenHandle,
            _session: &mut Session,
        ) -> Result<(), Self::Error> {
            reply.accept().await;
            Ok(())
        }

        async fn exec_request(
            &mut self,
            channel: ChannelId,
            command: &[u8],
            session: &mut Session,
        ) -> Result<(), Self::Error> {
            session.channel_success(channel)?;
            if command == b"hang" {
                return Ok(());
            }
            session.data(channel, &b"first\r\nsecond\n"[..])?;
            session.extended_data(channel, 1, &b"warning\n"[..])?;
            session.exit_status_request(channel, 7)?;
            session.eof(channel)?;
            session.close(channel)?;
            Ok(())
        }

        async fn signal(
            &mut self,
            _channel: ChannelId,
            signal: Sig,
            _session: &mut Session,
        ) -> Result<(), Self::Error> {
            let _ = self.signals.send(signal);
            Ok(())
        }

        async fn pty_request(
            &mut self,
            channel: ChannelId,
            term: &str,
            columns: u32,
            rows: u32,
            _pixel_width: u32,
            _pixel_height: u32,
            _modes: &[(Pty, u32)],
            session: &mut Session,
        ) -> Result<(), Self::Error> {
            let _ = self.pty_requests.send((term.to_string(), columns, rows));
            session.channel_success(channel)?;
            session.data(channel, &b"pty-ready"[..])?;
            Ok(())
        }

        async fn shell_request(
            &mut self,
            channel: ChannelId,
            session: &mut Session,
        ) -> Result<(), Self::Error> {
            session.channel_success(channel)?;
            Ok(())
        }

        async fn data(
            &mut self,
            channel: ChannelId,
            data: &[u8],
            session: &mut Session,
        ) -> Result<(), Self::Error> {
            let _ = self.pty_input.send(data.to_vec());
            session.data(channel, data.to_vec())?;
            Ok(())
        }

        async fn window_change_request(
            &mut self,
            _channel: ChannelId,
            columns: u32,
            rows: u32,
            _pixel_width: u32,
            _pixel_height: u32,
            _session: &mut Session,
        ) -> Result<(), Self::Error> {
            let _ = self.pty_resize.send((columns, rows));
            Ok(())
        }
    }

    #[tokio::test]
    async fn transport_executes_streams_and_cancels_over_in_process_ssh() {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let data_dir =
            std::env::temp_dir().join(format!("hacc-ssh-test-{}-{timestamp}", std::process::id()));
        let client_key = crate::termux::keys::load_or_create(&data_dir).unwrap();
        let host_key = PrivateKey::from(russh::keys::ssh_key::private::Ed25519Keypair::from_seed(
            &[1u8; 32],
        ));
        let config = Arc::new(server::Config {
            keys: vec![host_key],
            ..Default::default()
        });
        let (signal_sender, mut signal_receiver) = tokio_mpsc::unbounded_channel();
        let (pty_request_sender, mut pty_request_receiver) = tokio_mpsc::unbounded_channel();
        let (pty_input_sender, mut pty_input_receiver) = tokio_mpsc::unbounded_channel();
        let (pty_resize_sender, mut pty_resize_receiver) = tokio_mpsc::unbounded_channel();
        let (address_sender, address_receiver) = oneshot::channel();
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let server_task = tokio::spawn(async move {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let address = listener.local_addr().unwrap();
            let mut server = TestServer {
                signals: signal_sender,
                pty_requests: pty_request_sender,
                pty_input: pty_input_sender,
                pty_resize: pty_resize_sender,
            };
            let mut running = server.run_on_socket(config, &listener);
            let handle = running.handle();
            address_sender.send(address).unwrap();
            tokio::select! {
                result = &mut running => result.unwrap(),
                _ = shutdown_receiver => {
                    handle.shutdown("test complete".into());
                    running.await.unwrap();
                }
            }
        });

        let address = address_receiver.await.unwrap();
        let known_hosts = KnownHosts::load(&data_dir);
        let transport = TermuxSshTransport::new(
            connect(
                address.port(),
                "termux",
                &client_key,
                &known_hosts,
                "test-device",
            )
            .await
            .unwrap(),
        );
        assert!(known_hosts.expected("test-device").unwrap().is_some());

        let result = transport
            .execute("probe", Duration::from_secs(2))
            .await
            .unwrap();
        assert_eq!(result.stdout, "first\r\nsecond\n");
        assert_eq!(result.stderr, "warning\n");
        assert_eq!(result.exit_code, Some(7));

        let cancel = CancellationToken::new();
        let mut events = transport.stream("probe", cancel.clone()).await.unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_code = None;
        while let Some(event) = tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .unwrap()
        {
            match event {
                StreamEvent::Stdout { line } => stdout.push(line),
                StreamEvent::Stderr { line } => stderr.push(line),
                StreamEvent::Exit { code, .. } => {
                    exit_code = code;
                    break;
                }
                StreamEvent::Error { .. } => panic!("SSH stream returned an error"),
            }
        }
        assert_eq!(stdout, ["first", "second"]);
        assert_eq!(stderr, ["warning"]);
        assert_eq!(exit_code, Some(7));

        let (mut pty_output, pty_control) = transport.open_pty(80, 24).await.unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), pty_request_receiver.recv())
                .await
                .unwrap(),
            Some(("xterm-256color".into(), 80, 24))
        );
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), pty_output.recv())
                .await
                .unwrap(),
            Some(b"pty-ready".to_vec())
        );
        pty_control
            .send(PtyInput::Data(b"interactive input\n".to_vec()))
            .await
            .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), pty_input_receiver.recv())
                .await
                .unwrap(),
            Some(b"interactive input\n".to_vec())
        );
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), pty_output.recv())
                .await
                .unwrap(),
            Some(b"interactive input\n".to_vec())
        );
        pty_control
            .send(PtyInput::Resize {
                columns: 120,
                rows: 40,
            })
            .await
            .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), pty_resize_receiver.recv())
                .await
                .unwrap(),
            Some((120, 40))
        );
        pty_control.send(PtyInput::Close).await.unwrap();

        let cancel = CancellationToken::new();
        let mut events = transport.stream("hang", cancel.clone()).await.unwrap();
        cancel.cancel();
        let exit = tokio::time::timeout(Duration::from_secs(2), async {
            while let Some(event) = events.recv().await {
                if let StreamEvent::Exit { code, .. } = event {
                    return code;
                }
            }
            None
        })
        .await
        .unwrap();
        assert_eq!(exit, None);
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(2), signal_receiver.recv())
                .await
                .unwrap(),
            Some(Sig::TERM)
        ));

        drop(transport);
        let _ = shutdown_sender.send(());
        server_task.await.unwrap();
        let _ = std::fs::remove_dir_all(data_dir);
    }
}
