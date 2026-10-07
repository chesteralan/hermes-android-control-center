use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::config::{AppConfig, HermesEnvironment, HermesTransportKind};
use crate::error::AppError;
use crate::logs::api_ws::ApiWsLogSource;
use crate::logs::logcat::LogcatSource;
use crate::logs::transport_command::TransportCommandLogSource;
use crate::logs::{batcher, LogLine, LogSource, LogSourceKind};
use crate::state::AppState;
use crate::streams::StreamId;
use crate::termux::shell_escape;
use crate::transport::DeviceTransport;

fn path_argument(path: &str, environment: &HermesEnvironment) -> String {
    match environment {
        HermesEnvironment::Termux => match path.strip_prefix("~/") {
            Some(relative) => format!("\"$HOME\"{}", shell_escape(&format!("/{relative}"))),
            None => shell_escape(path),
        },
        HermesEnvironment::ProotDistro { .. } => {
            let guest_path = path
                .strip_prefix("~/")
                .map(|relative| format!("/root/{relative}"))
                .unwrap_or_else(|| path.to_string());
            format!("\"$R\"{}", shell_escape(&guest_path))
        }
    }
}

pub(crate) fn recent_tail_command(
    path: &str,
    environment: &HermesEnvironment,
    line_count: usize,
) -> String {
    let command = format!(
        "tail -n {line_count} -- {}",
        path_argument(path, environment)
    );
    match environment {
        HermesEnvironment::Termux => command,
        HermesEnvironment::ProotDistro { .. } => {
            format!("{}; {command}", crate::hermes::env::root_expr(environment))
        }
    }
}

fn tail_command(path: &str, environment: &HermesEnvironment) -> String {
    let command = format!("tail -n 200 -F -- {}", path_argument(path, environment));
    match environment {
        HermesEnvironment::Termux => command,
        HermesEnvironment::ProotDistro { .. } => {
            format!("{}; {command}", crate::hermes::env::root_expr(environment))
        }
    }
}

fn command_for_source(source: LogSourceKind, config: &AppConfig) -> Result<String, AppError> {
    config.validate()?;
    let log_index = match source {
        LogSourceKind::HermesGateway => 0,
        LogSourceKind::HermesToolCalls => 1,
        LogSourceKind::Supervisor => {
            return Ok(tail_command(
                "~/.hacc/supervisor.log",
                &HermesEnvironment::Termux,
            ))
        }
        LogSourceKind::Logcat => {
            return Err(AppError::Config(
                "Android logcat does not use a Termux command.".into(),
            ));
        }
    };
    let path = config
        .hermes
        .log_files
        .get(log_index)
        .filter(|path| !path.trim().is_empty())
        .ok_or_else(|| {
            AppError::Config("Configure the Hermes log file path in Settings.".into())
        })?;
    Ok(tail_command(path, &config.hermes.environment))
}

fn format_log_export(lines: &[LogLine], format: &str) -> Result<(&'static str, String), AppError> {
    match format {
        "log" => Ok((
            "hermes-logs.log",
            lines
                .iter()
                .map(|line| line.raw.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        )),
        "jsonl" => {
            let json_lines = lines
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| AppError::Io(error.to_string()))?;
            Ok(("hermes-logs.jsonl", json_lines.join("\n")))
        }
        _ => Err(AppError::Config(
            "Choose a .log or .jsonl export format.".into(),
        )),
    }
}

