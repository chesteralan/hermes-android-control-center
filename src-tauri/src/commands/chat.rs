use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;
use ts_rs::TS;

use crate::config::HermesConfig;
use crate::error::AppError;
use crate::state::AppState;
use crate::streams::StreamId;
use crate::termux::shell_escape;
use crate::termux::ssh::TermuxSshTransport;
use crate::transport::{DeviceTransport, StreamEvent};

const MAX_PROMPT_BYTES: usize = 32 * 1024;
const MAX_STDERR_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum HermesChatEvent {
    Session {
        session_id: String,
    },
    Text {
        text: String,
    },
    ToolUse {
        name: String,
    },
    ToolResult {
        name: String,
        output: Option<String>,
        is_error: bool,
    },
    Complete {
        session_id: Option<String>,
        exit_code: Option<i32>,
        error: Option<String>,
    },
    Error {
        message: String,
    },
}

fn build_chat_command(
    cfg: &HermesConfig,
    prompt: &str,
    session_id: Option<&str>,
) -> Result<String, AppError> {
    if prompt.trim().is_empty() {
        return Err(AppError::Config("Enter a message for Hermes.".into()));
    }
    if prompt.len() > MAX_PROMPT_BYTES {
        return Err(AppError::Config(format!(
            "Messages must be at most {} KB.",
            MAX_PROMPT_BYTES / 1024
        )));
    }
    if let Some(id) = session_id {
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
        {
            return Err(AppError::Config("Hermes session ID is invalid.".into()));
        }
    }

    let mut command = String::from("hermes chat --format stream-json --source cli");
    if let Some(id) = session_id {
        command.push_str(&format!(" --resume {}", shell_escape(id)));
    }
    command.push_str(&format!(" --query {}", shell_escape(prompt)));
    Ok(crate::hermes::env::wrap(cfg, &format!("exec {command}")))
}

fn parse_stream_line(line: &str) -> Option<HermesChatEvent> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let kind = value.get("type")?.as_str()?;
    match kind {
        "system" if value.get("subtype")?.as_str()? == "init" => Some(HermesChatEvent::Session {
            session_id: value.get("session_id")?.as_str()?.to_string(),
        }),
        "text" => Some(HermesChatEvent::Text {
            text: value.get("text")?.as_str()?.to_string(),
        }),
        "tool_use" => Some(HermesChatEvent::ToolUse {
            name: value.get("name")?.as_str()?.to_string(),
        }),
        "tool_result" => Some(HermesChatEvent::ToolResult {
            name: value.get("name")?.as_str()?.to_string(),
            output: value.get("output").and_then(|output| {
                if output.is_string() {
                    output.as_str().map(str::to_string)
                } else if output.is_null() {
                    None
                } else {
                    Some(output.to_string())
                }
            }),
            is_error: value
                .get("is_error")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        }),
        "result" => Some(HermesChatEvent::Complete {
            session_id: value
                .get("session_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            exit_code: value
                .get("exit_code")
                .and_then(serde_json::Value::as_i64)
                .map(|code| code as i32),
            error: value
                .get("error")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        }),
        _ => None,
    }
}

