use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::parse::LogLineParser;
use super::{LogLine, LogSource};
use crate::error::{AppError, AppResult};
use crate::transport::{DeviceTransport, StreamEvent};

const INITIAL_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(8);
const OUTPUT_BUFFER: usize = 4096;
const RECONNECTED_MARKER: &str = "[log stream reconnected]";

pub struct TransportCommandLogSource {
    transport: Arc<dyn DeviceTransport>,
    command: String,
    initial_backoff: Duration,
    max_backoff: Duration,
}

impl TransportCommandLogSource {
    pub fn new(transport: Arc<dyn DeviceTransport>, command: impl Into<String>) -> Self {
        Self {
            transport,
            command: command.into(),
            initial_backoff: INITIAL_BACKOFF,
            max_backoff: MAX_BACKOFF,
        }
    }

    #[cfg(test)]
    fn with_backoff(
        transport: Arc<dyn DeviceTransport>,
        command: impl Into<String>,
        initial_backoff: Duration,
        max_backoff: Duration,
    ) -> Self {
        Self {
            transport,
            command: command.into(),
            initial_backoff,
            max_backoff,
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

async fn send_line(
    sender: &mpsc::Sender<LogLine>,
    cancel: &CancellationToken,
    stream_cancel: &CancellationToken,
    line: LogLine,
) -> bool {
    tokio::select! {
        _ = cancel.cancelled() => false,
        result = sender.send(line) => {
            if result.is_err() {
                stream_cancel.cancel();
                false
            } else {
                true
            }
        },
    }
}

#[async_trait]
impl LogSource for TransportCommandLogSource {
    fn name(&self) -> String {
        "Termux command stream".into()
    }

    async fn stream(&self, cancel: CancellationToken) -> AppResult<mpsc::Receiver<LogLine>> {
        if self.command.trim().is_empty() {
            return Err(AppError::Config("Enter a log stream command.".into()));
        }

        let (sender, receiver) = mpsc::channel(OUTPUT_BUFFER);
        let transport = self.transport.clone();
        let command = self.command.clone();
        let initial_backoff = self.initial_backoff;
        let max_backoff = self.max_backoff.max(initial_backoff);

        tokio::spawn(async move {
            let mut parser = LogLineParser::new(true);
            let mut connected = false;
            let mut reconnecting = false;
            let mut backoff = initial_backoff;

            loop {
                let stream_cancel = cancel.child_token();
                let stream = tokio::select! {
                    _ = cancel.cancelled() => break,
                    result = transport.stream(&command, stream_cancel.clone()) => result,
                };

                match stream {
                    Ok(mut events) => {
                        let stream_started_at = tokio::time::Instant::now();
                        if reconnecting {
                            for line in parser.push_standalone(now_ms(), RECONNECTED_MARKER) {
                                if !send_line(&sender, &cancel, &stream_cancel, line).await {
                                    return;
                                }
                            }
                        }
                        connected = true;

                        let mut ended = false;
                        while !ended {
                            let event = tokio::select! {
                                _ = cancel.cancelled() => break,
                                event = events.recv() => event,
                            };
                            match event {
                                Some(StreamEvent::Stdout { line })
                                | Some(StreamEvent::Stderr { line }) => {
                                    for parsed in parser.push(now_ms(), &line) {
                                        if !send_line(&sender, &cancel, &stream_cancel, parsed)
                                            .await
                                        {
                                            return;
                                        }
                                    }
                                }
                                Some(StreamEvent::Exit { .. })
                                | Some(StreamEvent::Error { .. })
                                | None => {
                                    if let Some(parsed) = parser.finish() {
                                        if !send_line(&sender, &cancel, &stream_cancel, parsed)
                                            .await
                                        {
                                            return;
                                        }
                                    }
                                    ended = true;
                                }
                            }
                        }
                        if cancel.is_cancelled() {
                            break;
                        }
                        if stream_started_at.elapsed() >= Duration::from_secs(30) {
                            backoff = initial_backoff;
                        }
                        reconnecting = connected;
                    }
                    Err(_) => {
                        reconnecting = connected;
                    }
                }

                if cancel.is_cancelled() {
                    break;
                }
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = tokio::time::sleep(backoff) => {}
                }
                backoff = backoff.saturating_mul(2).min(max_backoff);
            }
        });

        Ok(receiver)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use super::*;
    use crate::transport::{CommandResult, TransportKind};

    struct RestartingTransport {
        starts: AtomicUsize,
        commands: Mutex<Vec<String>>,
    }

    struct ReceiverDropTransport {
        session_cancel: Mutex<Option<CancellationToken>>,
    }

    struct FailingTransport {
        starts: AtomicUsize,
    }

    #[async_trait]
    impl DeviceTransport for ReceiverDropTransport {
        fn kind(&self) -> TransportKind {
            TransportKind::TermuxSsh
        }

        async fn execute(&self, _command: &str, _timeout: Duration) -> AppResult<CommandResult> {
            Err(AppError::Config("execute is not used by this test".into()))
        }

        async fn stream(
            &self,
            _command: &str,
            cancel: CancellationToken,
        ) -> AppResult<mpsc::Receiver<StreamEvent>> {
            *self.session_cancel.lock().unwrap() = Some(cancel);
            let (sender, receiver) = mpsc::channel(1);
            sender
                .send(StreamEvent::Stdout {
                    line: "one line".into(),
                })
                .await
                .unwrap();
            Ok(receiver)
        }
    }

    #[async_trait]
    impl DeviceTransport for FailingTransport {
        fn kind(&self) -> TransportKind {
            TransportKind::TermuxSsh
        }

        async fn execute(&self, _command: &str, _timeout: Duration) -> AppResult<CommandResult> {
            Err(AppError::Config("execute is not used by this test".into()))
        }

        async fn stream(
            &self,
            _command: &str,
            cancel: CancellationToken,
        ) -> AppResult<mpsc::Receiver<StreamEvent>> {
            if self.starts.fetch_add(1, Ordering::SeqCst) < 4 {
                return Err(AppError::TermuxUnavailable {
                    reason: "test retry".into(),
                });
            }
            let (sender, receiver) = mpsc::channel(1);
            tokio::spawn(async move {
                cancel.cancelled().await;
                drop(sender);
            });
            Ok(receiver)
        }
    }

    #[async_trait]
    impl DeviceTransport for RestartingTransport {
        fn kind(&self) -> TransportKind {
            TransportKind::TermuxSsh
        }

        async fn execute(&self, _command: &str, _timeout: Duration) -> AppResult<CommandResult> {
            Err(AppError::Config("execute is not used by this test".into()))
        }

        async fn stream(
            &self,
            command: &str,
            cancel: CancellationToken,
        ) -> AppResult<mpsc::Receiver<StreamEvent>> {
            self.commands.lock().unwrap().push(command.into());
            let start = self.starts.fetch_add(1, Ordering::SeqCst);
            let (sender, receiver) = mpsc::channel(4);
            if start == 0 {
                sender
                    .send(StreamEvent::Stdout {
                        line: "first line".into(),
                    })
                    .await
                    .unwrap();
                sender
                    .send(StreamEvent::Exit {
                        code: Some(0),
                        duration_ms: 1,
                    })
                    .await
                    .unwrap();
            } else {
                sender
                    .send(StreamEvent::Stderr {
                        line: "2026-10-05 12:30:46,000 ERROR worker: second line".into(),
                    })
                    .await
                    .unwrap();
                sender
                    .send(StreamEvent::Stdout {
                        line: "2026-10-05 12:30:47,000 INFO worker: next line".into(),
                    })
                    .await
                    .unwrap();
                tokio::spawn(async move {
                    cancel.cancelled().await;
                    drop(sender);
                });
            }
            Ok(receiver)
        }
    }

    #[tokio::test(start_paused = true)]
    async fn restarts_dropped_stream_with_marker_and_monotonic_sequence() {
        let transport = Arc::new(RestartingTransport {
            starts: AtomicUsize::new(0),
            commands: Mutex::new(Vec::new()),
        });
        let source = TransportCommandLogSource::with_backoff(
            transport.clone(),
            "tail -n 200 -F /root/.hermes/logs/gateway.log",
            Duration::from_millis(10),
            Duration::from_millis(40),
        );
        let cancel = CancellationToken::new();
        let mut lines = source.stream(cancel.clone()).await.unwrap();

        let first = lines.recv().await.unwrap();
        assert_eq!((first.seq, first.raw.as_str()), (1, "first line"));
        tokio::time::advance(Duration::from_millis(10)).await;
        let marker = lines.recv().await.unwrap();
        let second = lines.recv().await.unwrap();
        assert_eq!((marker.seq, marker.raw.as_str()), (2, RECONNECTED_MARKER));
        assert_eq!(
            (second.seq, second.raw.as_str()),
            (3, "2026-10-05 12:30:46,000 ERROR worker: second line")
        );
        assert_eq!(transport.starts.load(Ordering::SeqCst), 2);
        assert_eq!(
            *transport.commands.lock().unwrap(),
            ["tail -n 200 -F /root/.hermes/logs/gateway.log"; 2]
        );

        cancel.cancel();
        let mut trailing = Vec::new();
        while let Some(line) = lines.recv().await {
            trailing.push((line.seq, line.raw));
        }
        assert!(
            trailing.is_empty()
                || trailing == [(4, "2026-10-05 12:30:47,000 INFO worker: next line".into())]
        );
        assert!(lines.recv().await.is_none());
    }

    #[tokio::test]
    async fn rejects_an_empty_stream_command() {
        let transport = Arc::new(RestartingTransport {
            starts: AtomicUsize::new(0),
            commands: Mutex::new(Vec::new()),
        });
        let source = TransportCommandLogSource::new(transport, "  ");

        assert!(matches!(
            source.stream(CancellationToken::new()).await,
            Err(AppError::Config(_))
        ));
    }

    #[tokio::test]
    async fn dropping_the_log_receiver_cancels_the_remote_stream() {
        let transport = Arc::new(ReceiverDropTransport {
            session_cancel: Mutex::new(None),
        });
        let source = TransportCommandLogSource::new(transport.clone(), "tail -F app.log");
        let output = source.stream(CancellationToken::new()).await.unwrap();
        drop(output);

        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let canceled = transport
                    .session_cancel
                    .lock()
                    .unwrap()
                    .as_ref()
                    .is_some_and(CancellationToken::is_cancelled);
                if canceled {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn retries_with_exponential_backoff_capped_at_maximum() {
        let transport = Arc::new(FailingTransport {
            starts: AtomicUsize::new(0),
        });
        let source = TransportCommandLogSource::with_backoff(
            transport.clone(),
            "tail -F app.log",
            Duration::from_millis(10),
            Duration::from_millis(40),
        );
        let cancel = CancellationToken::new();
        let mut output = source.stream(cancel.clone()).await.unwrap();
        tokio::task::yield_now().await;
        assert_eq!(transport.starts.load(Ordering::SeqCst), 1);

        for (delay, expected_starts) in [(9, 1), (1, 2), (20, 3), (40, 4), (40, 5)] {
            tokio::time::advance(Duration::from_millis(delay)).await;
            tokio::task::yield_now().await;
            assert_eq!(transport.starts.load(Ordering::SeqCst), expected_starts);
        }

        cancel.cancel();
        assert!(output.recv().await.is_none());
    }
}
