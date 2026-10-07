use std::time::Duration;

use base64::Engine;
use rand::random;
use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use crate::commands::hermes::{action_command, HermesAction};
use crate::config::{HermesConfig, HermesEnvironment};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::transport::{CommandResult, DeviceTransport};

const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const SERVICE_FILES: [(&str, &str); 7] = [
    (
        "pyproject.toml",
        include_str!("../../../android/hermes-control/pyproject.toml"),
    ),
    (
        "install.sh",
        include_str!("../../../android/hermes-control/install.sh"),
    ),
    (
        "hermes-control.toml",
        include_str!("../../../android/hermes-control/hermes-control.toml"),
    ),
    (
        "src/hermes_control/__init__.py",
        include_str!("../../../android/hermes-control/src/hermes_control/__init__.py"),
    ),
    (
        "src/hermes_control/__main__.py",
        include_str!("../../../android/hermes-control/src/hermes_control/__main__.py"),
    ),
    (
        "src/hermes_control/config.py",
        include_str!("../../../android/hermes-control/src/hermes_control/config.py"),
    ),
    (
        "src/hermes_control/server.py",
        include_str!("../../../android/hermes-control/src/hermes_control/server.py"),
    ),
];
const SERVICE_RUN: &str =
    include_str!("../../../android/hermes-control/service/hermes-control/run");

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ControlApiInstallPreview {
    pub installed_version: Option<String>,
    pub target_version: String,
    pub files_to_update: Vec<String>,
}

#[tauri::command]
pub async fn preview_control_api_install(
    state: State<'_, AppState>,
    serial: String,
) -> AppResult<ControlApiInstallPreview> {
    let transport = state.termux_transport(&serial).await?;
    let version_command = concat!(
        "\"$HOME/.local/share/hermes-control/venv/bin/python\" ",
        "-c 'from hermes_control import __version__; print(__version__)' 2>/dev/null"
    );
    let result = transport
        .execute(version_command, Duration::from_secs(10))
        .await?;
    let installed_version = (result.exit_code == Some(0))
        .then(|| result.stdout.trim().to_string())
        .filter(|version| !version.is_empty());
    Ok(ControlApiInstallPreview {
        installed_version,
        target_version: SERVICE_VERSION.into(),
        files_to_update: SERVICE_FILES
            .iter()
            .map(|(path, _)| (*path).to_string())
            .chain(std::iter::once("service/hermes-control/run".into()))
            .chain(std::iter::once(
                "~/.config/hermes-control-actions.json".into(),
            ))
            .collect(),
    })
}

