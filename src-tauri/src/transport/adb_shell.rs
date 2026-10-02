use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::lines::LineSplitter;
use super::{CommandResult, DeviceTransport, StreamEvent, TransportKind};
use crate::adb::{args, parse, AdbClient};
use crate::error::{AppError, AppResult};
use crate::process::{ProcessRunner, StreamChunk};

/// Runs commands in the Android shell (`adb -s <serial> shell`). Not Termux.
pub struct AdbShellTransport {
    client: AdbClient,
    runner: Arc<dyn ProcessRunner>,
    serial: String,
}

impl AdbShellTransport {
    pub fn new(
        client: AdbClient,
        runner: Arc<dyn ProcessRunner>,
        serial: impl Into<String>,
    ) -> Self {
        Self {
            client,
            runner,
            serial: serial.into(),
        }
    }
}

fn validate(command: &str) -> AppResult<()> {
    if command.trim().is_empty() {
        return Err(AppError::Config("Enter a command to run.".into()));
    }
    Ok(())
}

#[async_trait]
impl DeviceTransport for AdbShellTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::AdbShell
    }

    async fn execute(&self, command: &str, timeout: Duration) -> AppResult<CommandResult> {
        validate(command)?;
        let out = self.client.shell(&self.serial, command, timeout).await?;
        Ok(CommandResult {
            stdout: out.stdout,
            stderr: parse::strip_daemon_noise(&out.stderr),
            exit_code: out.exit_code,
            duration_ms: out.duration_ms,
        })
    }

    async fn stream(
        &self,
        command: &str,
        cancel: CancellationToken,
    ) -> AppResult<mpsc::Receiver<StreamEvent>> {
        validate(command)?;
        let started = Instant::now();
        let mut raw = self
            .runner
            .spawn_stream(
                self.client.path(),
                &args::shell(&self.serial, command),
                cancel,
            )
            .await?;
        let (tx, rx) = mpsc::channel(512);
        let serial = self.serial.clone();
        tokio::spawn(async move {
            let mut out = LineSplitter::default();
            let mut err = LineSplitter::default();
            let mut stderr_text = String::new();
            while let Some(chunk) = raw.recv().await {
                let events: Vec<StreamEvent> = match chunk {
                    StreamChunk::Stdout(b) => out
                        .push(&b)
                        .into_iter()
                        .map(|line| StreamEvent::Stdout { line })
                        .collect(),
                    StreamChunk::Stderr(b) => err
                        .push(&b)
                        .into_iter()
                        .inspect(|l| {
                            stderr_text.push_str(l);
                            stderr_text.push('\n');
                        })
                        .map(|line| StreamEvent::Stderr { line })
                        .collect(),
                    StreamChunk::Exit(code) => {
                        let mut tail = Vec::new();
                        tail.extend(out.finish().map(|line| StreamEvent::Stdout { line }));
                        tail.extend(err.finish().map(|line| StreamEvent::Stderr { line }));
                        // adb itself failing (device gone) is reported as a structured error.
                        let lower = stderr_text.to_lowercase();
                        if code != Some(0)
                            && (lower.contains("device offline")
                                || lower.contains("not found")
                                || lower.contains("unauthorized"))
                        {
                            tail.push(StreamEvent::Error {
                                error: parse::map_device_error(&serial, &stderr_text, code)
                                    .to_payload(),
                            });
                        } else {
                            tail.push(StreamEvent::Exit {
                                code,
                                duration_ms: started.elapsed().as_millis() as u64,
                            });
                        }
                        tail
                    }
                };
                for e in events {
                    if tx.send(e).await.is_err() {
                        return;
                    }
                }
            }
        });
        Ok(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{FakeRunner, RawOutput};

    fn transport(f: &FakeRunner) -> AdbShellTransport {
        let runner: Arc<dyn ProcessRunner> = Arc::new(f.clone());
        AdbShellTransport::new(AdbClient::new("/adb", runner.clone()), runner, "S")
    }

    async fn collect(mut rx: mpsc::Receiver<StreamEvent>) -> Vec<StreamEvent> {
        let mut v = Vec::new();
        while let Some(e) = rx.recv().await {
            v.push(e);
        }
        v
    }

    #[tokio::test]
    async fn execute_returns_exit_code_and_streams() {
        let f = FakeRunner::new();
        f.on(
            "-s S shell ls /nope",
            Ok(RawOutput {
                stdout: String::new(),
                stderr: "ls: /nope: No such file".into(),
                exit_code: Some(1),
                duration_ms: 12,
            }),
        );
        let r = transport(&f)
            .execute("ls /nope", Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(r.exit_code, Some(1));
        assert!(r.stderr.contains("No such file"));
    }

    #[tokio::test]
    async fn rejects_empty_command() {
        let f = FakeRunner::new();
        assert!(transport(&f)
            .execute("   ", Duration::from_secs(1))
            .await
            .is_err());
        assert!(f.calls().is_empty());
    }

    #[tokio::test]
    async fn stream_emits_lines_in_order_then_exit() {
        let f = FakeRunner::new();
        f.on_stream(
            "-s S shell echo hi",
            vec![
                StreamChunk::Stdout(b"a\nb".to_vec()),
                StreamChunk::Stderr(b"warn\n".to_vec()),
                StreamChunk::Stdout(b"c\n".to_vec()),
                StreamChunk::Exit(Some(3)),
            ],
        );
        let ev = collect(
            transport(&f)
                .stream("echo hi", CancellationToken::new())
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(ev[0], StreamEvent::Stdout { line: "a".into() });
        assert_eq!(
            ev[1],
            StreamEvent::Stderr {
                line: "warn".into()
            }
        );
        assert_eq!(ev[2], StreamEvent::Stdout { line: "bc".into() });
        assert!(matches!(ev[3], StreamEvent::Exit { code: Some(3), .. }));
        assert_eq!(ev.len(), 4);
    }

    #[tokio::test]
    async fn stream_reports_device_offline_as_error() {
        let f = FakeRunner::new();
        f.on_stream(
            "-s S shell top",
            vec![
                StreamChunk::Stderr(b"error: device offline\n".to_vec()),
                StreamChunk::Exit(Some(1)),
            ],
        );
        let ev = collect(
            transport(&f)
                .stream("top", CancellationToken::new())
                .await
                .unwrap(),
        )
        .await;
        assert!(
            matches!(ev.last().unwrap(), StreamEvent::Error { error } if error.kind == crate::error::ErrorKind::DeviceOffline)
        );
    }
}
