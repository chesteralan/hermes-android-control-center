use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::parse::parse_threadtime;
use super::{LogLine, LogSource};
use crate::adb::{args, AdbClient};
use crate::error::AppResult;
use crate::process::{ProcessRunner, StreamChunk};
use crate::transport::lines::LineSplitter;

/// Recent lines replayed when the stream starts, before following live output.
pub const BACKLOG: &str = "500";

pub struct LogcatSource {
    client: AdbClient,
    runner: Arc<dyn ProcessRunner>,
    serial: String,
    filter: Vec<String>,
    serial_resolver: Option<Arc<dyn Fn() -> String + Send + Sync>>,
}

impl LogcatSource {
    pub fn new(
        client: AdbClient,
        runner: Arc<dyn ProcessRunner>,
        serial: &str,
        filter: &str,
    ) -> Self {
        Self {
            client,
            runner,
            serial: serial.to_string(),
            filter: filter.split_whitespace().map(str::to_string).collect(),
            serial_resolver: None,
        }
    }

    /// Re-resolve the device's current serial on every (re)spawn so a
    /// wireless port/alias change doesn't strand the stream on a dead serial.
    pub fn with_serial_resolver(mut self, resolver: Arc<dyn Fn() -> String + Send + Sync>) -> Self {
        self.serial_resolver = Some(resolver);
        self
    }

    pub fn argv(&self) -> Vec<String> {
        make_argv(
            self.serial.clone(),
            self.filter.clone(),
            self.serial_resolver.clone(),
        )()
    }
}

/// Build the argv factory: serials are re-resolved on every (re)spawn.
fn make_argv(
    serial: String,
    filter: Vec<String>,
    resolver: Option<Arc<dyn Fn() -> String + Send + Sync>>,
) -> impl Fn() -> Vec<String> {
    move || {
        let current = resolver
            .as_ref()
            .map(|f| f())
            .unwrap_or_else(|| serial.clone());
        let mut extra = vec!["-T".to_string(), BACKLOG.to_string()];
        extra.extend(filter.iter().cloned());
        args::logcat(&current, &extra)
    }
}

/// Backoff schedule for respawn-after-exit (exponential, capped).
const RESTART_INITIAL: std::time::Duration = std::time::Duration::from_secs(1);
const RESTART_CAP: std::time::Duration = std::time::Duration::from_secs(8);
/// A stream that survived this long resets the backoff schedule.
const RESTART_STABLE_AFTER: std::time::Duration = std::time::Duration::from_secs(30);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[async_trait]
impl LogSource for LogcatSource {
    fn name(&self) -> String {
        format!("logcat {}", self.filter.join(" "))
    }

