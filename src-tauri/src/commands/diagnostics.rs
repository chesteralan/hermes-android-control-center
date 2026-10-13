use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, Cursor, Write};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tokio::process::Command;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::adb::{AndroidDevice, ConnectionType, DeviceState};
use crate::commands::logs::recent_tail_command;
use crate::config::AppConfig;
use crate::error::AppError;
use crate::state::AppState;
use crate::transport::DeviceTransport;

const APP_LOG_LINE_LIMIT: usize = 1_000;
const HERMES_LOG_LINE_LIMIT: usize = 500;
const LOG_BYTE_LIMIT: usize = 1_048_576;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsMetadata {
    app_version: &'static str,
    operating_system: &'static str,
    operating_system_version: String,
    adb_version: String,
    connected_devices: usize,
}

fn secret_patterns() -> &'static Result<Vec<Regex>, regex::Error> {
    static PATTERNS: OnceLock<Result<Vec<Regex>, regex::Error>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            r#"(?i)(authorization["']?\s*:\s*["']?bearer\s+)[^"'\s,;}\]]+"#,
            r#"(?i)((?:access[_ -]?token|api[_ -]?key|token|password|secret)["']?\s*[:=]\s*)("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|[^\s,;}\]]+)"#,
            r"\b\d{6,12}:[A-Za-z0-9_-]{25,}\b",
            r"\bAIza[0-9A-Za-z_-]{30,}\b",
            r"\bsk-[A-Za-z0-9_-]{20,}\b",
        ]
        .into_iter()
        .map(Regex::new)
        .collect()
    })
}

fn redact_text(text: &str, redact_ips: bool) -> Result<String, regex::Error> {
    let mut redacted = text.to_string();
    for (index, pattern) in secret_patterns()
        .as_ref()
        .map_err(Clone::clone)?
        .iter()
        .enumerate()
    {
        redacted = match index {
            0 => pattern.replace_all(&redacted, "$1[REDACTED]").into_owned(),
            1 => pattern
                .replace_all(&redacted, |captures: &regex::Captures<'_>| {
                    let value = &captures[2];
                    let replacement = if value.starts_with('"') {
                        "\"[REDACTED]\""
                    } else if value.starts_with('\'') {
                        "'[REDACTED]'"
                    } else {
                        "[REDACTED]"
                    };
                    format!("{}{replacement}", &captures[1])
                })
                .into_owned(),
            _ => pattern.replace_all(&redacted, "[REDACTED]").into_owned(),
        };
    }
    if redact_ips {
        static IPV4: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
        let pattern = IPV4.get_or_init(|| {
            Regex::new(r"\b(?:(?:25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])\.){3}(?:25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])(?::[0-9]{1,5})?\b")
        });
        redacted = pattern
            .as_ref()
            .map_err(Clone::clone)?
            .replace_all(&redacted, "[IP REDACTED]")
            .into_owned();
    }
    Ok(redacted)
}

fn scrub_json(value: Value, redact_ips: bool) -> Result<Value, regex::Error> {
    match value {
        Value::Object(values) => values
            .into_iter()
            .map(|(key, value)| {
                let normalized_key = key.to_ascii_lowercase();
                let is_secret = ["token", "secret", "password", "apikey", "api_key"]
                    .iter()
                    .any(|needle| normalized_key.contains(needle));
                let value = if is_secret {
                    Value::String("[REDACTED]".into())
                } else if normalized_key == "adbpath" {
                    Value::String("[REDACTED PATH]".into())
                } else if redact_ips && matches!(normalized_key.as_str(), "address" | "ipaddress") {
                    Value::String("[IP REDACTED]".into())
                } else {
                    scrub_json(value, redact_ips)?
                };
                Ok((key, value))
            })
            .collect::<Result<serde_json::Map<_, _>, regex::Error>>()
            .map(Value::Object),
        Value::Array(values) => values
            .into_iter()
            .map(|value| scrub_json(value, redact_ips))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::String(value) => redact_text(&value, redact_ips).map(Value::String),
        other => Ok(other),
    }
}

fn redact_device_identifiers(text: &str, devices: &[AndroidDevice], redact_ips: bool) -> String {
    let mut redacted = text.to_string();
    for device in devices {
        if device.connection != ConnectionType::WirelessIp || redact_ips {
            redacted = redacted.replace(&device.serial, "[DEVICE REDACTED]");
        }
        if let Some(device_id) = &device.device_id {
            redacted = redacted.replace(device_id, "[DEVICE REDACTED]");
        }
    }
    redacted
}