#[tauri::command]
pub async fn export_logs(
    app: AppHandle,
    lines: Vec<LogLine>,
    format: String,
) -> Result<Option<String>, AppError> {
    let (file_name, content) = format_log_export(&lines, &format)?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("Log files", &[format.as_str()])
        .set_file_name(file_name)
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
    std::fs::write(&path, content).map_err(|error| AppError::Io(error.to_string()))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// Starts a persistent log stream; lines arrive in batches (≤50 ms / ≤500 lines).
#[tauri::command]
pub async fn start_log_stream(
    state: State<'_, AppState>,
    serial: String,
    source: LogSourceKind,
    on_batch: Channel<Vec<LogLine>>,
) -> Result<StreamId, AppError> {
    let config = state.config.read().await.clone();
    let src: Box<dyn LogSource> = match source {
        LogSourceKind::Logcat => {
            let client = state.adb_client().await?;
            Box::new(LogcatSource::new(
                client,
                state.runner.clone(),
                &serial,
                &config.logs.logcat_filter,
            ))
        }
        LogSourceKind::HermesGateway
        | LogSourceKind::HermesToolCalls
        | LogSourceKind::Supervisor => {
            let api_source = if config.hermes.transport == HermesTransportKind::ControlApi
                && source == LogSourceKind::HermesGateway
            {
                let probe = async {
                    let api = Arc::new(state.api_transport(&serial).await?);
                    api.health().await?;
                    api.check_log_stream().await?;
                    Ok::<_, AppError>(api)
                }
                .await;
                match probe {
                    Ok(api) => Some(Box::new(ApiWsLogSource::new(api)) as Box<dyn LogSource>),
                    Err(error) if config.hermes.fallback_to_ssh => {
                        tracing::warn!(%error, "Control API log startup failed; using enabled SSH fallback");
                        None
                    }
                    Err(error) => return Err(error),
                }
            } else {
                None
            };
            if let Some(source) = api_source {
                source
            } else {
                if config.hermes.transport == HermesTransportKind::ControlApi
                    && !config.hermes.fallback_to_ssh
                {
                    return Err(AppError::Config(
                        "The Control API streams gateway logs only. Enable SSH fallback for this log source.".into(),
                    ));
                }
                let command = command_for_source(source, &config)?;
                let transport: Arc<dyn DeviceTransport> =
                    Arc::new(state.termux_transport(&serial).await?);
                Box::new(TransportCommandLogSource::new(transport, command))
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_hermes_log_commands_for_proot_and_termux_paths() {
        let config = AppConfig::default();
        let gateway = command_for_source(LogSourceKind::HermesGateway, &config).unwrap();
        let tool_calls = command_for_source(LogSourceKind::HermesToolCalls, &config).unwrap();
        assert!(gateway.contains("containers/debian/rootfs"));
        assert!(gateway.contains("\"$R\"/root/.hermes/logs/gateway.log"));
        assert!(tool_calls.contains("\"$R\"/root/.hermes/logs/tool_calls.log"));

        let supervisor = command_for_source(LogSourceKind::Supervisor, &config).unwrap();
        assert_eq!(
            supervisor,
            "tail -n 200 -F -- \"$HOME\"/.hacc/supervisor.log"
        );

        let termux_config = AppConfig {
            hermes: crate::config::HermesConfig {
                environment: HermesEnvironment::Termux,
                log_files: vec!["~/.hermes/logs/gateway.log".into()],
                ..config.hermes.clone()
            },
            ..config
        };
        let termux_gateway =
            command_for_source(LogSourceKind::HermesGateway, &termux_config).unwrap();
        assert_eq!(
            termux_gateway,
            "tail -n 200 -F -- \"$HOME\"/.hermes/logs/gateway.log"
        );
    }

    #[test]
    fn rejects_logcat_as_a_termux_file_command() {
        assert!(command_for_source(LogSourceKind::Logcat, &AppConfig::default()).is_err());
    }

    #[test]
    fn rejects_invalid_distro_before_building_a_remote_path() {
        let config = AppConfig {
            hermes: crate::config::HermesConfig {
                environment: HermesEnvironment::ProotDistro {
                    distro: "debian; touch /tmp/pwned".into(),
                },
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(command_for_source(LogSourceKind::HermesGateway, &config).is_err());
    }

    fn sample_lines() -> Vec<LogLine> {
        vec![
            LogLine {
                seq: 1,
                received_at: 100,
                timestamp: Some("10-05 12:00:00.000".into()),
                level: Some(crate::logs::LogLevel::Warn),
                tag: Some("Hermes".into()),
                message: "raw warning".into(),
                raw: "10-05 12:00:00.000 W/Hermes: raw warning".into(),
            },
            LogLine {
                seq: 2,
                received_at: 101,
                timestamp: None,
                level: None,
                tag: None,
                message: "unstructured".into(),
                raw: "unstructured".into(),
            },
        ]
    }

    #[test]
    fn exports_raw_and_jsonl_log_lines() {
        let lines = sample_lines();
        let (raw_name, raw) = format_log_export(&lines, "log").unwrap();
        assert_eq!(raw_name, "hermes-logs.log");
        assert_eq!(
            raw,
            "10-05 12:00:00.000 W/Hermes: raw warning\nunstructured"
        );

        let (jsonl_name, jsonl) = format_log_export(&lines, "jsonl").unwrap();
        let records = jsonl
            .lines()
            .map(serde_json::from_str::<serde_json::Value>)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(jsonl_name, "hermes-logs.jsonl");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["raw"], lines[0].raw);
        assert_eq!(records[1]["message"], "unstructured");
        assert!(format_log_export(&lines, "csv").is_err());
    }
}
