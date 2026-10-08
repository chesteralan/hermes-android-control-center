use std::path::PathBuf;
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
    apply_termux_install_receipt, cancellable, download_termux_apk, download_termux_boot_apk,
    record_termux_install,
};
use crate::provision::plan::{ProvisionCheck, ProvisionStepExecutor, ProvisionStepRunOutcome};
use crate::provision::recipe::{ProvisionRecipe, ProvisionTermuxSource};
use crate::provision::types::{ProvisionEvent, ProvisionStepId};
use crate::state::AppState;
use crate::termux::{keys, shell_escape};
use crate::transport::{DeviceTransport, StreamEvent};

const BOOTSTRAP_DIR: &str = "/sdcard/Download/hacc";
const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(600);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);
const TERMUX_FOCUS_CHECK: &str =
    "dumpsys window | grep -E 'mCurrentFocus|mFocusedApp' | grep -q 'com.termux/'";

fn device_awake_and_unlocked(power: &str, window: &str) -> bool {
    let asleep = power.contains("mWakefulness=Asleep") || power.contains("mWakefulness=Dozing");
    let awake =
        !asleep && (power.contains("mWakefulness=Awake") || power.contains("mInteractive=true"));
    let locked = [
        "mShowingLockscreen=true",
        "mDreamingLockscreen=true",
        "mKeyguardShowing=true",
    ]
    .iter()
    .any(|marker| window.contains(marker));
    let unlocked = [
        "mShowingLockscreen=false",
        "mDreamingLockscreen=false",
        "mKeyguardShowing=false",
    ]
    .iter()
    .any(|marker| window.contains(marker));
    awake && unlocked && !locked
}

fn distro_is_listed_installed(output: &str, distro: &str) -> bool {
    output.lines().any(|line| {
        let line = line.trim();
        line == distro
            || line
                .strip_prefix(distro)
                .is_some_and(|suffix| suffix.starts_with(" ("))
    })
}

fn distro_start_probe_command(distro: &str) -> String {
    format!(
        "proot-distro login {} -- bash -lc {}",
        shell_escape(distro),
        shell_escape("true")
    )
}

fn distro_repair_guidance(distro: &str) -> String {
    format!(
        "The {distro} rootfs is listed as installed but cannot start. This app will not remove it automatically. Back up any files you need, then in Termux run `proot-distro remove {distro}` followed by `proot-distro install {distro}`. Removing the distro deletes all files inside it. Resume this step after reinstalling."
    )
}

fn android_settings_commands(sdk: Option<u32>) -> Vec<String> {
    let mut commands = vec![
        "pm grant com.termux android.permission.READ_EXTERNAL_STORAGE".into(),
        "pm grant com.termux android.permission.WRITE_EXTERNAL_STORAGE".into(),
    ];
    if sdk.is_some_and(|sdk| sdk >= 33) {
        commands.push("pm grant com.termux android.permission.POST_NOTIFICATIONS".into());
    }
    commands.push("dumpsys deviceidle whitelist +com.termux".into());
    if sdk.is_some_and(|sdk| (31..=33).contains(&sdk)) {
        commands.push("device_config set_sync_disabled_for_tests persistent".into());
        commands.push("device_config put activity_manager max_phantom_processes 2147483647".into());
    } else if sdk.is_some_and(|sdk| sdk >= 34) {
        commands.push("settings put global settings_enable_monitor_phantom_procs false".into());
    }
    commands
}