fn bounded_text(mut text: String) -> String {
    if text.len() > LOG_BYTE_LIMIT {
        let mut end = LOG_BYTE_LIMIT;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("\n[truncated at 1 MiB]");
    }
    text
}

fn recent_app_logs(log_dir: &Path) -> (Vec<String>, Vec<String>) {
    let mut paths = match std::fs::read_dir(log_dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("hacc") && name.ends_with(".log"))
            })
            .collect::<Vec<_>>(),
        Err(error) => return (Vec::new(), vec![format!("App logs unavailable: {error}")]),
    };
    paths.sort();

    let mut lines = VecDeque::with_capacity(APP_LOG_LINE_LIMIT);
    let mut errors = Vec::new();
    for path in paths {
        match File::open(&path).map(BufReader::new) {
            Ok(reader) => {
                for line in reader.lines() {
                    match line {
                        Ok(line) => {
                            if lines.len() == APP_LOG_LINE_LIMIT {
                                lines.pop_front();
                            }
                            lines.push_back(line);
                        }
                        Err(error) => errors.push(format!("Could not read app log line: {error}")),
                    }
                }
            }
            Err(error) => errors.push(format!("Could not open {}: {error}", path.display())),
        }
    }
    (lines.into_iter().collect(), errors)
}

fn write_zip_file(
    archive: &mut ZipWriter<Cursor<Vec<u8>>>,
    name: &str,
    content: &[u8],
) -> Result<(), AppError> {
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    archive
        .start_file(name, options)
        .map_err(|error| AppError::Io(error.to_string()))?;
    archive
        .write_all(content)
        .map_err(|error| AppError::Io(error.to_string()))
}

async fn host_os_version() -> String {
    #[cfg(target_os = "macos")]
    let command = ("sw_vers", vec!["-productVersion"]);
    #[cfg(target_os = "linux")]
    let command = ("uname", vec!["-r"]);
    #[cfg(target_os = "windows")]
    let command = ("cmd", vec!["/C", "ver"]);
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let command = return "unknown".into();

    match tokio::time::timeout(
        Duration::from_secs(2),
        Command::new(command.0).args(command.1).output(),
    )
    .await
    {
        Ok(Ok(output)) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => "unavailable".into(),
    }
}

async fn adb_version(state: &AppState) -> String {
    match state.adb_client().await {
        Ok(client) => match client.version().await {
            Ok(version) => format!("{version:?}"),
            Err(error) => format!("unavailable: {}", error.user_message()),
        },
        Err(error) => format!("unavailable: {}", error.user_message()),
    }
}

async fn add_device_files(
    archive: &mut ZipWriter<Cursor<Vec<u8>>>,
    state: &AppState,
    config: &AppConfig,
    devices: &[AndroidDevice],
    redact_ips: bool,
) -> Result<usize, AppError> {
    for (index, device) in devices.iter().enumerate() {
        let prefix = format!("devices/device-{}", index + 1);
        let device_json = match state.adb_client().await {
            Ok(client) => match client.device_info(&device.serial).await {
                Ok(mut info) => {
                    info.serial = "[REDACTED]".into();
                    info.device_id = Some("[REDACTED]".into());
                    if redact_ips {
                        info.ip_address = Some("[IP REDACTED]".into());
                    }
                    serde_json::to_vec_pretty(&info)
                        .map_err(|error| AppError::Io(error.to_string()))?
                }
                Err(error) => {
                    let message = redact_device_identifiers(
                        &error.user_message(),
                        std::slice::from_ref(device),
                        redact_ips,
                    );
                    serde_json::to_vec_pretty(&serde_json::json!({ "error": message }))
                        .map_err(|error| AppError::Io(error.to_string()))?
                }
            },
            Err(error) => serde_json::to_vec_pretty(&serde_json::json!({
                "error": error.user_message()
            }))
            .map_err(|error| AppError::Io(error.to_string()))?,
        };
        write_zip_file(archive, &format!("{prefix}.json"), &device_json)?;

        let log_result = async {
            let path = config
                .hermes
                .log_files
                .first()
                .filter(|path| !path.trim().is_empty())
                .ok_or_else(|| {
                    AppError::Config("Hermes gateway log path is not configured.".into())
                })?;
            let transport: std::sync::Arc<dyn DeviceTransport> =
                std::sync::Arc::new(state.termux_transport(&device.serial).await?);
            let command =
                recent_tail_command(path, &config.hermes.environment, HERMES_LOG_LINE_LIMIT);
            let result = transport.execute(&command, Duration::from_secs(8)).await?;
            if result.exit_code.is_some_and(|code| code != 0) {
                return Err(AppError::Io(if result.stderr.trim().is_empty() {
                    "Hermes log command returned a non-zero exit code.".into()
                } else {
                    result.stderr
                }));
            }
            Ok::<String, AppError>(result.stdout)
        }
        .await;

        let log_text = match log_result {
            Ok(text) => text,
            Err(error) => format!("Hermes logs unavailable: {}", error.user_message()),
        };
        let log_text =
            redact_device_identifiers(&log_text, std::slice::from_ref(device), redact_ips);
        let log_text = bounded_text(
            redact_text(&log_text, redact_ips).map_err(|error| AppError::Io(error.to_string()))?,
        );
        write_zip_file(
            archive,
            &format!("{prefix}-hermes-gateway.log"),
            log_text.as_bytes(),
        )?;
    }
    Ok(devices.len())
}

