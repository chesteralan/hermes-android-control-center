//! Long-lived `adb track-devices` consumer (ADR-007) with polling fallback.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::client::AdbClient;
use super::parse::{parse_devices, TrackParser};
use super::types::AndroidDevice;
use crate::devices::{DeviceRegistry, SnapshotDiff};
use crate::error::AppResult;
use crate::process::StreamChunk;

pub type ClientFuture = Pin<Box<dyn Future<Output = AppResult<AdbClient>> + Send>>;
pub type ClientFn = Arc<dyn Fn() -> ClientFuture + Send + Sync>;

pub struct TrackerHooks {
    pub get_client: ClientFn,
    pub on_snapshot: Arc<dyn Fn(Vec<AndroidDevice>, SnapshotDiff) + Send + Sync>,
}

const RESTART_DELAYS: [u64; 3] = [1, 2, 5];
const POLL_INTERVAL: Duration = Duration::from_secs(5);
const POLL_ROUNDS_BEFORE_RETRY: u32 = 12;

pub async fn run(registry: Arc<DeviceRegistry>, hooks: TrackerHooks, cancel: CancellationToken) {
    let mut failures: usize = 0;
    while !cancel.is_cancelled() {
        let client = match (hooks.get_client)().await {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(error = %e, "tracker: adb unavailable");
                if sleep_or_cancel(Duration::from_secs(5), &cancel).await {
                    return;
                }
                continue;
            }
        };

        if failures >= RESTART_DELAYS.len() {
            tracing::warn!("track-devices keeps failing; polling `adb devices` instead");
            for _ in 0..POLL_ROUNDS_BEFORE_RETRY {
                if let Ok(list) = client.devices().await {
                    publish(&registry, &hooks, list);
                }
                if sleep_or_cancel(POLL_INTERVAL, &cancel).await {
                    return;
                }
            }
            failures = 0;
            continue;
        }

        let healthy = consume(&client, &registry, &hooks, &cancel).await;
        if cancel.is_cancelled() {
            return;
        }
        failures = if healthy { 0 } else { failures + 1 };
        let delay = RESTART_DELAYS[failures.min(RESTART_DELAYS.len() - 1)];
        if sleep_or_cancel(Duration::from_secs(delay), &cancel).await {
            return;
        }
    }
}

/// Returns true if the stream delivered at least one frame before ending.
async fn consume(
    client: &AdbClient,
    registry: &DeviceRegistry,
    hooks: &TrackerHooks,
    cancel: &CancellationToken,
) -> bool {
    let child = cancel.child_token();
    let mut rx = match client.track_devices(child.clone()).await {
        Ok(rx) => rx,
        Err(e) => {
            tracing::warn!(error = %e, "track-devices failed to start");
            return false;
        }
    };
    let mut parser = TrackParser::default();
    let mut got_frame = false;
    while let Some(chunk) = rx.recv().await {
        match chunk {
            StreamChunk::Stdout(bytes) => {
                for frame in parser.push(&bytes) {
                    got_frame = true;
                    publish(registry, hooks, parse_devices(&frame));
                }
            }
            StreamChunk::Stderr(bytes) => {
                tracing::debug!(stderr = %String::from_utf8_lossy(&bytes), "track-devices")
            }
            StreamChunk::Exit(code) => {
                tracing::info!(?code, "track-devices exited");
                break;
            }
        }
    }
    child.cancel();
    got_frame
}

fn publish(registry: &DeviceRegistry, hooks: &TrackerHooks, list: Vec<AndroidDevice>) {
    let diff = registry.apply_snapshot(list);
    if diff.changed || !diff.lost.is_empty() || !diff.ready.is_empty() {
        (hooks.on_snapshot)(registry.list(), diff);
    }
}

async fn sleep_or_cancel(d: Duration, cancel: &CancellationToken) -> bool {
    tokio::select! {
        _ = cancel.cancelled() => true,
        _ = tokio::time::sleep(d) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::FakeRunner;
    use std::sync::Mutex;

    #[tokio::test(start_paused = true)]
    async fn publishes_frames_from_stream() {
        let f = FakeRunner::new();
        f.on_stream(
            "track-devices -l",
            vec![
                StreamChunk::Stdout(
                    include_bytes!("../../tests/fixtures/adb/track_frames.bin").to_vec(),
                ),
                StreamChunk::Stdout(b"0000".to_vec()),
                StreamChunk::Exit(Some(0)),
            ],
        );
        let client = AdbClient::new("/adb", Arc::new(f));
        let registry = Arc::new(DeviceRegistry::new());
        let seen: Arc<Mutex<Vec<(usize, usize, usize)>>> = Arc::default();
        let s = seen.clone();
        let hooks = TrackerHooks {
            get_client: Arc::new(move || {
                let c = client.clone();
                Box::pin(async move { Ok(c) })
            }),
            on_snapshot: Arc::new(move |list, diff| {
                s.lock()
                    .unwrap()
                    .push((list.len(), diff.ready.len(), diff.lost.len()))
            }),
        };
        let cancel = CancellationToken::new();
        let task = tokio::spawn(run(registry, hooks, cancel.clone()));
        tokio::time::sleep(Duration::from_millis(100)).await;
        cancel.cancel();
        task.await.unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0], (1, 1, 0), "device appeared");
        assert_eq!(seen[1], (0, 0, 1), "device lost");
    }
}
