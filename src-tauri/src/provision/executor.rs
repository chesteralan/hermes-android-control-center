use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::Engine;
use tauri::{AppHandle, Manager, Runtime};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::adb::{ConnectionType, DeviceState};
use crate::commands::hermes::{action_command, HermesAction};
use crate::config::{HermesConfig, HermesEnvironment, HermesTransportKind};
use crate::error::{AppError, AppResult};
use crate::process::RawOutput;
use crate::provision::apk::{
    apply_termux_install_receipt, download_termux_apk, download_termux_boot_apk,
    record_termux_install,
};
use crate::provision::plan::{ProvisionCheck, ProvisionStepExecutor, ProvisionStepRunOutcome};
use crate::provision::recipe::{ProvisionRecipe, ProvisionTermuxSource};
use crate::provision::types::{ProvisionEvent, ProvisionStepId};
use crate::provision::MIN_FREE_BYTES;
use crate::state::AppState;
use crate::termux::{keys, shell_escape};
use crate::transport::{DeviceTransport, StreamEvent};

const BOOTSTRAP_DIR: &str = "/sdcard/Download/hacc";
const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(600);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);
const TERMUX_FOCUS_CHECK: &str =
    "dumpsys window | grep -E 'mCurrentFocus|mFocusedApp' | grep -q 'com.termux/'";

pub struct AndroidProvisionExecutor<R: Runtime> {
    app: AppHandle<R>,
    serial: String,
    recipe: ProvisionRecipe,
}

impl<R: Runtime> AndroidProvisionExecutor<R> {
    pub fn new(app: AppHandle<R>, serial: String, recipe: ProvisionRecipe) -> Self {
        Self {
            app,
            serial,
            recipe,
        }
    }

    async fn adb_shell(&self, command: &str) -> AppResult<RawOutput> {
        let state = self.app.state::<AppState>();
        state
            .adb_client()
            .await?
            .shell(&self.serial, command, COMMAND_TIMEOUT)
            .await
    }

    async fn remote(&self) -> AppResult<crate::termux::ssh::TermuxSshTransport> {
        self.app
            .state::<AppState>()
            .termux_transport(&self.serial)
            .await
    }

    async fn run_remote(
        &self,
        step: ProvisionStepId,
        command: &str,
        timeout: Duration,
        events: &mpsc::Sender<ProvisionEvent>,
    ) -> AppResult<()> {
        let result = self.remote().await?.execute(command, timeout).await?;
        emit_output(step, &result, events).await;
        if result.exit_code.is_some_and(|code| code != 0) {
            return Err(AppError::CommandFailed {
                command: command.to_string(),
                exit_code: result.exit_code,
                stderr: if result.stderr.is_empty() {
                    result.stdout
                } else {
                    result.stderr
                },
            });
        }
        Ok(())
    }

    fn hermes_command(&self, command: &str) -> String {
        match &self.recipe.hermes_install.native_apt_package {
            Some(_) => command.to_string(),
            None => {
                let distro = shell_escape(&self.recipe.distro);
                let inner = shell_escape(command);
                format!("proot-distro login {distro} -- bash -lc {inner}")
            }
        }
    }

    fn hermes_config(&self) -> HermesConfig {
        HermesConfig {
            environment: if self.recipe.hermes_install.native_apt_package.is_some() {
                HermesEnvironment::Termux
            } else {
                HermesEnvironment::ProotDistro {
                    distro: self.recipe.distro.clone(),
                }
            },
            start_mode: self.recipe.hermes_runtime.start_mode,
            transport: HermesTransportKind::TermuxSsh,
            gateway_command: self.recipe.hermes_runtime.gateway_command.clone(),
            process_match: self.recipe.hermes_runtime.process_match.clone(),
            gateway_match: "gateway run".into(),
            hermes_home: if self.recipe.hermes_install.native_apt_package.is_some() {
                "~/.hermes".into()
            } else {
                "/root/.hermes".into()
            },
            log_files: self.recipe.hermes_runtime.log_files.clone(),
            path_prepend: self.recipe.hermes_runtime.path_prepend.clone(),
            ..HermesConfig::default()
        }
    }

