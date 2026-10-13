//! Device transports (ADR-004): the UI never knows whether a command ran via ADB, SSH or the API.

pub mod adb_shell;
pub mod api;
pub mod lines;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::error::{AppError, AppResult, ErrorPayload};

pub(crate) fn validate_command(command: &str) -> AppResult<()> {
    if command.trim().is_empty() {
        return Err(AppError::Config("Enter a command to run.".into()));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TransportKind {
    /// Android shell (`adb shell`, uid `shell`) — not Termux.
    AdbShell,
    TermuxSsh,
    Api,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    #[ts(type = "number")]
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
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
        #[serde(rename = "durationMs")]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_validation_rejects_empty_and_whitespace() {
        for command in ["", " ", "\t\r\n", "\u{2003}"] {
            assert!(matches!(
                validate_command(command),
                Err(AppError::Config(message)) if message == "Enter a command to run.",
            ));
        }
    }

    #[test]
    fn command_validation_preserves_nonempty_shell_commands() {
        for command in ["echo hello", "  printf ' '; echo done\n", "false || true"] {
            let original = command.to_string();
            validate_command(command).unwrap();
            assert_eq!(command, original);
        }
    }
}
