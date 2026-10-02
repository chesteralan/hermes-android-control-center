//! Log sources (ADR-005): logcat now, Termux files (M7) and API WebSocket (M8) later — same `LogLine`.

pub mod batcher;
pub mod logcat;
pub mod parse;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::error::AppResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LogLine {
    #[ts(type = "number")]
    pub seq: u64,
    /// Unix ms when the line reached the desktop.
    #[ts(type = "number")]
    pub received_at: u64,
    /// Timestamp text as printed by the source, if any.
    pub timestamp: Option<String>,
    pub level: Option<LogLevel>,
    pub tag: Option<String>,
    pub message: String,
    /// Original line, always preserved.
    pub raw: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LogSourceKind {
    Logcat,
}

#[async_trait]
pub trait LogSource: Send + Sync {
    fn name(&self) -> String;
    /// Persistent stream (one process, not one call per line). Ends when `cancel` fires or the source dies.
    async fn stream(&self, cancel: CancellationToken) -> AppResult<mpsc::Receiver<LogLine>>;
}