    async fn termux_installed(&self) -> AppResult<Option<crate::adb::TermuxPackageInfo>> {
        let state = self.app.state::<AppState>();
        let client = state.adb_client().await?;
        let mut info = client.device_info(&self.serial).await?;
        if let Some(installed) = info.termux.as_mut() {
            apply_termux_install_receipt(
                &client,
                &self.serial,
                &state.data_dir,
                &state.device_id_for(&self.serial),
                installed,
            )
            .await?;
        }
        Ok(info.termux)
    }

    async fn bootstrap_ssh(
        &self,
        cancel: CancellationToken,
        events: &mpsc::Sender<ProvisionEvent>,
    ) -> AppResult<()> {
        ensure_success(
            "Keep Termux unlocked and in the foreground",
            self.adb_shell(TERMUX_FOCUS_CHECK).await?,
        )?;
        let state = self.app.state::<AppState>();
        let public_key = keys::public_key_line(state.ssh_key()?)?;
        let script_path = self
            .app
            .path()
            .app_cache_dir()
            .map_err(|error| AppError::Io(error.to_string()))?
            .join("hacc-termux-bootstrap.sh");
        std::fs::write(&script_path, bootstrap_script())?;

        let client = state.adb_client().await?;
        let mkdir = client
            .shell(
                &self.serial,
                &format!("mkdir -p {BOOTSTRAP_DIR}"),
                COMMAND_TIMEOUT,
            )
            .await?;
        ensure_success("Prepare Termux bootstrap directory", mkdir)?;
        ensure_success(
            "Clear stale bootstrap status",
            self.adb_shell(&format!("rm -f {BOOTSTRAP_DIR}/status"))
                .await?,
        )?;
        client
            .push_file(
                &self.serial,
                &script_path,
                &format!("{BOOTSTRAP_DIR}/bootstrap.sh"),
            )
            .await?;
        let public_key_path = script_path.with_extension("pub");
        std::fs::write(&public_key_path, format!("{public_key}\n"))?;
        let push_key = client
            .push_file(
                &self.serial,
                &public_key_path,
                &format!("{BOOTSTRAP_DIR}/id_ed25519.pub"),
            )
            .await;
        let _ = std::fs::remove_file(&script_path);
        let _ = std::fs::remove_file(&public_key_path);
        push_key?;

        let launch = format!("input text 'sh%s{BOOTSTRAP_DIR}/bootstrap.sh'; input keyevent 66");
        let output = client.shell(&self.serial, &launch, COMMAND_TIMEOUT).await?;
        ensure_success("Start Termux SSH bootstrap", output)?;

        let started = Instant::now();
        let mut retried = false;
        let mut last_marker = String::new();
        loop {
            if cancel.is_cancelled() {
                return Err(AppError::Cancelled);
            }
            if started.elapsed() > BOOTSTRAP_TIMEOUT {
                return Err(AppError::Timeout {
                    operation: "Termux SSH bootstrap".into(),
                    after_ms: BOOTSTRAP_TIMEOUT.as_millis() as u64,
                });
            }
            let status = client
                .shell(
                    &self.serial,
                    &format!("cat {BOOTSTRAP_DIR}/status 2>/dev/null"),
                    Duration::from_secs(10),
                )
                .await;
            if let Ok(output) = status {
                let marker = output.stdout.trim();
                if marker != last_marker {
                    last_marker = marker.to_string();
                    if !marker.is_empty() {
                        let _ = events
                            .send(ProvisionEvent::Output {
                                step: ProvisionStepId::BootstrapSsh,
                                event: StreamEvent::Stdout {
                                    line: format!("Bootstrap status: {marker}"),
                                },
                            })
                            .await;
                    }
                }
                if marker == "done" {
                    let _ = client
                        .shell(
                            &self.serial,
                            &format!("rm -rf {BOOTSTRAP_DIR}"),
                            COMMAND_TIMEOUT,
                        )
                        .await;
                    return Ok(());
                }
                if marker.starts_with("error") {
                    return Err(AppError::CommandFailed {
                        command: "Termux SSH bootstrap".into(),
                        exit_code: Some(1),
                        stderr: marker.to_string(),
                    });
                }
            }
            if !retried && last_marker.is_empty() && started.elapsed() > Duration::from_secs(20) {
                ensure_success(
                    "Keep Termux unlocked and in the foreground",
                    self.adb_shell(TERMUX_FOCUS_CHECK).await?,
                )?;
                ensure_success(
                    "Retry Termux SSH bootstrap",
                    client.shell(&self.serial, &launch, COMMAND_TIMEOUT).await?,
                )?;
                retried = true;
            }
            tokio::select! {
                _ = cancel.cancelled() => return Err(AppError::Cancelled),
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            }
        }
    }

