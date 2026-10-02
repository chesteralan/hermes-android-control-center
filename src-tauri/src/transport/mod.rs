//! Device transports (ADR-004): the UI never knows whether a command ran via ADB, SSH or the API.

pub mod adb_shell;
pub mod lines;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::error::{AppResult, ErrorPayload};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TransportKind {
    /// Android shell (`adb shell`, uid `shell`) — not Termux.
    AdbShell,
    TermuxSsh,
    Api,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    #[ts(type = "number")]
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum StreamEvent {
    Stdout {
        line: String,
    },
    Stderr {
        line: String,
    },
    Exit {
        code: Option<i32>,
        #[ts(type = "number")]
        duration_ms: u64,
    },
    Error {
        error: ErrorPayload,
    },
}

#[async_trait]
pub trait DeviceTransport: Send + Sync {
    fn kind(&self) -> TransportKind;
    async fn execute(
        &self,
        command: &str,
        timeout: std::time::Duration,
    ) -> AppResult<CommandResult>;
    /// Ends with exactly one `Exit` (or `Error`) event. Cancelling kills the remote command.
    async fn stream(
        &self,
        command: &str,
        cancel: CancellationToken,
    ) -> AppResult<mpsc::Receiver<StreamEvent>>;
}
