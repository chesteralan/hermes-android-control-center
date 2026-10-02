use tauri::ipc::Channel;
use tauri::State;

use crate::error::AppError;
use crate::logs::logcat::LogcatSource;
use crate::logs::{batcher, LogLine, LogSource, LogSourceKind};
use crate::state::AppState;
use crate::streams::StreamId;

/// Starts a persistent log stream; lines arrive in batches (≤50 ms / ≤500 lines).
#[tauri::command]
pub async fn start_log_stream(
    state: State<'_, AppState>,
    serial: String,
    source: LogSourceKind,
    on_batch: Channel<Vec<LogLine>>,
) -> Result<StreamId, AppError> {
    let client = state.adb_client().await?;
    let src: Box<dyn LogSource> = match source {
        LogSourceKind::Logcat => {
            let filter = state.config.read().await.logs.logcat_filter.clone();
            Box::new(LogcatSource::new(
                client,
                state.runner.clone(),
                &serial,
                &filter,
            ))
        }
    };
    let (id, cancel) = state.streams.register(&serial, "logs");
    let rx = match src.stream(cancel).await {
        Ok(rx) => rx,
        Err(e) => {
            state.streams.finish(&id);
            return Err(e);
        }
    };
    tracing::info!(%serial, source = %src.name(), "log stream started");
    let streams = state.streams.clone();
    let stream_id = id.clone();
    tauri::async_runtime::spawn(async move {
        batcher::run(rx, batcher::MAX_BATCH, batcher::FLUSH_EVERY, |batch| {
            on_batch.send(batch).is_ok()
        })
        .await;
        streams.cancel(&stream_id);
    });
    Ok(id)
}