fn doctor_warning_lines(stdout: &str, stderr: &str) -> Vec<String> {
    stdout
        .lines()
        .chain(stderr.lines())
        .filter(|line| line.to_ascii_lowercase().contains("warn"))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

struct BootstrapTempFiles(PathBuf);

impl Drop for BootstrapTempFiles {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(self.0.with_extension("pub"));
    }
}

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

    async fn phone_awake_and_unlocked(&self) -> AppResult<bool> {
        let power = self.adb_shell("dumpsys power").await?;
        let window = self.adb_shell("dumpsys window").await?;
        Ok(device_awake_and_unlocked(&power.stdout, &window.stdout))
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
        cancel: CancellationToken,
        events: &mpsc::Sender<ProvisionEvent>,
    ) -> AppResult<()> {
        let stream_cancel = cancel.child_token();
        let stream = self
            .remote()
            .await?
            .stream(command, stream_cancel.clone())
            .await?;
        let result =
            forward_remote_stream(step, command, timeout, cancel, stream, events.clone()).await;
        if result.is_err() {
            stream_cancel.cancel();
        }
        result
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

    async fn distro_is_installed(&self) -> AppResult<bool> {
        let result = self
            .remote()
            .await?
            .execute("proot-distro list --installed", COMMAND_TIMEOUT)
            .await?;
        if result.exit_code != Some(0) {
            return Err(AppError::CommandFailed {
                command: "List installed proot distros".into(),
                exit_code: result.exit_code,
                stderr: if result.stderr.is_empty() {
                    result.stdout
                } else {
                    result.stderr
                },
            });
        }
        Ok(distro_is_listed_installed(
            &result.stdout,
            &self.recipe.distro,
        ))
    }

    async fn distro_is_startable(&self) -> AppResult<bool> {
        let result = self
            .remote()
            .await?
            .execute(
                &distro_start_probe_command(&self.recipe.distro),
                COMMAND_TIMEOUT,
            )
            .await?;
        Ok(result.exit_code == Some(0))
    }

    async fn bootstrap_ssh(
        &self,
        cancel: CancellationToken,
        events: &mpsc::Sender<ProvisionEvent>,
    ) -> AppResult<()> {
        let result = self.bootstrap_ssh_inner(cancel, events).await;
        let cleanup = self
            .adb_shell(&format!("rm -rf {BOOTSTRAP_DIR}"))
            .await
            .and_then(|output| ensure_success("Clean up Termux SSH bootstrap files", output));
        match result {
            Ok(()) => cleanup,
            Err(error) => {
                if let Err(cleanup_error) = cleanup {
                    tracing::warn!(error = %cleanup_error, "failed to remove Termux bootstrap files");
                }
                Err(error)
            }
        }
    }

    async fn bootstrap_ssh_inner(
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
        let _temporary_files = BootstrapTempFiles(script_path.clone());
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

async fn forward_remote_stream(
    step: ProvisionStepId,
    command: &str,
    timeout: Duration,
    cancel: CancellationToken,
    mut stream: mpsc::Receiver<StreamEvent>,
    events: mpsc::Sender<ProvisionEvent>,
) -> AppResult<()> {
    let mut stdout = String::new();
    let mut stderr = String::new();
    let collect = async {
        loop {
            let next = tokio::select! {
                _ = cancel.cancelled() => return Err(AppError::Cancelled),
                event = stream.recv() => event,
            };
            let Some(event) = next else {
                return Err(AppError::Io(
                    "Remote command stream closed without an exit event.".into(),
                ));
            };

            let output_event = match event {
                StreamEvent::Stdout { line } => {
                    stdout.push_str(&line);
                    stdout.push('\n');
                    Some(StreamEvent::Stdout { line })
                }
                StreamEvent::Stderr { line } => {
                    stderr.push_str(&line);
                    stderr.push('\n');
                    Some(StreamEvent::Stderr { line })
                }
                StreamEvent::Error { error } => {
                    let failure = AppError::Io(match error.details.as_deref() {
                        Some(details) if !details.is_empty() => {
                            format!("{}\n{details}", error.message)
                        }
                        _ => error.message.clone(),
                    });
                    if events
                        .send(ProvisionEvent::Output {
                            step,
                            event: StreamEvent::Error { error },
                        })
                        .await
                        .is_err()
                    {
                        return Err(AppError::Cancelled);
                    }
                    return Err(failure);
                }
                StreamEvent::Exit { code, duration_ms } => {
                    if events
                        .send(ProvisionEvent::Output {
                            step,
                            event: StreamEvent::Exit { code, duration_ms },
                        })
                        .await
                        .is_err()
                    {
                        return Err(AppError::Cancelled);
                    }
                    return if code == Some(0) {
                        Ok(())
                    } else {
                        Err(AppError::CommandFailed {
                            command: command.to_string(),
                            exit_code: code,
                            stderr: if stderr.is_empty() { stdout } else { stderr },
                        })
                    };
                }
            };

            if let Some(event) = output_event {
                if events
                    .send(ProvisionEvent::Output { step, event })
                    .await
                    .is_err()
                {
                    return Err(AppError::Cancelled);
                }
            }
        }
    };

    match tokio::time::timeout(timeout, collect).await {
        Ok(result) => result,
        Err(_) => Err(AppError::Timeout {
            operation: "Provisioning command".into(),
            after_ms: timeout.as_millis() as u64,
        }),
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

fn emit_apk_download_progress(
    step: ProvisionStepId,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    events: &mpsc::Sender<ProvisionEvent>,
) {
    let mib = 1024.0 * 1024.0;
    let downloaded_mib = downloaded_bytes as f64 / mib;
    let progress = match total_bytes.filter(|total| *total > 0) {
        Some(total) => {
            let total_mib = total as f64 / mib;
            let percent = downloaded_bytes as f64 * 100.0 / total as f64;
            format!("{downloaded_mib:.1} / {total_mib:.1} MiB ({percent:.0}%)")
        }
        None => format!("{downloaded_mib:.1} MiB downloaded"),
    };
    let _ = events.try_send(ProvisionEvent::Output {
        step,
        event: StreamEvent::Stdout {
            line: format!("APK download: {progress}"),
        },
    });
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
                    Some(storage) if storage.free_bytes >= self.recipe.minimum_free_bytes() => {}
                    Some(_) => {
                        return Ok(ProvisionCheck::Blocked(format!(
                            "Free at least {} GiB on the phone before provisioning.",
                            self.recipe.minimum_free_gib
                        )));
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
                    Ok(
                        if self
                            .recipe
                            .termux_version_meets_minimum(installed.version_name.as_deref())
                        {
                            ProvisionCheck::Done
                        } else {
                            ProvisionCheck::Todo
                        },
                    )
                } else {
                    Ok(ProvisionCheck::Blocked(format!(
                        "Installed Termux source ({:?}) differs from recipe source ({:?}); replacing it deletes Termux data.",
                        installed.source, self.recipe.termux_source
                    )))
                }
            }
            ProvisionStepId::LaunchTermux => {
                if !self.phone_awake_and_unlocked().await? {
                    return Ok(ProvisionCheck::Todo);
                }
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
                if !self.distro_is_installed().await? {
                    return Ok(ProvisionCheck::Todo);
                }
                if !self.distro_is_startable().await? {
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
                match self
                    .remote()
                    .await?
                    .execute(&command, COMMAND_TIMEOUT)
                    .await
                {
                    Ok(result) if result.exit_code == Some(0) => {
                        let warnings = doctor_warning_lines(&result.stdout, &result.stderr);
                        if warnings.is_empty() {
                            Ok(ProvisionCheck::Done)
                        } else {
                            Ok(ProvisionCheck::DoneWithWarnings(warnings))
                        }
                    }
                    _ => Ok(ProvisionCheck::Todo),
                }
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
                let progress_events = events.clone();
                let downloaded = download_termux_apk(
                    self.recipe.termux_source,
                    &abi,
                    &cache,
                    cancel.clone(),
                    move |downloaded_bytes, total_bytes| {
                        emit_apk_download_progress(step, downloaded_bytes, total_bytes, &progress_events);
                    },
                )
                .await?;
                if !self
                    .recipe
                    .termux_version_meets_minimum(Some(&downloaded.version_name))
                {
                    let minimum = self
                        .recipe
                        .minimum_termux_version
                        .as_deref()
                        .unwrap_or("a configured minimum");
                    return Err(AppError::Config(format!(
                        "Available Termux version {} is below recipe minimum {minimum}.",
                        downloaded.version_name
                    )));
                }
                let _ = events.send(ProvisionEvent::Output {
                    step,
                    event: StreamEvent::Stdout {
                        line: format!("Verified {} ({}, SHA-256 {}).", downloaded.file_name, downloaded.version_name, downloaded.sha256),
                    },
                }).await;
                let client = state.adb_client().await?;
                let output = match cancellable(
                    &cancel,
                    client.install_apk(&self.serial, &downloaded.path),
                )
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
                for command in android_settings_commands(info.sdk) {
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
                let _ = self.adb_shell("input keyevent KEYCODE_WAKEUP").await;
                if !self.phone_awake_and_unlocked().await? {
                    return Ok(ProvisionStepRunOutcome::PhoneActionNeeded(
                        "Unlock the phone, then resume this step.".into(),
                    ));
                }
                let result = state.adb_client().await?.shell(
                    &self.serial,
                    "am start -n com.termux/.app.TermuxActivity",
                    COMMAND_TIMEOUT,
                ).await?;
                ensure_success("Launch Termux", result)?;
                let started = Instant::now();
                loop {
                    if cancel.is_cancelled() {
                        return Err(AppError::Cancelled);
                    }
                    if self
                        .adb_shell(TERMUX_FOCUS_CHECK)
                        .await
                        .is_ok_and(|output| output.success())
                    {
                        return Ok(ProvisionStepRunOutcome::Done);
                    }
                    if started.elapsed() >= Duration::from_secs(20) {
                        return Ok(ProvisionStepRunOutcome::PhoneActionNeeded(
                            "Keep Termux open in the foreground, then resume this step.".into(),
                        ));
                    }
                    tokio::select! {
                        _ = cancel.cancelled() => return Err(AppError::Cancelled),
                        _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                    }
                }
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
                self.run_remote(step, &command, Duration::from_secs(600), cancel.clone(), &events)
                    .await?;
                Ok(ProvisionStepRunOutcome::Done)
            }
            ProvisionStepId::InstallDistro => {
                if self.recipe.distro.is_empty() {
                    return Ok(ProvisionStepRunOutcome::Done);
                }
                let distro = shell_escape(&self.recipe.distro);
                let distro_installed = self.distro_is_installed().await?;
                if distro_installed && !self.distro_is_startable().await? {
                    return Ok(ProvisionStepRunOutcome::PhoneActionNeeded(
                        distro_repair_guidance(&self.recipe.distro),
                    ));
                }
                if !distro_installed {
                    self.run_remote(
                        step,
                        &format!("proot-distro install {distro}"),
                        Duration::from_secs(1200),
                        cancel.clone(),
                        &events,
                    )
                    .await?;
                    if !self.distro_is_startable().await? {
                        return Ok(ProvisionStepRunOutcome::PhoneActionNeeded(
                            distro_repair_guidance(&self.recipe.distro),
                        ));
                    }
                }
                if !self.recipe.distro_packages.is_empty() {
                    let packages = self.recipe.distro_packages.iter()
                        .map(|package| shell_escape(package))
                        .collect::<Vec<_>>().join(" ");
                    let command = format!(
                        "proot-distro login {distro} -- bash -lc {}",
                        shell_escape(&format!("apt-get update && apt-get install -y {packages}"))
                    );
                    self.run_remote(step, &command, Duration::from_secs(1200), cancel.clone(), &events)
                        .await?;
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
                self.run_remote(step, &command, Duration::from_secs(1800), cancel.clone(), &events)
                    .await?;
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
                    let progress_events = events.clone();
                    let downloaded = download_termux_boot_apk(
                        self.recipe.termux_source,
                        &abi,
                        &cache,
                        cancel.clone(),
                        move |downloaded_bytes, total_bytes| {
                            emit_apk_download_progress(
                                step,
                                downloaded_bytes,
                                total_bytes,
                                &progress_events,
                            );
                        },
                    ).await?;
                    let client = state.adb_client().await?;
                    let output = match cancellable(
                        &cancel,
                        client.install_apk(&self.serial, &downloaded.path),
                    )
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
                self.run_remote(step, &command, COMMAND_TIMEOUT, cancel.clone(), &events)
                    .await?;
                Ok(ProvisionStepRunOutcome::Done)
            }
            ProvisionStepId::VerifyAndStart => {
                let config = self.hermes_config();
                let command = action_command(&config, HermesAction::Start);
                self.run_remote(step, &command, Duration::from_secs(120), cancel.clone(), &events)
                    .await?;
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

#[cfg(test)]
mod tests {
    use super::{
        android_settings_commands, device_awake_and_unlocked, distro_is_listed_installed,
        distro_repair_guidance, distro_start_probe_command, doctor_warning_lines,
        forward_remote_stream, BootstrapTempFiles,
    };
    use crate::error::AppError;
    use crate::provision::types::{ProvisionEvent, ProvisionStepId};
    use crate::transport::StreamEvent;
    use std::time::Duration;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    #[test]
    fn android_settings_commands_cover_storage_notifications_and_phantom_api_ranges() {
        let android_11 = android_settings_commands(Some(30));
        assert!(android_11
            .contains(&"pm grant com.termux android.permission.READ_EXTERNAL_STORAGE".into()));
        assert!(android_11
            .contains(&"pm grant com.termux android.permission.WRITE_EXTERNAL_STORAGE".into()));
        assert!(!android_11
            .iter()
            .any(|command| command.contains("POST_NOTIFICATIONS")));

        let android_12 = android_settings_commands(Some(31));
        assert!(android_12
            .iter()
            .any(|command| command.contains("max_phantom_processes")));
        assert!(!android_12
            .iter()
            .any(|command| command.contains("POST_NOTIFICATIONS")));

        let android_13 = android_settings_commands(Some(33));
        assert!(android_13
            .iter()
            .any(|command| command.contains("POST_NOTIFICATIONS")));
        assert!(android_13
            .iter()
            .any(|command| command.contains("max_phantom_processes")));

        let android_14 = android_settings_commands(Some(34));
        assert!(android_14
            .iter()
            .any(|command| command.contains("POST_NOTIFICATIONS")));
        assert!(android_14
            .iter()
            .any(|command| command.contains("settings_enable_monitor_phantom_procs")));

        let unknown_sdk = android_settings_commands(None);
        assert_eq!(unknown_sdk.len(), 3);
        assert!(!unknown_sdk
            .iter()
            .any(|command| command.contains("POST_NOTIFICATIONS")));
    }

    #[test]
    fn device_readiness_requires_awake_and_explicitly_unlocked_state() {
        assert!(device_awake_and_unlocked(
            "mWakefulness=Awake",
            "mShowingLockscreen=false"
        ));
        assert!(!device_awake_and_unlocked(
            "mWakefulness=Asleep",
            "mShowingLockscreen=false"
        ));
        assert!(!device_awake_and_unlocked(
            "mWakefulness=Asleep\nmInteractive=true",
            "mShowingLockscreen=false"
        ));
        assert!(!device_awake_and_unlocked(
            "mWakefulness=Awake",
            "mShowingLockscreen=true"
        ));
        assert!(!device_awake_and_unlocked(
            "mWakefulness=Awake",
            "window state unknown"
        ));
    }

    #[test]
    fn distro_listing_matches_only_the_selected_distribution() {
        assert!(distro_is_listed_installed("debian\n", "debian"));
        assert!(distro_is_listed_installed("debian (installed)\n", "debian"));
        assert!(!distro_is_listed_installed("debian-testing\n", "debian"));
        assert!(!distro_is_listed_installed(
            "ubuntu (installed)\n",
            "debian"
        ));
    }

    #[test]
    fn distro_start_probe_targets_and_escapes_selected_distro() {
        assert_eq!(
            distro_start_probe_command("debian"),
            "proot-distro login debian -- bash -lc true"
        );
        assert_eq!(
            distro_start_probe_command("debian; id"),
            "proot-distro login 'debian; id' -- bash -lc true"
        );
    }

    #[test]
    fn partial_rootfs_guidance_warns_before_removal() {
        let guidance = distro_repair_guidance("debian");
        assert!(guidance.contains("will not remove it automatically"));
        assert!(guidance.contains("proot-distro remove debian"));
        assert!(guidance.contains("deletes all files inside it"));
    }

    #[test]
    fn doctor_warning_lines_select_warning_output_without_command_contents() {
        assert_eq!(
            doctor_warning_lines(
                "Doctor passed\nWarning: service setup unavailable",
                "WARN: configure an allow-listed user\nordinary detail"
            ),
            [
                "Warning: service setup unavailable",
                "WARN: configure an allow-listed user"
            ]
        );
    }

    #[test]
    fn bootstrap_temporary_files_are_removed_on_drop() {
        let root =
            std::env::temp_dir().join(format!("hacc-bootstrap-temp-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("bootstrap.sh");
        let public_key = script.with_extension("pub");
        std::fs::write(&script, "script").unwrap();
        std::fs::write(&public_key, "public key").unwrap();

        drop(BootstrapTempFiles(script.clone()));

        assert!(!script.exists());
        assert!(!public_key.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn remote_stream_forwards_output_before_exit() {
        let (remote_sender, remote_stream) = mpsc::channel(4);
        let (event_sender, mut event_receiver) = mpsc::channel(4);
        let cancel = CancellationToken::new();
        let task = tokio::spawn(forward_remote_stream(
            ProvisionStepId::TermuxPackages,
            "pkg install openssh",
            Duration::from_secs(2),
            cancel,
            remote_stream,
            event_sender,
        ));

        remote_sender
            .send(StreamEvent::Stdout {
                line: "resolving packages".into(),
            })
            .await
            .unwrap();
        let first = event_receiver.recv().await.unwrap();
        assert!(matches!(
            first,
            ProvisionEvent::Output {
                event: StreamEvent::Stdout { line },
                ..
            } if line == "resolving packages"
        ));

        remote_sender
            .send(StreamEvent::Exit {
                code: Some(0),
                duration_ms: 50,
            })
            .await
            .unwrap();
        assert!(matches!(
            event_receiver.recv().await,
            Some(ProvisionEvent::Output {
                event: StreamEvent::Exit { code: Some(0), .. },
                ..
            })
        ));
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn remote_stream_failure_preserves_stderr() {
        let (remote_sender, remote_stream) = mpsc::channel(4);
        let (event_sender, _event_receiver) = mpsc::channel(4);
        remote_sender
            .send(StreamEvent::Stderr {
                line: "package mirror unavailable".into(),
            })
            .await
            .unwrap();
        remote_sender
            .send(StreamEvent::Exit {
                code: Some(17),
                duration_ms: 50,
            })
            .await
            .unwrap();

        let error = forward_remote_stream(
            ProvisionStepId::TermuxPackages,
            "pkg install openssh",
            Duration::from_secs(2),
            CancellationToken::new(),
            remote_stream,
            event_sender,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            AppError::CommandFailed {
                exit_code: Some(17),
                ref stderr,
                ..
            } if stderr.contains("package mirror unavailable")
        ));
    }
}
