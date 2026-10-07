use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

use super::{LogLine, LogSource};
use crate::error::AppResult;
use crate::transport::api::ApiTransport;

const INITIAL_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(8);
const OUTPUT_BUFFER: usize = 4096;

pub struct ApiWsLogSource {
    transport: Arc<ApiTransport>,
}

impl ApiWsLogSource {
    pub fn new(transport: Arc<ApiTransport>) -> Self {
        Self { transport }
    }
}

#[async_trait]
impl LogSource for ApiWsLogSource {
    fn name(&self) -> String {
        "Hermes Control API".into()
    }

    async fn stream(&self, cancel: CancellationToken) -> AppResult<mpsc::Receiver<LogLine>> {
        let (sender, receiver) = mpsc::channel(OUTPUT_BUFFER);
        let transport = self.transport.clone();
        tokio::spawn(async move {
            let mut backoff = INITIAL_BACKOFF;
            loop {
                let started = tokio::time::Instant::now();
                let connection = tokio::select! {
                    _ = cancel.cancelled() => break,
                    result = transport.connect_ws("/logs?tail=200") => result,
                };
                if let Ok(mut socket) = connection {
                    loop {
                        let message = tokio::select! {
                            _ = cancel.cancelled() => {
                                let _ = socket.close(None).await;
                                return;
                            }
                            message = socket.next() => message,
                        };
                        match message {
                            Some(Ok(Message::Text(text))) => {
                                if let Ok(line) = serde_json::from_str::<LogLine>(text.as_str()) {
                                    if sender.send(line).await.is_err() {
                                        return;
                                    }
                                }
                            }
                            Some(Ok(Message::Ping(payload))) => {
                                use futures_util::SinkExt;
                                if socket.send(Message::Pong(payload)).await.is_err() {
                                    break;
                                }
                            }
                            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                            Some(Ok(_)) => {}
                        }
                    }
                }
                if cancel.is_cancelled() {
                    break;
                }
                if started.elapsed() >= Duration::from_secs(30) {
                    backoff = INITIAL_BACKOFF;
                }
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = tokio::time::sleep(backoff) => {}
                }
                backoff = backoff.saturating_mul(2).min(MAX_BACKOFF);
            }
        });
        Ok(receiver)
    }
}