fn new_token() -> String {
    random::<[u8; 32]>()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn action_overlay(config: &HermesConfig) -> serde_json::Value {
    let (environment, distro) = match &config.environment {
        HermesEnvironment::Termux => ("termux", ""),
        HermesEnvironment::ProotDistro { distro } => ("proot-distro", distro.as_str()),
    };
    serde_json::json!({
        "actions": {
            "start": action_command(config, HermesAction::Start),
            "stop": action_command(config, HermesAction::Stop),
            "restart": action_command(config, HermesAction::Restart),
        },
        "hermes": {
            "process_match": config.process_match,
            "gateway_match": config.gateway_match,
            "environment": environment,
            "distro": distro,
        },
        "logs": {
            "path": config.log_files.first().map(String::as_str).unwrap_or("/root/.hermes/logs/gateway.log"),
        },
    })
}

fn install_command(token: &str, actions_json: &str) -> String {
    let mut commands = vec![
        "set -eu".to_string(),
        "STAGE=\"$HOME/.cache/hacc-control-api\"".to_string(),
        "mkdir -p \"$STAGE/src/hermes_control\" \"$STAGE/service/hermes-control\"".to_string(),
    ];
    for (path, contents) in SERVICE_FILES
        .iter()
        .copied()
        .chain(std::iter::once(("service/hermes-control/run", SERVICE_RUN)))
    {
        let encoded = base64::engine::general_purpose::STANDARD.encode(contents);
        commands.push(format!(
            "printf '%s' '{encoded}' | base64 -d > \"$STAGE/{path}\""
        ));
    }
    commands.extend([
        "chmod +x \"$STAGE/install.sh\"".into(),
        "sh \"$STAGE/install.sh\"".into(),
        "(export SVDIR=\"$PREFIX/var/service\"; sv down hermes-control >/dev/null 2>&1 || true)"
            .into(),
        "mkdir -p \"$HOME/.config\"".into(),
        "umask 077".into(),
        format!("printf '%s' '{token}' > \"$HOME/.config/hermes-control.token\""),
        "chmod 600 \"$HOME/.config/hermes-control.token\"".into(),
        format!(
            "printf '%s' '{}' | base64 -d > \"$HOME/.config/hermes-control-actions.json\"",
            base64::engine::general_purpose::STANDARD.encode(actions_json)
        ),
        "chmod 600 \"$HOME/.config/hermes-control-actions.json\"".into(),
        "export SVDIR=\"$PREFIX/var/service\" LOGDIR=\"$PREFIX/var/log\"".into(),
        "(service-daemon start >/dev/null 2>&1 &)".into(),
        "sv up hermes-control".into(),
    ]);
    commands.join("; ")
}

#[tauri::command]
pub async fn install_control_api(
    state: State<'_, AppState>,
    serial: String,
) -> AppResult<CommandResult> {
    let token = state.api_token(&serial)?.unwrap_or_else(new_token);
    state.set_api_token(&serial, &token)?;
    tracing::info!(%serial, "stored Control API token in Keychain");
    let hermes_config = state.config.read().await.hermes.clone();
    let overlay = action_overlay(&hermes_config);
    let actions_json =
        serde_json::to_string(&overlay).map_err(|error| AppError::Io(error.to_string()))?;
    let transport = state.termux_transport(&serial).await?;
    let result = transport
        .execute(
            &install_command(&token, &actions_json),
            Duration::from_secs(600),
        )
        .await?;
    if result.exit_code != Some(0) {
        return Err(AppError::CommandFailed {
            command: "Install Hermes Control API".into(),
            exit_code: result.exit_code,
            stderr: if result.stderr.is_empty() {
                result.stdout.clone()
            } else {
                result.stderr.clone()
            },
        });
    }
    Ok(result)
}

#[tauri::command]
pub async fn test_control_api(state: State<'_, AppState>, serial: String) -> AppResult<String> {
    state.api_transport(&serial).await?.health().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_upload_encodes_assets_and_keeps_token_out_of_command_output() {
        let token = "aabbcc";
        let command = install_command(token, "{\"actions\":{\"start\":\"start\"}}");
        assert!(command.contains("base64 -d"));
        assert!(command.contains("$HOME/.config/hermes-control.token"));
        assert!(command.contains("hermes-control-actions.json"));
        assert!(command.contains("service-daemon start >/dev/null 2>&1 &"));
        assert!(command.contains("SVDIR=\"$PREFIX/var/service\""));
        assert!(command.contains("sv down hermes-control"));
        assert!(command.contains("sv up hermes-control"));
        assert!(command.contains("src/hermes_control/server.py"));
    }

    #[test]
    fn action_overlay_keeps_process_and_log_settings_in_sync() {
        let config = HermesConfig {
            log_files: vec!["/root/custom/gateway.log".into()],
            ..HermesConfig::default()
        };
        let overlay = action_overlay(&config);
        assert_eq!(overlay["hermes"]["environment"], "proot-distro");
        assert_eq!(overlay["hermes"]["process_match"], config.process_match);
        assert_eq!(overlay["logs"]["path"], "/root/custom/gateway.log");
        assert!(overlay["actions"]["start"]
            .as_str()
            .unwrap()
            .contains("hermes-supervisor.sh"));
    }

    #[test]
    fn generated_token_has_256_bits_of_hex_entropy() {
        let token = new_token();
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|character| character.is_ascii_hexdigit()));
    }
}