    async fn hermes_is_installed(&self) -> AppResult<bool> {
        let command = self.hermes_command(&self.recipe.hermes_runtime.version_command);
        let result = self
            .remote()
            .await?
            .execute(&command, COMMAND_TIMEOUT)
            .await?;
        Ok(result.exit_code == Some(0) && !result.stdout.trim().is_empty())
    }
}

fn bootstrap_script() -> &'static str {
    r#"#!/data/data/com.termux/files/usr/bin/bash
set -eu
STATUS=/sdcard/Download/hacc/status
trap 'printf "error:bootstrap\n" > "$STATUS"' ERR
printf 'installing-packages\n' > "$STATUS"
export DEBIAN_FRONTEND=noninteractive
pkg install -y -o Dpkg::Options::=--force-confold openssh termux-services
mkdir -p "$HOME/.ssh"
chmod 700 "$HOME/.ssh"
KEY=$(cat /sdcard/Download/hacc/id_ed25519.pub)
grep -qxF "$KEY" "$HOME/.ssh/authorized_keys" 2>/dev/null || printf '%s\n' "$KEY" >> "$HOME/.ssh/authorized_keys"
chmod 600 "$HOME/.ssh/authorized_keys"
SSHD="$PREFIX/etc/ssh/sshd_config"
grep -q '^ListenAddress 127.0.0.1$' "$SSHD" || printf '\nListenAddress 127.0.0.1\n' >> "$SSHD"
export SVDIR="$PREFIX/var/service" LOGDIR="$PREFIX/var/log"
(service-daemon start >/dev/null 2>&1 &)
sshd
sv-enable sshd || true
printf 'done\n' > "$STATUS"
"#
}

fn ensure_success(command: &str, output: RawOutput) -> AppResult<()> {
    if output.success() {
        Ok(())
    } else {
        Err(AppError::CommandFailed {
            command: command.into(),
            exit_code: output.exit_code,
            stderr: if output.stderr.is_empty() {
                output.stdout
            } else {
                output.stderr
            },
        })
    }
}

async fn emit_output(
    step: ProvisionStepId,
    result: &crate::transport::CommandResult,
    events: &mpsc::Sender<ProvisionEvent>,
) {
    for line in result.stdout.lines() {
        let _ = events
            .send(ProvisionEvent::Output {
                step,
                event: StreamEvent::Stdout {
                    line: line.to_string(),
                },
            })
            .await;
    }
    for line in result.stderr.lines() {
        let _ = events
            .send(ProvisionEvent::Output {
                step,
                event: StreamEvent::Stderr {
                    line: line.to_string(),
                },
            })
            .await;
    }
    let _ = events
        .send(ProvisionEvent::Output {
            step,
            event: StreamEvent::Exit {
                code: result.exit_code,
                duration_ms: result.duration_ms,
            },
        })
        .await;
}