#[tauri::command]
pub async fn export_diagnostics(
    app: AppHandle,
    state: State<'_, AppState>,
    redact_ips: bool,
) -> Result<Option<String>, AppError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("ZIP archives", &["zip"])
        .set_file_name("hermes-control-center-diagnostics.zip")
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

    let config = state.config.read().await.clone();
    let devices = state
        .devices
        .list()
        .into_iter()
        .filter(|device| device.state == DeviceState::Device)
        .collect::<Vec<_>>();
    let settings = scrub_json(
        serde_json::to_value(config.clone()).map_err(|error| AppError::Io(error.to_string()))?,
        redact_ips,
    )
    .map_err(|error| AppError::Io(error.to_string()))?;
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    let settings_json =
        serde_json::to_vec_pretty(&settings).map_err(|error| AppError::Io(error.to_string()))?;
    write_zip_file(&mut archive, "settings-redacted.json", &settings_json)?;

    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|error| AppError::Io(error.to_string()))?;
    let (lines, log_errors) = recent_app_logs(&log_dir);
    let log_text = redact_device_identifiers(&lines.join("\n"), &devices, redact_ips);
    let log_text = bounded_text(
        redact_text(&log_text, redact_ips).map_err(|error| AppError::Io(error.to_string()))?,
    );
    write_zip_file(&mut archive, "app-last-1000-lines.log", log_text.as_bytes())?;

    let connected_devices =
        add_device_files(&mut archive, &state, &config, &devices, redact_ips).await?;
    let metadata = DiagnosticsMetadata {
        app_version: env!("CARGO_PKG_VERSION"),
        operating_system: std::env::consts::OS,
        operating_system_version: host_os_version().await,
        adb_version: adb_version(&state).await,
        connected_devices,
    };
    let metadata =
        serde_json::to_vec_pretty(&metadata).map_err(|error| AppError::Io(error.to_string()))?;
    write_zip_file(&mut archive, "system-info.json", &metadata)?;

    if !log_errors.is_empty() {
        let errors = redact_text(&log_errors.join("\n"), redact_ips)
            .map_err(|error| AppError::Io(error.to_string()))?;
        write_zip_file(&mut archive, "collection-notes.txt", errors.as_bytes())?;
    }

    let bytes = archive
        .finish()
        .map_err(|error| AppError::Io(error.to_string()))?
        .into_inner();
    Ok(Some(super::write_export_file(&path, &bytes)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn redacts_common_tokens_and_optional_ipv4_addresses() {
        let source = "Authorization: Bearer abcdef123456 token=topsecret 192.0.2.12:5555";
        let redacted = redact_text(source, true).unwrap();
        assert!(!redacted.contains("abcdef123456"));
        assert!(!redacted.contains("topsecret"));
        assert!(!redacted.contains("192.0.2.12"));

        let unmasked_ip = redact_text(source, false).unwrap();
        assert!(unmasked_ip.contains("192.0.2.12:5555"));
        assert!(!unmasked_ip.contains("topsecret"));
    }

    #[test]
    fn redacts_quoted_json_secrets_without_corrupting_the_payload() {
        let source = serde_json::json!({
            "api_key": "json-secret-value",
            "password": "secret with spaces and \"quotes\"",
            "token": "escaped\\secret",
            "Authorization": "Bearer header-secret-value",
            "message": "ordinary log text"
        })
        .to_string();
        let redacted = redact_text(&source, false).unwrap();
        let payload: Value = serde_json::from_str(&redacted).unwrap();
        assert_eq!(payload["api_key"], "[REDACTED]");
        assert_eq!(payload["password"], "[REDACTED]");
        assert_eq!(payload["token"], "[REDACTED]");
        assert_eq!(payload["Authorization"], "Bearer [REDACTED]");
        assert_eq!(payload["message"], "ordinary log text");
    }

    #[test]
    fn redacts_complete_quoted_values_in_prefixed_log_lines() {
        let source = r#"DEBUG payload={"api_key":"json-secret"} password='secret with spaces' token="escaped \"quote\" secret" message=visible"#;
        let redacted = redact_text(source, false).unwrap();
        assert_eq!(
            redacted,
            r#"DEBUG payload={"api_key":"[REDACTED]"} password='[REDACTED]' token="[REDACTED]" message=visible"#,
        );
    }

    #[test]
    fn exported_zip_entries_remove_synthetic_secrets_and_respect_ip_consent() {
        for redact_ips in [false, true] {
            let source = r#"{"api_key":"archive-secret", "password":"archive password", "address":"192.0.2.12:5555", "message":"ordinary text"}"#;
            let log = redact_text(source, redact_ips).unwrap();
            let settings = scrub_json(
                serde_json::json!({
                    "apiToken": "settings-secret",
                    "adbPath": "/Users/example/custom-adb",
                    "knownAddresses": [{ "address": "192.0.2.12:5555" }]
                }),
                redact_ips,
            )
            .unwrap();
            let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
            write_zip_file(&mut writer, "app-last-1000-lines.log", log.as_bytes()).unwrap();
            write_zip_file(
                &mut writer,
                "settings-redacted.json",
                &serde_json::to_vec(&settings).unwrap(),
            )
            .unwrap();
            let bytes = writer.finish().unwrap().into_inner();
            let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
            for name in ["app-last-1000-lines.log", "settings-redacted.json"] {
                let mut contents = String::new();
                archive
                    .by_name(name)
                    .unwrap()
                    .read_to_string(&mut contents)
                    .unwrap();
                assert!(!contents.contains("archive-secret"));
                assert!(!contents.contains("archive password"));
                assert!(!contents.contains("settings-secret"));
                assert!(!contents.contains("/Users/example/custom-adb"));
                assert_eq!(contents.contains("192.0.2.12:5555"), !redact_ips);
                let _: Value = serde_json::from_str(&contents).unwrap();
            }
        }
    }

    #[test]
    fn recursively_redacts_secret_settings_keys() {
        let value = serde_json::json!({
            "knownAddresses": [{ "address": "phone.local:5555" }],
            "adbPath": "/Users/alice/tools/adb",
            "apiToken": "secret-value"
        });
        let redacted = scrub_json(value, true).unwrap();
        assert_eq!(redacted["apiToken"], "[REDACTED]");
        assert_eq!(redacted["knownAddresses"][0]["address"], "[IP REDACTED]");
        assert_eq!(redacted["adbPath"], "[REDACTED PATH]");
    }

    #[test]
    fn redacts_device_serials_and_respects_optional_ip_redaction() {
        let devices =
            crate::adb::parse::parse_devices("USB-SERIAL-123 device\n192.0.2.10:5555 device\n");
        let source = "USB-SERIAL-123 at 192.0.2.10:5555";
        let private = redact_device_identifiers(source, &devices, false);
        assert!(!private.contains("USB-SERIAL-123"));
        assert!(private.contains("192.0.2.10:5555"));

        let redacted = redact_device_identifiers(source, &devices, true);
        assert!(!redacted.contains("USB-SERIAL-123"));
        assert!(!redacted.contains("192.0.2.10:5555"));
    }

    #[test]
    fn app_logs_keep_only_the_last_one_thousand_lines() {
        let directory = std::env::temp_dir().join(format!(
            "hacc-diagnostics-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let contents = (0..1_005)
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(directory.join("hacc.2026-10-07.log"), contents).unwrap();
        let (lines, errors) = recent_app_logs(&directory);
        assert!(errors.is_empty());
        assert_eq!(lines.len(), APP_LOG_LINE_LIMIT);
        assert_eq!(lines.first().map(String::as_str), Some("5"));
        assert_eq!(lines.last().map(String::as_str), Some("1004"));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
