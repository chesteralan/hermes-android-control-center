use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::Mutex;
use ts_rs::TS;

use crate::config::{HermesConfig, HermesTransportKind, StartMode};
use crate::error::AppError;
use crate::hermes::detect::{self, HermesInstallReport};
use crate::hermes::status::{self, ComponentStatus, HermesStatus};
use crate::hermes::supervisor;
use crate::state::AppState;
use crate::transport::api::ApiTransport;
use crate::transport::{CommandResult, DeviceTransport};

const STATUS_TIMEOUT: Duration = Duration::from_secs(15);
const ACTION_TIMEOUT: Duration = Duration::from_secs(30);
const UPDATE_TIMEOUT: Duration = Duration::from_secs(600);
const CONFIRM_DELAYS: [u64; 4] = [1, 2, 3, 5];

/// Serializes Start/Stop/Restart per device.
#[derive(Default)]
pub struct ActionLocks(Mutex<HashMap<String, Arc<Mutex<()>>>>);

impl ActionLocks {
    async fn for_device(&self, serial: &str) -> Arc<Mutex<()>> {
        self.0
            .lock()
            .await
            .entry(serial.to_string())
            .or_default()
            .clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum HermesAction {
    Start,
    Stop,
    Restart,
    RestartNow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum HermesTool {
    Doctor,
    Update,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HermesActionResult {
    pub output: CommandResult,
    pub status: HermesStatus,
    /// Whether the status reached the expected state before we stopped waiting.
    pub confirmed: bool,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

async fn fetch_status(
    state: &AppState,
    serial: &str,
    cfg: &HermesConfig,
) -> Result<HermesStatus, AppError> {
    if cfg.transport == crate::config::HermesTransportKind::ControlApi {
        match state.api_transport(serial).await {
            Ok(api) => match api.status().await {
                Ok(status) => return Ok(status),
                Err(error) if cfg.fallback_to_ssh => {
                    tracing::warn!(%error, "Control API status failed; using enabled SSH fallback");
                }
                Err(error) => return Err(error),
            },
            Err(error) if cfg.fallback_to_ssh => {
                tracing::warn!(%error, "Control API setup failed; using enabled SSH fallback");
            }
            Err(error) => return Err(error),
        }
    }
    match state.termux_transport(serial).await {
        Ok(t) => {
            let out = t
                .execute(&status::probe_script(cfg), STATUS_TIMEOUT)
                .await?;
            let mut parsed = status::parse_status(cfg, &out.stdout, now());
            if !cfg.status_command.trim().is_empty() {
                let command = crate::hermes::env::wrap(cfg, &cfg.status_command);
                match t.execute(&command, STATUS_TIMEOUT).await {
                    Ok(result) => {
                        let text = if result.stdout.trim().is_empty() {
                            result.stderr
                        } else {
                            result.stdout
                        };
                        parsed.raw_status_output = (!text.trim().is_empty()).then_some(text);
                    }
                    Err(e) => {
                        tracing::debug!(error = %e, "configured Hermes status command failed");
                        parsed.raw_status_output = Some(e.user_message());
                    }
                }
            }
            Ok(parsed)
        }
        Err(bridge_err) => {
            tracing::debug!(error = %bridge_err, "hermes status: falling back to adb");
            let client = state.adb_client().await?;
            let out = client
                .shell(serial, &status::adb_probe_script(cfg), STATUS_TIMEOUT)
                .await?;
            Ok(status::parse_adb_processes(cfg, &out.stdout, now()))
        }
    }
}

async fn action_transport(
    state: &AppState,
    serial: &str,
    cfg: &HermesConfig,
) -> Result<(Arc<dyn DeviceTransport>, Option<ApiTransport>), AppError> {
    if cfg.transport == HermesTransportKind::ControlApi {
        let api = match state.api_transport(serial).await {
            Ok(api) => api,
            Err(error) if cfg.fallback_to_ssh => {
                tracing::warn!(%error, "Control API setup failed; using enabled SSH fallback");
                return Ok((Arc::new(state.termux_transport(serial).await?), None));
            }
            Err(error) => return Err(error),
        };
        match api.health().await {
            Ok(_) => return Ok((Arc::new(api.clone()), Some(api))),
            Err(error) if cfg.fallback_to_ssh => {
                tracing::warn!(%error, "Control API health check failed; using enabled SSH fallback");
                return Ok((Arc::new(state.termux_transport(serial).await?), None));
            }
            Err(error) => return Err(error),
        }
    }
    Ok((Arc::new(state.termux_transport(serial).await?), None))
}

#[tauri::command]
pub async fn get_hermes_status(
    state: State<'_, AppState>,
    serial: String,
) -> Result<HermesStatus, AppError> {
    let cfg = state.config.read().await.hermes.clone();
    fetch_status(&state, &serial, &cfg).await
}

#[tauri::command]
pub async fn detect_hermes(
    state: State<'_, AppState>,
    serial: String,
) -> Result<HermesInstallReport, AppError> {
    let t = state.termux_transport(&serial).await?;
    let out = t.execute(detect::SCRIPT, STATUS_TIMEOUT).await?;
    let mut report = detect::parse(&out.stdout);
    let base = state.config.read().await.hermes.clone();
    for candidate in &mut report.candidates {
        let mut candidate_cfg = base.clone();
        candidate_cfg.environment = candidate.environment.clone();
        candidate_cfg.path_prepend = vec![candidate.bin_dir.clone()];
        candidate_cfg.hermes_home = match &candidate.environment {
            crate::config::HermesEnvironment::Termux => "~/.hermes".into(),
            crate::config::HermesEnvironment::ProotDistro { .. } => "/root/.hermes".into(),
        };
        let command = candidate_cfg.version_command.trim();
        if !command.is_empty() {
            let wrapped = crate::hermes::env::wrap(&candidate_cfg, command);
            match t.execute(&wrapped, Duration::from_secs(45)).await {
                Ok(version) if version.exit_code == Some(0) => {
                    candidate.version = detect::first_version_line(&version.stdout);
                }
                Ok(_) => {}
                Err(e) => tracing::debug!(error = %e, "Hermes version probe unavailable"),
            }
        }
    }
    Ok(report)
}

pub(crate) fn action_command(cfg: &HermesConfig, action: HermesAction) -> String {
    let custom = match action {
        HermesAction::Start => &cfg.start_command,
        HermesAction::Stop => &cfg.stop_command,
        HermesAction::Restart | HermesAction::RestartNow => &cfg.restart_command,
    };
    if !custom.trim().is_empty() {
        return crate::hermes::env::wrap(cfg, custom);
    }
    match (action, cfg.start_mode) {
        (HermesAction::Start, StartMode::Supervised) => supervisor::start_command(cfg),
        (HermesAction::Start, StartMode::Detached) => supervisor::start_detached_command(cfg),
        (HermesAction::Stop, _) => supervisor::stop_command(cfg),
        (HermesAction::Restart, StartMode::Supervised) => supervisor::restart_command(cfg, true),
        (HermesAction::RestartNow, StartMode::Supervised) => {
            supervisor::restart_command(cfg, false)
        }
        (HermesAction::Restart | HermesAction::RestartNow, StartMode::Detached) => {
            supervisor::restart_detached_command(cfg)
        }
    }
}

fn reached(action: HermesAction, before: Option<u32>, s: &HermesStatus) -> bool {
    match action {
        HermesAction::Start => s.gateway == ComponentStatus::Running,
        HermesAction::Stop => s.gateway == ComponentStatus::Stopped,
        HermesAction::Restart | HermesAction::RestartNow => {
            s.gateway == ComponentStatus::Running
                && s.gateway_pid.is_some()
                && s.gateway_pid != before
        }
    }
}

/// Start/Stop/Restart Hermes' gateway, then wait (bounded) for the status to confirm it.
#[tauri::command]
pub async fn hermes_action(
    state: State<'_, AppState>,
    locks: State<'_, ActionLocks>,
    serial: String,
    action: HermesAction,
) -> Result<HermesActionResult, AppError> {
    let lock = locks.for_device(&serial).await;
    let _guard = lock.lock().await;
    let cfg = state.config.read().await.hermes.clone();
    if cfg.transport == crate::config::HermesTransportKind::ControlApi {
        let api = match state.api_transport(&serial).await {
            Ok(api) => api,
            Err(error) if cfg.fallback_to_ssh => {
                tracing::warn!(%error, "Control API setup failed; using enabled SSH action fallback");
                return hermes_action_over_ssh(state, serial, action, cfg).await;
            }
            Err(error) => return Err(error),
        };
        match api.health().await {
            Ok(_) => {
                let before = api
                    .status()
                    .await
                    .ok()
                    .and_then(|status| status.gateway_pid);
                let action_name = match action {
                    HermesAction::Start => "start",
                    HermesAction::Stop => "stop",
                    HermesAction::Restart | HermesAction::RestartNow => "restart",
                };
                let response = api.action(action_name).await?;
                if response.result.exit_code != Some(0) {
                    return Err(AppError::CommandFailed {
                        command: format!("{action:?} Hermes"),
                        exit_code: response.result.exit_code,
                        stderr: response.result.stderr,
                    });
                }
                let output = response.result;
                let mut status = response.status;
                let mut delays = CONFIRM_DELAYS.to_vec();
                if action == HermesAction::Restart {
                    delays.extend([5, 10, 10]);
                }
                for delay in delays {
                    if reached(action, before, &status) {
                        return Ok(HermesActionResult {
                            output,
                            status,
                            confirmed: true,
                        });
                    }
                    tokio::time::sleep(Duration::from_secs(delay)).await;
                    status = fetch_status(&state, &serial, &cfg).await?;
                }
                let confirmed = reached(action, before, &status);
                return Ok(HermesActionResult {
                    output,
                    status,
                    confirmed,
                });
            }
            Err(error) if cfg.fallback_to_ssh => {
                tracing::warn!(%error, "Control API health check failed; using enabled SSH action fallback");
            }
            Err(error) => return Err(error),
        }
    }
    hermes_action_over_ssh(state, serial, action, cfg).await
}

async fn hermes_action_over_ssh(
    state: State<'_, AppState>,
    serial: String,
    action: HermesAction,
    cfg: HermesConfig,
) -> Result<HermesActionResult, AppError> {
    let t = state.termux_transport(&serial).await?;
    let before = fetch_status(&state, &serial, &cfg)
        .await
        .ok()
        .and_then(|s| s.gateway_pid);
    tracing::info!(%serial, ?action, "hermes action");
    execute_action_and_confirm(&t, &cfg, action, before, || {
        fetch_status(&state, &serial, &cfg)
    })
    .await
}

async fn execute_action_and_confirm<Probe, ProbeFuture>(
    transport: &dyn DeviceTransport,
    cfg: &HermesConfig,
    action: HermesAction,
    before: Option<u32>,
    mut probe: Probe,
) -> Result<HermesActionResult, AppError>
where
    Probe: FnMut() -> ProbeFuture,
    ProbeFuture: std::future::Future<Output = Result<HermesStatus, AppError>>,
{
    let output = transport
        .execute(&action_command(cfg, action), ACTION_TIMEOUT)
        .await?;
    if output.exit_code != Some(0) {
        let msg = output.stdout.trim().to_string();
        return Err(AppError::CommandFailed {
            command: format!("{action:?} Hermes"),
            exit_code: output.exit_code,
            stderr: if msg.is_empty() {
                output.stderr
            } else {
                format!("{msg}\n{}", output.stderr)
            },
        });
    }
    // Restart drains in-flight turns first, so allow longer.
    let mut delays: Vec<u64> = CONFIRM_DELAYS.to_vec();
    if action == HermesAction::Restart {
        delays.extend([5, 10, 10]);
    }
    let mut status = probe().await?;
    for d in delays {
        if reached(action, before, &status) {
            return Ok(HermesActionResult {
                output,
                status,
                confirmed: true,
            });
        }
        tokio::time::sleep(Duration::from_secs(d)).await;
        status = probe().await?;
    }
    let confirmed = reached(action, before, &status);
    Ok(HermesActionResult {
        output,
        status,
        confirmed,
    })
}

/// Runs the configured Hermes Doctor or Update command. Update restarts the gateway afterward.
#[tauri::command]
pub async fn run_hermes_tool(
    state: State<'_, AppState>,
    locks: State<'_, ActionLocks>,
    serial: String,
    tool: HermesTool,
) -> Result<CommandResult, AppError> {
    let lock = locks.for_device(&serial).await;
    let _guard = lock.lock().await;
    let cfg = state.config.read().await.hermes.clone();
    let configured = match tool {
        HermesTool::Doctor => &cfg.doctor_command,
        HermesTool::Update => &cfg.update_command,
    };
    if configured.trim().is_empty() {
        return Err(AppError::Config(format!(
            "Hermes {tool:?} command is not configured."
        )));
    }
    let (transport, api_transport) = action_transport(&state, &serial, &cfg).await?;
    let timeout = if tool == HermesTool::Update {
        UPDATE_TIMEOUT
    } else {
        ACTION_TIMEOUT
    };
    let command = crate::hermes::env::wrap(&cfg, configured);
    let mut used_api = api_transport.is_some();
    let mut output = match transport.execute(&command, timeout).await {
        Ok(output) => output,
        Err(AppError::Config(message)) if cfg.fallback_to_ssh && message.contains("HTTP 403:") => {
            tracing::warn!("Control API command execution is disabled; using enabled SSH fallback");
            used_api = false;
            state
                .termux_transport(&serial)
                .await?
                .execute(&command, timeout)
                .await?
        }
        Err(error) => return Err(error),
    };
    if output.exit_code != Some(0) {
        return Err(AppError::CommandFailed {
            command: format!("Hermes {tool:?}"),
            exit_code: output.exit_code,
            stderr: if output.stdout.is_empty() {
                output.stderr
            } else {
                format!("{}\n{}", output.stdout, output.stderr)
            },
        });
    }
    // In supervised mode Hermes update exits the gateway and the supervisor relaunches it.
    if tool == HermesTool::Update && cfg.start_mode == StartMode::Detached {
        let restart = if used_api {
            let api = api_transport.ok_or_else(|| {
                AppError::Config("Control API transport disappeared during update.".into())
            })?;
            api.action("restart").await?.result
        } else {
            state
                .termux_transport(&serial)
                .await?
                .execute(
                    &action_command(&cfg, HermesAction::RestartNow),
                    ACTION_TIMEOUT,
                )
                .await?
        };
        output.stdout.push_str(&restart.stdout);
        output.stderr.push_str(&restart.stderr);
        output.duration_ms += restart.duration_ms;
        if restart.exit_code != Some(0) {
            return Err(AppError::CommandFailed {
                command: "Restart Hermes after update".into(),
                exit_code: restart.exit_code,
                stderr: restart.stderr,
            });
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex as StdMutex;

    struct FakeTransport {
        results: StdMutex<VecDeque<CommandResult>>,
        commands: StdMutex<Vec<(String, Duration)>>,
    }

    impl FakeTransport {
        fn new(results: Vec<CommandResult>) -> Self {
            Self {
                results: StdMutex::new(results.into()),
                commands: StdMutex::new(Vec::new()),
            }
        }

        async fn probe(&self) -> Result<HermesStatus, AppError> {
            let output = self.execute("status", STATUS_TIMEOUT).await?;
            let mut status = status::parse_status(&HermesConfig::default(), "", 0);
            status.gateway_pid = output.stdout.parse().ok();
            status.gateway = if status.gateway_pid.is_some() {
                ComponentStatus::Running
            } else {
                ComponentStatus::Stopped
            };
            Ok(status)
        }
    }

    #[async_trait::async_trait]
    impl DeviceTransport for FakeTransport {
        fn kind(&self) -> crate::transport::TransportKind {
            crate::transport::TransportKind::TermuxSsh
        }

        async fn execute(
            &self,
            command: &str,
            timeout: Duration,
        ) -> Result<CommandResult, AppError> {
            self.commands
                .lock()
                .unwrap()
                .push((command.into(), timeout));
            self.results
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| AppError::Config("Unexpected extra transport call".into()))
        }

        async fn stream(
            &self,
            _command: &str,
            _cancel: tokio_util::sync::CancellationToken,
        ) -> Result<tokio::sync::mpsc::Receiver<crate::transport::StreamEvent>, AppError> {
            Err(AppError::Config("Unexpected stream call".into()))
        }
    }

    fn output(stdout: &str, stderr: &str, exit_code: i32) -> CommandResult {
        CommandResult {
            stdout: stdout.into(),
            stderr: stderr.into(),
            exit_code: Some(exit_code),
            duration_ms: 12,
        }
    }

    #[tokio::test(start_paused = true)]
    async fn actions_execute_then_poll_until_confirmed() {
        for (action, initial, final_pid) in [
            (HermesAction::Start, "", "11"),
            (HermesAction::Stop, "10", ""),
            (HermesAction::Restart, "10", "11"),
            (HermesAction::RestartNow, "10", "11"),
        ] {
            let cfg = HermesConfig::default();
            let transport = FakeTransport::new(vec![
                output("action output", "", 0),
                output(initial, "", 0),
                output(final_pid, "", 0),
            ]);
            let result = execute_action_and_confirm(&transport, &cfg, action, Some(10), || {
                transport.probe()
            })
            .await
            .unwrap();
            assert!(result.confirmed, "{action:?}");
            assert_eq!(result.output.stdout, "action output");
            assert_eq!(result.status.gateway_pid, final_pid.parse().ok());
            assert_eq!(
                *transport.commands.lock().unwrap(),
                vec![
                    (action_command(&cfg, action), ACTION_TIMEOUT),
                    ("status".into(), STATUS_TIMEOUT),
                    ("status".into(), STATUS_TIMEOUT),
                ]
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn restarts_with_unchanged_pid_exhaust_the_bounded_poll_budget() {
        for (action, polls, seconds) in [
            (HermesAction::Restart, 8, 36),
            (HermesAction::RestartNow, 5, 11),
        ] {
            let mut results = vec![output("", "", 0)];
            results.extend((0..polls).map(|_| output("10", "", 0)));
            let transport = FakeTransport::new(results);
            let started = tokio::time::Instant::now();
            let result = execute_action_and_confirm(
                &transport,
                &HermesConfig::default(),
                action,
                Some(10),
                || transport.probe(),
            )
            .await
            .unwrap();
            assert!(!result.confirmed);
            assert_eq!(result.status.gateway_pid, Some(10));
            assert_eq!(started.elapsed(), Duration::from_secs(seconds));
            assert_eq!(transport.commands.lock().unwrap().len(), polls + 1);
        }
    }

    #[tokio::test]
    async fn action_failure_preserves_output_and_skips_confirmation() {
        let transport = FakeTransport::new(vec![output("start refused", "details", 7)]);
        let error = execute_action_and_confirm(
            &transport,
            &HermesConfig::default(),
            HermesAction::Start,
            None,
            || transport.probe(),
        )
        .await
        .unwrap_err();
        match error {
            AppError::CommandFailed {
                exit_code, stderr, ..
            } => {
                assert_eq!(exit_code, Some(7));
                assert_eq!(stderr, "start refused\ndetails");
            }
            other => panic!("Unexpected error: {other:?}"),
        }
        assert_eq!(transport.commands.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn status_probe_failure_is_not_reported_as_confirmation() {
        let transport = FakeTransport::new(vec![output("", "", 0)]);
        let error = execute_action_and_confirm(
            &transport,
            &HermesConfig::default(),
            HermesAction::Start,
            None,
            || async { Err(AppError::Config("status unavailable".into())) },
        )
        .await
        .unwrap_err();
        assert!(matches!(error, AppError::Config(message) if message == "status unavailable"));
        assert_eq!(transport.commands.lock().unwrap().len(), 1);
    }

    #[test]
    fn custom_commands_override_and_are_wrapped() {
        let cfg = HermesConfig {
            stop_command: "pkill -f gateway".into(),
            ..Default::default()
        };
        assert!(action_command(&cfg, HermesAction::Stop)
            .starts_with("proot-distro login debian -- bash -c"));
        assert!(
            action_command(&HermesConfig::default(), HermesAction::Start)
                .contains("hermes-supervisor.sh")
        );
    }

    #[test]
    fn restart_requires_new_pid() {
        let mut s = status::parse_status(&HermesConfig::default(), "", 0);
        s.gateway = ComponentStatus::Running;
        s.gateway_pid = Some(10);
        assert!(!reached(HermesAction::Restart, Some(10), &s));
        s.gateway_pid = Some(11);
        assert!(reached(HermesAction::Restart, Some(10), &s));
    }

    #[test]
    fn detached_restart_does_not_require_supervisor() {
        let cfg = HermesConfig {
            start_mode: StartMode::Detached,
            ..Default::default()
        };
        let command = action_command(&cfg, HermesAction::RestartNow);
        assert!(command.contains("kill -TERM"));
        assert!(command.contains("nohup setsid"));
        assert!(!command.contains("not supervised"));
    }
}