#[async_trait]
impl<R: Runtime> ProvisionStepExecutor for AndroidProvisionExecutor<R> {
    async fn check(&self, step: ProvisionStepId) -> AppResult<ProvisionCheck> {
        let state = self.app.state::<AppState>();
        match step {
            ProvisionStepId::Preflight => {
                let Some(device) = state.devices.get(&self.serial) else {
                    return Ok(ProvisionCheck::Blocked(
                        "Phone is no longer connected.".into(),
                    ));
                };
                if device.state != DeviceState::Device || device.connection == ConnectionType::Usb {
                    return Ok(ProvisionCheck::Blocked(
                        "Connect and authorize this phone over Wireless ADB.".into(),
                    ));
                }
                let info = state.adb_client().await?.device_info(&self.serial).await?;
                if info
                    .cpu
                    .as_ref()
                    .and_then(|cpu| cpu.abi.as_deref())
                    .is_none()
                {
                    return Ok(ProvisionCheck::Blocked(
                        "The phone CPU ABI could not be detected.".into(),
                    ));
                }
                match info.storage {
                    Some(storage) if storage.free_bytes >= MIN_FREE_BYTES => {}
                    Some(_) => {
                        return Ok(ProvisionCheck::Blocked(
                            "Free at least 2 GB on the phone before provisioning.".into(),
                        ));
                    }
                    None => {
                        return Ok(ProvisionCheck::Blocked(
                            "Phone storage could not be measured; provisioning cannot safely continue.".into(),
                        ));
                    }
                }
                Ok(ProvisionCheck::Done)
            }
            ProvisionStepId::InstallTermux => {
                let Some(installed) = self.termux_installed().await? else {
                    return Ok(ProvisionCheck::Todo);
                };
                if !installed.installed {
                    return Ok(ProvisionCheck::Todo);
                }
                let source_matches = matches!(
                    (self.recipe.termux_source, installed.source),
                    (
                        ProvisionTermuxSource::Fdroid,
                        crate::adb::TermuxSource::FDroid
                    ) | (
                        ProvisionTermuxSource::Github,
                        crate::adb::TermuxSource::Sideloaded
                    )
                );
                if source_matches {
                    Ok(ProvisionCheck::Done)
                } else {
                    Ok(ProvisionCheck::Blocked(format!(
                        "Installed Termux source ({:?}) differs from recipe source ({:?}); replacing it deletes Termux data.",
                        installed.source, self.recipe.termux_source
                    )))
                }
            }
            ProvisionStepId::LaunchTermux => {
                let result = self.adb_shell(TERMUX_FOCUS_CHECK).await;
                Ok(if result.is_ok_and(|output| output.success()) {
                    ProvisionCheck::Done
                } else {
                    ProvisionCheck::Todo
                })
            }
            ProvisionStepId::BootstrapSsh | ProvisionStepId::ConnectSsh => {
                Ok(if state.termux_transport(&self.serial).await.is_ok() {
                    ProvisionCheck::Done
                } else {
                    ProvisionCheck::Todo
                })
            }
            ProvisionStepId::AndroidSettings => Ok(ProvisionCheck::Todo),
            ProvisionStepId::TermuxPackages => {
                let packages = self
                    .recipe
                    .termux_packages
                    .iter()
                    .map(|package| format!("dpkg -s {} >/dev/null 2>&1", shell_escape(package)))
                    .collect::<Vec<_>>()
                    .join(" && ");
                let command = format!("{packages} && termux-wake-lock");
                Ok(
                    match self
                        .remote()
                        .await?
                        .execute(&command, COMMAND_TIMEOUT)
                        .await
                    {
                        Ok(result) if result.exit_code == Some(0) => ProvisionCheck::Done,
                        _ => ProvisionCheck::Todo,
                    },
                )
            }
            ProvisionStepId::InstallDistro => {
                if self.recipe.distro.is_empty() {
                    return Ok(ProvisionCheck::Done);
                }
                let result = self
                    .remote()
                    .await?
                    .execute("proot-distro list --installed", COMMAND_TIMEOUT)
                    .await?;
                if !result.stdout.lines().any(|line| {
                    line.trim() == self.recipe.distro
                        || line.contains(&format!("{} (", self.recipe.distro))
                }) {
                    return Ok(ProvisionCheck::Todo);
                }
                let package_check = self.recipe.distro_packages.iter().map(|package| {
                    format!("dpkg-query -W -f='${{Status}}' {} 2>/dev/null | grep -q 'install ok installed'", shell_escape(package))
                }).collect::<Vec<_>>().join(" && ");
                if package_check.is_empty() {
                    return Ok(ProvisionCheck::Done);
                }
                let command = format!(
                    "proot-distro login {} -- bash -lc {}",
                    shell_escape(&self.recipe.distro),
                    shell_escape(&package_check)
                );
                Ok(
                    match self
                        .remote()
                        .await?
                        .execute(&command, COMMAND_TIMEOUT)
                        .await
                    {
                        Ok(result) if result.exit_code == Some(0) => ProvisionCheck::Done,
                        _ => ProvisionCheck::Todo,
                    },
                )
            }
            ProvisionStepId::InstallHermes => Ok(if self.hermes_is_installed().await? {
                ProvisionCheck::Done
            } else {
                ProvisionCheck::Todo
            }),
            ProvisionStepId::ConfigureHermes => {
                let command = self.hermes_command(&self.recipe.hermes_runtime.doctor_command);
                Ok(
                    match self
                        .remote()
                        .await?
                        .execute(&command, COMMAND_TIMEOUT)
                        .await
                    {
                        Ok(result) if result.exit_code == Some(0) => ProvisionCheck::Done,
                        _ => ProvisionCheck::Todo,
                    },
                )
            }
            ProvisionStepId::Autostart => {
                if !self.recipe.autostart {
                    return Ok(ProvisionCheck::Done);
                }
                let installed = self
                    .adb_shell("pm list packages com.termux.boot | grep -q com.termux.boot")
                    .await
                    .is_ok_and(|result| result.success());
                if !installed {
                    return Ok(ProvisionCheck::Todo);
                }
                let result = self
                    .remote()
                    .await?
                    .execute("test -x ~/.termux/boot/10-hermes", COMMAND_TIMEOUT)
                    .await;
                Ok(if result.is_ok_and(|result| result.exit_code == Some(0)) {
                    ProvisionCheck::Done
                } else {
                    ProvisionCheck::Todo
                })
            }
            ProvisionStepId::VerifyAndStart => {
                let command = self.hermes_command(&self.recipe.hermes_runtime.status_commands[0]);
                let result = self
                    .remote()
                    .await?
                    .execute(&command, COMMAND_TIMEOUT)
                    .await?;
                let lower = result.stdout.to_ascii_lowercase();
                Ok(
                    if result.exit_code == Some(0) && lower.contains("running") {
                        ProvisionCheck::Done
                    } else {
                        ProvisionCheck::Todo
                    },
                )
            }
        }
    }

