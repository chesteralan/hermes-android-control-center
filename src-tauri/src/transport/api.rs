use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use reqwest::{Client, Response};
use serde::Deserialize;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::{HeaderValue, AUTHORIZATION};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::{connect_async, WebSocketStream};
use tokio_util::sync::CancellationToken;

use super::{CommandResult, DeviceTransport, StreamEvent, TransportKind};
use crate::error::{AppError, AppResult, ErrorKind, ErrorPayload};
use crate::hermes::status::HermesStatus;

#[derive(Clone)]
pub struct ApiTransport {
    base_url: String,
    token: String,
    client: Client,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiActionResponse {
    pub result: CommandResult,
    pub status: HermesStatus,
}

impl ApiTransport {
    pub fn new(port: u16, token: impl Into<String>) -> Self {
        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            token: token.into(),
            client: Client::new(),
        }
    }

    pub async fn health(&self) -> AppResult<String> {
        let response = self
            .client
            .get(format!("{}/health", self.base_url))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(api_error)?;
        if !response.status().is_success() {
            return Err(response_error(response).await);
        }
        #[derive(Deserialize)]
        struct HealthResponse {
            version: String,
        }
        response
            .json::<HealthResponse>()
            .await
            .map(|health| health.version)
            .map_err(api_error)
    }

    pub async fn status(&self) -> AppResult<HermesStatus> {
        let response = self
            .client
            .get(format!("{}/status", self.base_url))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(api_error)?;
        if !response.status().is_success() {
            return Err(response_error(response).await);
        }
        response.json().await.map_err(api_error)
    }

    pub async fn check_log_stream(&self) -> AppResult<()> {
        let mut socket = self.connect_ws("/logs?tail=1").await?;
        let first_message = tokio::time::timeout(Duration::from_millis(250), socket.next()).await;
        match first_message {
            Ok(Some(Err(error))) => return Err(api_error(error)),
            Ok(Some(Ok(Message::Close(frame)))) => {
                let reason = frame
                    .map(|value| value.reason.to_string())
                    .unwrap_or_default();
                return Err(AppError::Config(format!(
                    "Control API log stream closed during startup: {reason}"
                )));
            }
            _ => {}
        }
        socket.close(None).await.map_err(api_error)
    }

    pub async fn action(&self, action: &str) -> AppResult<ApiActionResponse> {
        if !matches!(action, "start" | "stop" | "restart") {
            return Err(AppError::Config("Unsupported Control API action.".into()));
        }
        let response = self
            .client
            .post(format!("{}/{action}", self.base_url))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(api_error)?;
        if !response.status().is_success() {
            return Err(response_error(response).await);
        }
        response.json().await.map_err(api_error)
    }

    pub(crate) async fn connect_ws(
        &self,
        path: &str,
    ) -> AppResult<WebSocketStream<MaybeTlsStream<TcpStream>>> {
        let url = format!("ws{}{}", &self.base_url[4..], path);
        let mut request = url
            .into_client_request()
            .map_err(|error| AppError::Config(error.to_string()))?;
        let authorization = HeaderValue::from_str(&format!("Bearer {}", self.token))
            .map_err(|error| AppError::Config(error.to_string()))?;
        request.headers_mut().insert(AUTHORIZATION, authorization);
        connect_async(request)
            .await
            .map(|(socket, _)| socket)
            .map_err(api_error)
    }
}

async fn response_error(response: Response) -> AppError {
    let status = response.status();
    let message = response
        .text()
        .await
        .unwrap_or_else(|error| error.to_string());
    AppError::Config(format!("Control API returned HTTP {status}: {message}"))
}

fn api_error(error: impl std::fmt::Display) -> AppError {
    AppError::Config(format!("Control API request failed: {error}"))
}

#[async_trait]
impl DeviceTransport for ApiTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Api
    }

    async fn execute(&self, command: &str, timeout: Duration) -> AppResult<CommandResult> {
        if command.trim().is_empty() {
            return Err(AppError::Config("Enter a command to run.".into()));
        }
        let response = self
            .client
            .post(format!("{}/command", self.base_url))
            .bearer_auth(&self.token)
            .timeout(timeout)
            .json(&serde_json::json!({
                "command": command,
                "timeout_ms": timeout.as_millis().min(u64::MAX as u128) as u64,
            }))
            .send()
            .await
            .map_err(api_error)?;
        if !response.status().is_success() {
            return Err(response_error(response).await);
        }
        response.json().await.map_err(api_error)
    }

    async fn stream(
        &self,
        command: &str,
        cancel: CancellationToken,
    ) -> AppResult<mpsc::Receiver<StreamEvent>> {
        if command.trim().is_empty() {
            return Err(AppError::Config("Enter a command to run.".into()));
        }
        let mut socket = self.connect_ws("/command/stream").await?;
        socket
            .send(Message::Text(
                serde_json::json!({ "command": command }).to_string().into(),
            ))
            .await
            .map_err(api_error)?;
        let (sender, receiver) = mpsc::channel(512);
        tokio::spawn(async move {
            loop {
                let message = tokio::select! {
                    _ = cancel.cancelled() => {
                        let _ = socket.close(None).await;
                        break;
                    }
                    message = socket.next() => message,
                };
                let Some(Ok(Message::Text(text))) = message else {
                    break;
                };
                match serde_json::from_str::<StreamEvent>(text.as_str()) {
                    Ok(event) => {
                        let terminal =
                            matches!(event, StreamEvent::Exit { .. } | StreamEvent::Error { .. });
                        if sender.send(event).await.is_err() || terminal {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = sender
                            .send(StreamEvent::Error {
                                error: ErrorPayload {
                                    kind: ErrorKind::Config,
                                    message: "Control API sent an invalid stream event.".into(),
                                    details: Some(error.to_string()),
                                },
                            })
                            .await;
                        break;
                    }
                }
            }
        });
        Ok(receiver)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_http_port_to_loopback_base_and_rejects_unknown_actions() {
        let transport = ApiTransport::new(43210, "token");
        assert_eq!(transport.base_url, "http://127.0.0.1:43210");
        assert_eq!(transport.kind(), TransportKind::Api);
    }

    #[tokio::test]
    async fn action_names_are_allowlisted_before_network_access() {
        let transport = ApiTransport::new(1, "token");
        let error = transport.action("../command").await.unwrap_err();
        assert!(matches!(error, AppError::Config(message) if message.contains("Unsupported")));
    }

    #[test]
    fn decodes_camel_case_stream_and_action_contracts() {
        let exit: StreamEvent = serde_json::from_value(serde_json::json!({
            "type": "exit",
            "code": 0,
            "durationMs": 12,
        }))
        .unwrap();
        assert!(matches!(
            exit,
            StreamEvent::Exit {
                duration_ms: 12,
                ..
            }
        ));

        let status = crate::hermes::status::parse_status(
            &crate::config::HermesConfig::default(),
            "@@procs\n@@end",
            7,
        );
        let response = serde_json::json!({
            "result": {
                "stdout": "started",
                "stderr": "",
                "exitCode": 0,
                "durationMs": 4,
            },
            "status": status,
        });
        let decoded: ApiActionResponse = serde_json::from_value(response).unwrap();
        assert_eq!(decoded.result.exit_code, Some(0));
        assert_eq!(decoded.status.checked_at, 7);
    }
}
