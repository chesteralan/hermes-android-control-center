use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::AppError;
use crate::state::AppState;
use crate::streams::StreamId;
use crate::termux::ssh::PtyInput;
use crate::transport::adb_shell::AdbShellTransport;
use crate::transport::{CommandResult, DeviceTransport, StreamEvent, TransportKind};

const EXECUTE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TerminalPtyEvent {
    Data { data: Vec<u8> },
    Closed,
}

#[tauri::command]
pub async fn export_terminal_text(
    app: AppHandle,
    text: String,
) -> Result<Option<String>, AppError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("Text file", &["txt"])
        .set_file_name("terminal-session.txt")
        .save_file(move |path| {
            let _ = sender.send(path);
        });
    let Some(file_path) = receiver
        .await
        .map_err(|error| AppError::Io(error.to_string()))?
    else {
        return Ok(None);
    };
    let path = file_path
        .into_path()
        .map_err(|error| AppError::Io(error.to_string()))?;
    std::fs::write(&path, text).map_err(|error| AppError::Io(error.to_string()))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

async fn transport(
    state: &AppState,
    serial: &str,
    kind: Option<TransportKind>,
) -> Result<Arc<dyn DeviceTransport>, AppError> {
    match kind.unwrap_or(TransportKind::AdbShell) {
        TransportKind::AdbShell => {
            let client = state.adb_client().await?;
            Ok(Arc::new(AdbShellTransport::new(
                client,
                state.runner.clone(),
                serial,
            )))
        }
        TransportKind::TermuxSsh => Ok(Arc::new(state.termux_transport(serial).await?)),
        TransportKind::Api => Err(AppError::Config(
            "The Hermes Control API transport is not available yet.".into(),
        )),
    }
}

/// Runs a user-submitted command and waits for it (60 s cap).
#[tauri::command]
pub async fn execute_command(
    state: State<'_, AppState>,
    serial: String,
    command: String,
    transport_kind: Option<TransportKind>,
) -> Result<CommandResult, AppError> {
    tracing::debug!(%serial, ?transport_kind, "execute_command");
    transport(&state, &serial, transport_kind)
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
    transport_kind: Option<TransportKind>,
    on_event: Channel<StreamEvent>,
) -> Result<StreamId, AppError> {
    tracing::debug!(%serial, ?transport_kind, "stream_command");
    let t = transport(&state, &serial, transport_kind).await?;
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

#[tauri::command]
pub async fn start_terminal_pty(
    state: State<'_, AppState>,
    serial: String,
    columns: u32,
    rows: u32,
    on_event: Channel<TerminalPtyEvent>,
) -> Result<String, AppError> {
    let transport = state.termux_transport(&serial).await?;
    let (mut output, control) = transport.open_pty(columns, rows).await?;
    let sessions = state.pty_sessions.clone();
    let session_id = sessions.insert(&serial, control);
    let task_sessions = sessions.clone();
    let task_session_id = session_id.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(data) = output.recv().await {
            if on_event.send(TerminalPtyEvent::Data { data }).is_err() {
                task_sessions.close(&task_session_id).await;
                return;
            }
        }
        let _ = on_event.send(TerminalPtyEvent::Closed);
        task_sessions.close(&task_session_id).await;
    });
    Ok(session_id)
}

#[tauri::command]
pub async fn write_terminal_pty(
    state: State<'_, AppState>,
    session_id: String,
    data: Vec<u8>,
) -> Result<(), AppError> {
    let control = state
        .pty_sessions
        .control(&session_id)
        .ok_or_else(|| AppError::Config("The interactive terminal session has closed.".into()))?;
    control.send(PtyInput::Data(data)).await
}

#[tauri::command]
pub async fn resize_terminal_pty(
    state: State<'_, AppState>,
    session_id: String,
    columns: u32,
    rows: u32,
) -> Result<(), AppError> {
    let control = state
        .pty_sessions
        .control(&session_id)
        .ok_or_else(|| AppError::Config("The interactive terminal session has closed.".into()))?;
    control.send(PtyInput::Resize { columns, rows }).await
}

#[tauri::command]
pub async fn close_terminal_pty(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<bool, AppError> {
    Ok(state.pty_sessions.close(&session_id).await)
}