    async fn run(
        &self,
        step: ProvisionStepId,
        cancel: CancellationToken,
        events: mpsc::Sender<ProvisionEvent>,
    ) -> AppResult<ProvisionStepRunOutcome> {
        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }
        let state = self.app.state::<AppState>();
        match step {
            ProvisionStepId::Preflight | ProvisionStepId::ConnectSsh => {
                if step == ProvisionStepId::ConnectSsh {
                    state.termux_transport(&self.serial).await?;
                }
                Ok(ProvisionStepRunOutcome::Done)
            }
            ProvisionStepId::InstallTermux => {
                let info = state.adb_client().await?.device_info(&self.serial).await?;
                let abi = info.cpu.and_then(|cpu| cpu.abi).ok_or_else(|| {
                    AppError::Config("The phone CPU ABI could not be detected.".into())
                })?;
                let cache = self.app.path().app_cache_dir()
                    .map_err(|error| AppError::Io(error.to_string()))?;
                let downloaded = download_termux_apk(self.recipe.termux_source, &abi, &cache).await?;
                let _ = events.send(ProvisionEvent::Output {
                    step,
                    event: StreamEvent::Stdout {
                        line: format!("Verified {} ({}, SHA-256 {}).", downloaded.file_name, downloaded.version_name, downloaded.sha256),
                    },
                }).await;
                let output = match state
                    .adb_client()
                    .await?
                    .install_apk(&self.serial, &downloaded.path)
                    .await
                {
                    Ok(output) => output,
                    Err(AppError::CommandFailed { stderr, .. })
                        if stderr.contains("INSTALL_FAILED_USER_RESTRICTED") =>
                    {
                        return Ok(ProvisionStepRunOutcome::PhoneActionNeeded(
                            "Approve the Android package installation prompt, then resume this step.".into(),
                        ));
                    }
                    Err(error) => return Err(error),
                };
                record_termux_install(&state.data_dir, &state.device_id_for(&self.serial), self.recipe.termux_source, &downloaded)?;
                emit_output(step, &crate::transport::CommandResult {
                    stdout: output.stdout,
                    stderr: output.stderr,
                    exit_code: output.exit_code,
                    duration_ms: output.duration_ms,
                }, &events).await;
                Ok(ProvisionStepRunOutcome::Done)
            }
            ProvisionStepId::AndroidSettings => {
                let info = state.adb_client().await?.device_info(&self.serial).await?;
                let mut commands = Vec::new();
                if info.sdk.is_some_and(|sdk| sdk >= 33) {
                    commands.push("pm grant com.termux android.permission.POST_NOTIFICATIONS".to_string());
                }
                commands.push("dumpsys deviceidle whitelist +com.termux".into());
                if info.sdk.is_some_and(|sdk| (31..=33).contains(&sdk)) {
                    commands.push("device_config set_sync_disabled_for_tests persistent".into());
                    commands.push("device_config put activity_manager max_phantom_processes 2147483647".into());
                } else if info.sdk.is_some_and(|sdk| sdk >= 34) {
                    commands.push("settings put global settings_enable_monitor_phantom_procs false".into());
                }
                for command in commands {
                    let result = state.adb_client().await?.shell(&self.serial, &command, COMMAND_TIMEOUT).await?;
                    let _ = events.send(ProvisionEvent::Output {
                        step,
                        event: if result.success() {
                            StreamEvent::Stdout { line: format!("Applied: {command}") }
                        } else {
                            StreamEvent::Stderr { line: format!("Optional setting failed: {}", result.combined()) }
                        },
                    }).await;
                }
                Ok(ProvisionStepRunOutcome::Done)
            }
            ProvisionStepId::LaunchTermux => {
                let result = state.adb_client().await?.shell(
                    &self.serial,
                    "am start -n com.termux/.app.TermuxActivity",
                    COMMAND_TIMEOUT,
                ).await?;
                ensure_success("Launch Termux", result)?;
                Ok(ProvisionStepRunOutcome::PhoneActionNeeded(
                    "Unlock the phone and keep Termux in the foreground.".into(),
                ))
            }
            ProvisionStepId::BootstrapSsh => {
                self.bootstrap_ssh(cancel, &events).await?;
                Ok(ProvisionStepRunOutcome::Done)
            }
            ProvisionStepId::TermuxPackages => {
                let packages = self.recipe.termux_packages.iter()
                    .map(|package| shell_escape(package))
                    .collect::<Vec<_>>().join(" ");
                let command = format!(
                    "pkg install -y {packages} && export SVDIR=\"$PREFIX/var/service\" LOGDIR=\"$PREFIX/var/log\" && (service-daemon start >/dev/null 2>&1 &) && sv-enable sshd && termux-wake-lock"
                );
                self.run_remote(step, &command, Duration::from_secs(600), &events).await?;
                Ok(ProvisionStepRunOutcome::Done)
            }
            ProvisionStepId::InstallDistro => {
                if self.recipe.distro.is_empty() {
                    return Ok(ProvisionStepRunOutcome::Done);
                }
                let distro = shell_escape(&self.recipe.distro);
                self.run_remote(step, &format!("proot-distro install {distro}"), Duration::from_secs(1200), &events).await?;
                if !self.recipe.distro_packages.is_empty() {
                    let packages = self.recipe.distro_packages.iter()
                        .map(|package| shell_escape(package))
                        .collect::<Vec<_>>().join(" ");
                    let command = format!(
                        "proot-distro login {distro} -- bash -lc {}",
                        shell_escape(&format!("apt-get update && apt-get install -y {packages}"))
                    );
                    self.run_remote(step, &command, Duration::from_secs(1200), &events).await?;
                }
                Ok(ProvisionStepRunOutcome::Done)
            }
            ProvisionStepId::InstallHermes => {
                if self.recipe.hermes_install.native_apt_package.is_some() {
                    return Ok(ProvisionStepRunOutcome::PhoneActionNeeded(
                        "This experimental native APT recipe requires adding its signed repository manually; no repository URL is configured.".into(),
                    ));
                }
                if self.recipe.hermes_install.interactive {
                    return Ok(ProvisionStepRunOutcome::PhoneActionNeeded(
                        "Open the interactive Termux terminal and run the reviewed Hermes installer inside the selected environment.".into(),
                    ));
                }
                let url = self.recipe.hermes_install.script_url.as_deref().ok_or_else(|| AppError::Config("No Hermes installer URL is configured.".into()))?;
                let command = self.hermes_command(&format!(
                    "curl -fsSL {} -o /tmp/hacc-hermes-install.sh && bash /tmp/hacc-hermes-install.sh",
                    shell_escape(url)
                ));
                self.run_remote(step, &command, Duration::from_secs(1800), &events).await?;
                Ok(ProvisionStepRunOutcome::Done)
            }
            ProvisionStepId::ConfigureHermes => Ok(ProvisionStepRunOutcome::PhoneActionNeeded(
                format!(
                    "Complete these commands in the interactive terminal: {}. Then resume this step.",
                    self.recipe.hermes_configure.steps.join("; ")
                ),
            )),
            ProvisionStepId::Autostart => {
                if !self.recipe.autostart {
                    return Ok(ProvisionStepRunOutcome::Done);
                }
                let boot_installed = self
                    .adb_shell("pm list packages com.termux.boot | grep -q com.termux.boot")
                    .await
                    .is_ok_and(|output| output.success());
                if !boot_installed {
                    let info = state.adb_client().await?.device_info(&self.serial).await?;
                    let abi = info.cpu.and_then(|cpu| cpu.abi).ok_or_else(|| {
                        AppError::Config("The phone CPU ABI could not be detected.".into())
                    })?;
                    let cache = self.app.path().app_cache_dir()
                        .map_err(|error| AppError::Io(error.to_string()))?;
                    let downloaded = download_termux_boot_apk(
                        self.recipe.termux_source,
                        &abi,
                        &cache,
                    ).await?;
                    let output = match state
                        .adb_client()
                        .await?
                        .install_apk(&self.serial, &downloaded.path)
                        .await
                    {
                        Ok(output) => output,
                        Err(AppError::CommandFailed { stderr, .. })
                            if stderr.contains("INSTALL_FAILED_USER_RESTRICTED") =>
                        {
                            return Ok(ProvisionStepRunOutcome::PhoneActionNeeded(
                                "Approve the Termux:Boot installation prompt, then resume this step.".into(),
                            ));
                        }
                        Err(error) => return Err(error),
                    };
                    emit_output(step, &crate::transport::CommandResult {
                        stdout: output.stdout,
                        stderr: output.stderr,
                        exit_code: output.exit_code,
                        duration_ms: output.duration_ms,
                    }, &events).await;
                    return Ok(ProvisionStepRunOutcome::PhoneActionNeeded(
                        "Open Termux:Boot once on the phone, then resume this step to create the boot script.".into(),
                    ));
                }
                let launch = self
                    .adb_shell("am start -n com.termux.boot/.BootActivity")
                    .await?;
                ensure_success("Launch Termux:Boot", launch)?;
                let config = self.hermes_config();
                let launch = action_command(&config, HermesAction::Start);
                let script = format!(
                    "#!/data/data/com.termux/files/usr/bin/bash\ntermux-wake-lock\nexport SVDIR=\"$PREFIX/var/service\" LOGDIR=\"$PREFIX/var/log\"\n(service-daemon start >/dev/null 2>&1 &)\nsshd\n{launch}\n"
                );
                let encoded = base64::engine::general_purpose::STANDARD.encode(script);
                let command = format!(
                    "mkdir -p \"$HOME/.termux/boot\" && printf '%s' '{encoded}' | base64 -d > \"$HOME/.termux/boot/10-hermes\" && chmod 700 \"$HOME/.termux/boot/10-hermes\""
                );
                self.run_remote(step, &command, COMMAND_TIMEOUT, &events).await?;
                Ok(ProvisionStepRunOutcome::Done)
            }
            ProvisionStepId::VerifyAndStart => {
                let config = self.hermes_config();
                let command = action_command(&config, HermesAction::Start);
                self.run_remote(step, &command, Duration::from_secs(120), &events).await?;
                let status = self.hermes_command(&self.recipe.hermes_runtime.status_commands[0]);
                let result = self.remote().await?.execute(&status, COMMAND_TIMEOUT).await?;
                emit_output(step, &result, &events).await;
                let lower = result.stdout.to_ascii_lowercase();
                if result.exit_code == Some(0) && lower.contains("running") {
                    Ok(ProvisionStepRunOutcome::Done)
                } else {
                    Ok(ProvisionStepRunOutcome::PhoneActionNeeded(
                        "Hermes was started, but its status has not confirmed Running. Review the output, then resume this step.".into(),
                    ))
                }
            }
        }
    }
}