#[tauri::command]
pub async fn start_hermes_chat(
    state: State<'_, AppState>,
    serial: String,
    prompt: String,
    session_id: Option<String>,
    on_event: Channel<HermesChatEvent>,
) -> Result<StreamId, AppError> {
    let cfg = state.config.read().await.hermes.clone();
    let command = build_chat_command(&cfg, &prompt, session_id.as_deref())?;
    let transport: TermuxSshTransport = state.termux_transport(&serial).await?;
    let (id, cancel) = state.streams.register(&serial, "chat");
    let mut stream = match transport.stream(&command, cancel.clone()).await {
        Ok(stream) => stream,
        Err(error) => {
            state.streams.finish(&id);
            return Err(error);
        }
    };
    let streams = state.streams.clone();
    let stream_id = id.clone();
    tauri::async_runtime::spawn(async move {
        let mut stderr = String::new();
        let mut session_id = None;
        let mut result_code = None;
        let mut result_error = None;
        let mut exit_code = None;

        while let Some(event) = stream.recv().await {
            match event {
                StreamEvent::Stdout { line } => {
                    if let Some(event) = parse_stream_line(&line) {
                        match &event {
                            HermesChatEvent::Session { session_id: id } => {
                                session_id = Some(id.clone());
                                if on_event.send(event).is_err() {
                                    cancel.cancel();
                                    break;
                                }
                            }
                            HermesChatEvent::Complete {
                                session_id: id,
                                exit_code,
                                error,
                            } => {
                                if id.is_some() {
                                    session_id = id.clone();
                                }
                                result_code = *exit_code;
                                result_error = error.clone();
                            }
                            _ => {
                                if on_event.send(event).is_err() {
                                    cancel.cancel();
                                    break;
                                }
                            }
                        }
                    }
                }
                StreamEvent::Stderr { line } => {
                    if !line.trim().is_empty() && stderr.len() < MAX_STDERR_BYTES {
                        stderr.push_str(&line);
                        stderr.push('\n');
                    }
                }
                StreamEvent::Exit { code, .. } => exit_code = code,
                StreamEvent::Error { error } => {
                    result_error = Some(error.message.clone());
                    if on_event
                        .send(HermesChatEvent::Error {
                            message: error.message,
                        })
                        .is_err()
                    {
                        cancel.cancel();
                        break;
                    }
                }
            }
        }

        let exit_code = result_code.or(exit_code);
        if exit_code.is_some_and(|code| code != 0) {
            let message = result_error
                .clone()
                .filter(|message| !message.trim().is_empty())
                .or_else(|| (!stderr.trim().is_empty()).then(|| stderr.trim().to_string()))
                .unwrap_or_else(|| format!("Hermes chat exited with code {}.", exit_code.unwrap()));
            let _ = on_event.send(HermesChatEvent::Error { message });
        }
        let _ = on_event.send(HermesChatEvent::Complete {
            session_id,
            exit_code,
            error: result_error,
        });
        streams.finish(&stream_id);
    });
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HermesEnvironment;

    #[test]
    fn parses_stream_json_events() {
        assert_eq!(
            parse_stream_line(
                r#"{"type":"system","subtype":"init","session_id":"20261002_120000_a1b2c3"}"#
            ),
            Some(HermesChatEvent::Session {
                session_id: "20261002_120000_a1b2c3".into()
            })
        );
        assert_eq!(
            parse_stream_line(r#"{"type":"text","text":"Hello "}"#),
            Some(HermesChatEvent::Text {
                text: "Hello ".into()
            })
        );
        assert_eq!(
            parse_stream_line(
                r#"{"type":"result","session_id":"s1","exit_code":0,"text":"Hello"}"#
            ),
            Some(HermesChatEvent::Complete {
                session_id: Some("s1".into()),
                exit_code: Some(0),
                error: None,
            })
        );
        assert_eq!(parse_stream_line("not json"), None);
    }

    #[test]
    fn builds_resumable_shell_safe_chat_command() {
        let cfg = HermesConfig {
            environment: HermesEnvironment::Termux,
            path_prepend: vec![],
            ..Default::default()
        };
        let command = build_chat_command(&cfg, "what's $(id)?", Some("session_1-a")).unwrap();
        assert!(command.starts_with("exec hermes chat --format stream-json --source cli"));
        assert!(command.contains("--resume session_1-a"));
        assert!(command.contains(r#"'what'\''s $(id)?'"#));
    }

    #[test]
    fn rejects_empty_prompts_and_invalid_session_ids() {
        let cfg = HermesConfig::default();
        assert!(build_chat_command(&cfg, "  ", None).is_err());
        assert!(build_chat_command(&cfg, "hello", Some("bad id; rm")).is_err());
    }
}
