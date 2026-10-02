use std::sync::Arc;
use std::time::Duration;

use tauri::ipc::Channel;
use tauri::State;

use crate::error::AppError;
use crate::state::AppState;
use crate::streams::StreamId;
use crate::transport::adb_shell::AdbShellTransport;
use crate::transport::{CommandResult, DeviceTransport, StreamEvent};

const EXECUTE_TIMEOUT: Duration = Duration::from_secs(60);

async fn transport(state: &AppState, serial: &str) -> Result<Arc<dyn DeviceTransport>, AppError> {
    let client = state.adb_client().await?;
    Ok(Arc::new(AdbShellTransport::new(
        client,
        state.runner.clone(),
        serial,
    )))
}

/// Runs a user-submitted command and waits for it (60 s cap).
#[tauri::command]
pub async fn execute_command(
    state: State<'_, AppState>,
    serial: String,
    command: String,
) -> Result<CommandResult, AppError> {
    tracing::debug!(%serial, "execute_command");
    transport(&state, &serial)
        .await?
        .execute(&command, EXECUTE_TIMEOUT)
        .await
}

/// Streams a user-submitted command's output; returns an id for `cancel_stream`.
#[tauri::command]
pub async fn stream_command(
    state: State<'_, AppState>,
    serial: String,
    command: String,
    on_event: Channel<StreamEvent>,
) -> Result<StreamId, AppError> {
    tracing::debug!(%serial, "stream_command");
    let t = transport(&state, &serial).await?;
    let (id, cancel) = state.streams.register(&serial, "cmd");
    let mut rx = match t.stream(&command, cancel).await {
        Ok(rx) => rx,
        Err(e) => {
            state.streams.finish(&id);
            return Err(e);
        }
    };
    let streams = state.streams.clone();
    let stream_id = id.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(ev) = rx.recv().await {
            if on_event.send(ev).is_err() {
                break;
            }
        }
        streams.cancel(&stream_id);
    });
    Ok(id)
}

#[tauri::command]
pub async fn cancel_stream(
    state: State<'_, AppState>,
    stream_id: String,
) -> Result<bool, AppError> {
    Ok(state.streams.cancel(&stream_id))
}