    async fn stream(&self, cancel: CancellationToken) -> AppResult<mpsc::Receiver<LogLine>> {
        let make_argv = make_argv(
            self.serial.clone(),
            self.filter.clone(),
            self.serial_resolver.clone(),
        );
        let mut raw = self
            .runner
            .spawn_stream(self.client.path(), &make_argv(), cancel.clone())
            .await?;
        let (tx, rx) = mpsc::channel(4096);
        let client = self.client.clone();
        let runner = self.runner.clone();
        tokio::spawn(async move {
            let mut seq = 0u64;
            let mut backoff = RESTART_INITIAL;
            loop {
                let spawned_at = tokio::time::Instant::now();
                {
                    let mut split = LineSplitter::default();
                    while let Some(chunk) = raw.recv().await {
                        let lines = match chunk {
                            StreamChunk::Stdout(b) | StreamChunk::Stderr(b) => split.push(&b),
                            StreamChunk::Exit(_) => split.finish().into_iter().collect(),
                        };
                        for l in lines.into_iter().filter(|l| !l.is_empty()) {
                            seq += 1;
                            if tx.send(parse_threadtime(seq, now_ms(), &l)).await.is_err() {
                                return;
                            }
                        }
                    }
                }
                if cancel.is_cancelled() {
                    return;
                }
                if spawned_at.elapsed() >= RESTART_STABLE_AFTER {
                    backoff = RESTART_INITIAL;
                }
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(backoff) => {}
                }
                backoff = (backoff * 2).min(RESTART_CAP);
                // Respawn until a child opens; serials are re-resolved per attempt.
                loop {
                    let argv = make_argv();
                    match runner
                        .spawn_stream(client.path(), &argv, cancel.clone())
                        .await
                    {
                        Ok(next) => {
                            raw = next;
                            break;
                        }
                        Err(_) => {
                            tokio::select! {
                                _ = cancel.cancelled() => return,
                                _ = tokio::time::sleep(backoff) => {}
                            }
                            backoff = (backoff * 2).min(RESTART_CAP);
                        }
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
    use crate::process::FakeRunner;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[tokio::test(start_paused = true)]
    async fn restarts_after_child_exit_and_keeps_the_receiver_alive() {
        let f = FakeRunner::new();
        f.on_stream(
            "-s S logcat -v threadtime -T 500 *:I",
            vec![
                StreamChunk::Stdout(b"10-02 16:11:21.369  1  2 I Tag: first\n".to_vec()),
                StreamChunk::Exit(None),
            ],
        );
        let calls = f.calls.clone();
        let runner: Arc<dyn ProcessRunner> = Arc::new(f);
        let src = LogcatSource::new(AdbClient::new("/adb", runner.clone()), runner, "S", "*:I");
        let cancel = CancellationToken::new();
        let mut rx = src.stream(cancel.clone()).await.unwrap();

        let a = rx.recv().await.unwrap();
        assert_eq!(a.seq, 1);
        assert_eq!(a.message, "first");

        // The child exited; the source must respawn instead of ending the stream.
        let spawned = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                if calls
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|c| c.join(" ").contains("logcat"))
                    .count()
                    >= 2
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(spawned.is_ok(), "logcat child was never restarted");

        // Lines from the restarted child keep flowing on the same receiver.
        let b = rx.recv().await.unwrap();
        assert_eq!(b.seq, 2);
        cancel.cancel();
    }

    #[tokio::test(start_paused = true)]
    async fn respawn_uses_the_serial_resolved_at_spawn_time() {
        let f = FakeRunner::new();
        f.on_stream(
            "-s S1 logcat -v threadtime -T 500 *:I",
            vec![
                StreamChunk::Stdout(b"10-02 16:11:21.369  1  2 I Tag: one\n".to_vec()),
                StreamChunk::Exit(None),
            ],
        );
        f.on_stream(
            "-s S2 logcat -v threadtime -T 500 *:I",
            vec![
                StreamChunk::Stdout(b"10-02 16:11:22.000  1  2 I Tag: two\n".to_vec()),
                StreamChunk::Exit(None),
            ],
        );
        let calls = f.calls.clone();
        let runner: Arc<dyn ProcessRunner> = Arc::new(f);
        let rebound = Arc::new(AtomicBool::new(false));
        let rebound_flag = rebound.clone();
        let src = LogcatSource::new(AdbClient::new("/adb", runner.clone()), runner, "S1", "*:I")
            .with_serial_resolver(Arc::new(move || {
                if rebound_flag.load(Ordering::SeqCst) {
                    "S2".to_string()
                } else {
                    "S1".to_string()
                }
            }));
        let cancel = CancellationToken::new();
        let mut rx = src.stream(cancel.clone()).await.unwrap();

        let a = rx.recv().await.unwrap();
        assert_eq!(a.message, "one");
        // The device rebinds to a new serial before the first respawn.
        rebound.store(true, Ordering::SeqCst);

        let respawned_on_new = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                if calls
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|c| c.join(" ") == "-s S2 logcat -v threadtime -T 500 *:I")
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(
            respawned_on_new.is_ok(),
            "respawn kept using the stale serial after rebind"
        );
        cancel.cancel();
    }

    #[tokio::test]
    async fn streams_parsed_lines_with_backlog_and_filter() {
        let f = FakeRunner::new();
        f.on_stream(
            "-s S logcat -v threadtime -T 500 *:I",
            vec![
                StreamChunk::Stdout(
                    b"--------- beginning of main\n10-02 16:11:21.369  1  2 W Tag: hel".to_vec(),
                ),
                StreamChunk::Stdout(b"lo\n".to_vec()),
                StreamChunk::Exit(None),
            ],
        );
        let runner: Arc<dyn ProcessRunner> = Arc::new(f);
        let src = LogcatSource::new(AdbClient::new("/adb", runner.clone()), runner, "S", "*:I");
        let cancel = CancellationToken::new();
        let mut rx = src.stream(cancel.clone()).await.unwrap();
        let a = rx.recv().await.unwrap();
        let b = rx.recv().await.unwrap();
        assert_eq!((a.seq, a.level), (1, None));
        assert_eq!(b.seq, 2);
        assert_eq!(b.message, "hello");
        // Streams stay open across child exits (restart semantics); cancelling
        // is what tears the receiver down.
        cancel.cancel();
        let ended = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while rx.recv().await.is_some() {}
        })
        .await;
        assert!(ended.is_ok(), "receiver must close after cancellation");
    }
}
