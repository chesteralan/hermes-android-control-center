use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::error::{AppError, AppResult};
use crate::transport::StreamEvent;

use super::recipe::ProvisionRecipe;
use super::types::{
    ProvisionConsent, ProvisionEvent, ProvisionPlan, ProvisionStep, ProvisionStepId,
    ProvisionStepState,
};

pub fn build_plan(
    serial: impl Into<String>,
    device_id: impl Into<String>,
    recipe: &ProvisionRecipe,
    progress: &ProvisionProgress,
) -> ProvisionPlan {
    let serial = serial.into();
    let device_id = device_id.into();
    let mut steps = vec![
        step(ProvisionStepId::Preflight, "Preflight", None, None),
        step(
            ProvisionStepId::InstallTermux,
            "Install Termux",
            Some(ProvisionConsent {
                title: "Install Termux from the selected source".into(),
                commands: vec![format!(
                    "Download the official {} APK for this phone, verify its SHA-256, then run adb install -r.",
                    source_label(recipe)
                )],
                destructive: false,
            }),
            Some("Allow installation when Android prompts you.".into()),
        ),
        step(
            ProvisionStepId::AndroidSettings,
            "Android settings",
            Some(ProvisionConsent {
                title: "Adjust optional Termux background settings".into(),
                commands: vec![
                    "pm grant com.termux android.permission.POST_NOTIFICATIONS (Android 13+)".into(),
                    "dumpsys deviceidle whitelist +com.termux".into(),
                    "Android 12-13: raise the per-device phantom process limit".into(),
                ],
                destructive: false,
            }),
            None,
        ),
        step(
            ProvisionStepId::LaunchTermux,
            "Launch Termux",
            None,
            Some("Unlock the phone and keep Termux visible for first launch.".into()),
        ),
        step(
            ProvisionStepId::BootstrapSsh,
            "Bootstrap the SSH bridge",
            Some(ProvisionConsent {
                title: "Authorize this app's public key in Termux".into(),
                commands: vec![
                    "Copy the app public key and an idempotent bootstrap script to /sdcard/Download/hacc/.".into(),
                    "Type sh /sdcard/Download/hacc/bootstrap.sh in Termux.".into(),
                ],
                destructive: false,
            }),
            Some("Keep Termux focused while the SSH bootstrap runs.".into()),
        ),
        step(
            ProvisionStepId::ConnectSsh,
            "Connect over SSH",
            None,
            None,
        ),
        step(
            ProvisionStepId::TermuxPackages,
            "Install Termux packages",
            Some(ProvisionConsent {
                title: "Install packages and enable the Termux wake lock".into(),
                commands: vec![format!("pkg install -y {}", recipe.termux_packages.join(" "))],
                destructive: false,
            }),
            None,
        ),
        step(
            ProvisionStepId::InstallDistro,
            "Install Linux environment",
            (!recipe.distro.is_empty()).then(|| ProvisionConsent {
                title: format!("Install {} in proot-distro", recipe.distro),
                commands: vec![
                    format!("proot-distro install {}", recipe.distro),
                    format!(
                        "Install distro packages: {}",
                        recipe.distro_packages.join(" ")
                    ),
                ],
                destructive: false,
            }),
            None,
        ),
        step(
            ProvisionStepId::InstallHermes,
            "Install Hermes Agent",
            Some(ProvisionConsent {
                title: "Review and install Hermes Agent".into(),
                commands: recipe
                    .hermes_install
                    .script_url
                    .as_ref()
                    .map(|url| {
                        vec![
                            format!("Download {url} over HTTPS and show its SHA-256."),
                            "Run the reviewed installer inside the selected environment.".into(),
                        ]
                    })
                    .unwrap_or_else(|| {
                        vec![format!(
                            "Install the Termux package {} after verifying its signing-key fingerprint.",
                            recipe.hermes_install.native_apt_package.as_deref().unwrap_or("hermes-agent")
                        )]
                    }),
                destructive: false,
            }),
            None,
        ),
        step(
            ProvisionStepId::ConfigureHermes,
            "Configure Hermes",
            None,
            Some("Complete Hermes setup and enter provider secrets in the interactive terminal.".into()),
        ),
        step(
            ProvisionStepId::Autostart,
            "Configure autostart",
            recipe.autostart.then(|| ProvisionConsent {
                title: "Start Hermes after the phone reboots".into(),
                commands: vec![
                    "Install Termux:Boot from the selected Termux source.".into(),
                    "Create ~/.termux/boot/10-hermes with the Hermes supervisor launch command.".into(),
                ],
                destructive: false,
            }),
            None,
        ),
        step(
            ProvisionStepId::VerifyAndStart,
            "Verify and start Hermes",
            Some(ProvisionConsent {
                title: "Start the Hermes gateway".into(),
                commands: vec![recipe.hermes_runtime.gateway_command.clone()],
                destructive: false,
            }),
            None,
        ),
    ];

    for item in &mut steps {
        if progress.completed_steps.contains(&item.id)
            || item.id == ProvisionStepId::InstallDistro && recipe.distro.is_empty()
            || item.id == ProvisionStepId::Autostart && !recipe.autostart
        {
            item.state = ProvisionStepState::Done;
        }
    }

    ProvisionPlan {
        serial,
        device_id,
        recipe_id: recipe.id.clone(),
        steps,
        current_step: None,
    }
}

fn source_label(recipe: &ProvisionRecipe) -> &'static str {
    match recipe.termux_source {
        super::recipe::ProvisionTermuxSource::Fdroid => "F-Droid",
        super::recipe::ProvisionTermuxSource::Github => "GitHub",
    }
}

fn step(
    id: ProvisionStepId,
    title: &str,
    consent: Option<ProvisionConsent>,
    phone_action: Option<String>,
) -> ProvisionStep {
    ProvisionStep {
        id,
        title: title.into(),
        state: ProvisionStepState::Todo,
        detail: None,
        phone_action,
        consent,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvisionCheck {
    Done,
    DoneWithWarnings(Vec<String>),
    Todo,
    Blocked(String),
}

#[async_trait]
pub trait ProvisionStepExecutor: Send + Sync {
    async fn check(&self, step: ProvisionStepId) -> AppResult<ProvisionCheck>;

    async fn run(
        &self,
        step: ProvisionStepId,
        cancel: CancellationToken,
        events: mpsc::Sender<ProvisionEvent>,
    ) -> AppResult<ProvisionStepRunOutcome>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvisionStepRunOutcome {
    Done,
    PhoneActionNeeded(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProvisionProgress {
    pub device_id: String,
    pub recipe_id: String,
    pub completed_steps: Vec<ProvisionStepId>,
    #[serde(default)]
    pub completed_at_unix_ms: Vec<ProvisionStepCompletion>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProvisionStepCompletion {
    pub step: ProvisionStepId,
    pub completed_at_unix_ms: u64,
}

impl ProvisionProgress {
    pub fn new(device_id: impl Into<String>, recipe_id: impl Into<String>) -> Self {
        Self {
            device_id: device_id.into(),
            recipe_id: recipe_id.into(),
            completed_steps: Vec::new(),
            completed_at_unix_ms: Vec::new(),
            last_error: None,
        }
    }

    pub fn mark_done(&mut self, step: ProvisionStepId) {
        let newly_completed = !self.completed_steps.contains(&step);
        if newly_completed {
            self.completed_steps.push(step);
        }
        if newly_completed
            || !self
                .completed_at_unix_ms
                .iter()
                .any(|completion| completion.step == step)
        {
            let completed_at_unix_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .min(u64::MAX as u128) as u64;
            self.completed_at_unix_ms.push(ProvisionStepCompletion {
                step,
                completed_at_unix_ms,
            });
        }
        self.last_error = None;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisionProgressStore {
    root: PathBuf,
}

impl ProvisionProgressStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path(&self, device_id: &str, recipe_id: &str) -> PathBuf {
        let key = format!("{device_id}\0{recipe_id}");
        let encoded = key
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        self.root
            .join("provisioning")
            .join(format!("{encoded}.json"))
    }

    pub fn load(&self, device_id: &str, recipe_id: &str) -> AppResult<ProvisionProgress> {
        let path = self.path(device_id, recipe_id);
        match std::fs::read(&path) {
            Ok(bytes) => {
                let progress: ProvisionProgress =
                    serde_json::from_slice(&bytes).map_err(|error| {
                        AppError::Io(format!("Invalid provisioning progress: {error}"))
                    })?;
                if progress.device_id != device_id || progress.recipe_id != recipe_id {
                    return Err(AppError::Io(
                        "Provisioning progress identity mismatch.".into(),
                    ));
                }
                Ok(progress)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(ProvisionProgress::new(device_id, recipe_id))
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn save(&self, progress: &ProvisionProgress) -> AppResult<()> {
        let path = self.path(&progress.device_id, &progress.recipe_id);
        let directory = path
            .parent()
            .ok_or_else(|| AppError::Io("Provisioning progress path has no parent.".into()))?;
        std::fs::create_dir_all(directory)?;
        let temporary_path = path.with_extension("tmp");
        let bytes =
            serde_json::to_vec_pretty(progress).map_err(|error| AppError::Io(error.to_string()))?;
        std::fs::write(&temporary_path, bytes)?;
        std::fs::rename(&temporary_path, &path)?;
        Ok(())
    }

    pub fn reset(&self, device_id: &str, recipe_id: &str) -> AppResult<()> {
        let path = self.path(device_id, recipe_id);
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ProvisionRunOutcome {
    PlanDone,
    StepDone {
        step: ProvisionStepId,
    },
    ConsentRequired {
        step: ProvisionStepId,
    },
    Blocked {
        step: ProvisionStepId,
        reason: String,
    },
    PhoneActionNeeded {
        step: ProvisionStepId,
        message: String,
    },
    Cancelled,
}

#[allow(clippy::too_many_arguments)]
pub async fn run_plan<E: ProvisionStepExecutor>(
    executor: &E,
    plan: &mut ProvisionPlan,
    progress: &mut ProvisionProgress,
    from_step: Option<ProvisionStepId>,
    only_step: bool,
    approved_steps: &[ProvisionStepId],
    cancel: CancellationToken,
    events: mpsc::Sender<ProvisionEvent>,
    store: &ProvisionProgressStore,
) -> AppResult<ProvisionRunOutcome> {
    if plan.device_id != progress.device_id || plan.recipe_id != progress.recipe_id {
        return Err(AppError::Config(
            "Provisioning plan and progress must belong to the same device and recipe.".into(),
        ));
    }
    let start_index = from_step
        .and_then(|id| plan.steps.iter().position(|step| step.id == id))
        .unwrap_or(0);
    let end_index = if only_step {
        start_index.saturating_add(1).min(plan.steps.len())
    } else {
        plan.steps.len()
    };

    for index in start_index..end_index {
        if cancel.is_cancelled() {
            plan.current_step = None;
            store.save(progress)?;
            return Ok(ProvisionRunOutcome::Cancelled);
        }
        let step = &mut plan.steps[index];
        plan.current_step = Some(step.id);
        let check = match executor.check(step.id).await {
            Ok(check) => check,
            Err(error) => {
                step.state = ProvisionStepState::Failed;
                step.detail = Some(error.user_message());
                progress.last_error = Some(error.user_message());
                plan.current_step = None;
                store.save(progress)?;
                let _ = events
                    .send(ProvisionEvent::StepFailed {
                        step: step.id,
                        error: error.to_payload(),
                    })
                    .await;
                return Err(error);
            }
        };
        match check {
            ProvisionCheck::Done => {
                step.state = ProvisionStepState::Done;
                step.detail = None;
                progress.mark_done(step.id);
                store.save(progress)?;
                let _ = events
                    .send(ProvisionEvent::StepDone { step: step.id })
                    .await;
                continue;
            }
            ProvisionCheck::DoneWithWarnings(warnings) => {
                for warning in warnings {
                    let _ = events
                        .send(ProvisionEvent::Output {
                            step: step.id,
                            event: StreamEvent::Stderr {
                                line: format!("Hermes doctor warning: {warning}"),
                            },
                        })
                        .await;
                }
                step.state = ProvisionStepState::Done;
                step.detail = None;
                progress.mark_done(step.id);
                store.save(progress)?;
                let _ = events
                    .send(ProvisionEvent::StepDone { step: step.id })
                    .await;
                continue;
            }
            ProvisionCheck::Blocked(reason) => {
                step.state = ProvisionStepState::Blocked;
                step.detail = Some(reason.clone());
                plan.current_step = None;
                store.save(progress)?;
                let _ = events
                    .send(ProvisionEvent::StepBlocked {
                        step: step.id,
                        reason: reason.clone(),
                    })
                    .await;
                return Ok(ProvisionRunOutcome::Blocked {
                    step: step.id,
                    reason,
                });
            }
            ProvisionCheck::Todo => {}
        }

        if let Some(consent) = step.consent.clone() {
            if !approved_steps.contains(&step.id) {
                step.state = ProvisionStepState::Todo;
                plan.current_step = None;
                let _ = events
                    .send(ProvisionEvent::ConsentRequired {
                        step: step.id,
                        consent,
                    })
                    .await;
                return Ok(ProvisionRunOutcome::ConsentRequired { step: step.id });
            }
        }

        step.state = ProvisionStepState::Running;
        step.detail = None;
        let _ = events
            .send(ProvisionEvent::StepStarted { step: step.id })
            .await;
        let result = executor
            .run(step.id, cancel.child_token(), events.clone())
            .await;
        match result {
            Ok(ProvisionStepRunOutcome::Done) => {
                step.state = ProvisionStepState::Done;
                progress.mark_done(step.id);
                store.save(progress)?;
                let _ = events
                    .send(ProvisionEvent::StepDone { step: step.id })
                    .await;
            }
            Ok(ProvisionStepRunOutcome::PhoneActionNeeded(message)) => {
                step.state = ProvisionStepState::PhoneActionNeeded;
                step.detail = Some(message.clone());
                plan.current_step = None;
                store.save(progress)?;
                let _ = events
                    .send(ProvisionEvent::PhoneActionNeeded {
                        step: step.id,
                        message: message.clone(),
                    })
                    .await;
                return Ok(ProvisionRunOutcome::PhoneActionNeeded {
                    step: step.id,
                    message,
                });
            }
            Err(AppError::Cancelled) if cancel.is_cancelled() => {
                step.state = ProvisionStepState::Todo;
                plan.current_step = None;
                store.save(progress)?;
                return Ok(ProvisionRunOutcome::Cancelled);
            }
            Err(error) => {
                step.state = ProvisionStepState::Failed;
                step.detail = Some(error.user_message());
                progress.last_error = Some(error.user_message());
                plan.current_step = None;
                store.save(progress)?;
                let _ = events
                    .send(ProvisionEvent::StepFailed {
                        step: step.id,
                        error: error.to_payload(),
                    })
                    .await;
                return Err(error);
            }
        }
    }
    plan.current_step = None;
    store.save(progress)?;
    if only_step {
        if let Some(step) = plan.steps.get(start_index) {
            return Ok(ProvisionRunOutcome::StepDone { step: step.id });
        }
    }
    let _ = events.send(ProvisionEvent::PlanDone).await;
    Ok(ProvisionRunOutcome::PlanDone)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;

    use super::*;
    use crate::provision::{ProvisionConsent, ProvisionStep};

    #[derive(Default)]
    struct MockExecutor {
        checks: Vec<(ProvisionStepId, ProvisionCheck)>,
        fail_check: Option<ProvisionStepId>,
        fail_step: Option<ProvisionStepId>,
        runs: Mutex<Vec<ProvisionStepId>>,
    }

    #[async_trait]
    impl ProvisionStepExecutor for MockExecutor {
        async fn check(&self, step: ProvisionStepId) -> AppResult<ProvisionCheck> {
            if self.fail_check == Some(step) {
                return Err(AppError::Config("simulated check failure".into()));
            }
            Ok(self
                .checks
                .iter()
                .find(|(checked, _)| *checked == step)
                .map(|(_, result)| result.clone())
                .unwrap_or(ProvisionCheck::Todo))
        }

        async fn run(
            &self,
            step: ProvisionStepId,
            _cancel: CancellationToken,
            _events: mpsc::Sender<ProvisionEvent>,
        ) -> AppResult<ProvisionStepRunOutcome> {
            self.runs.lock().unwrap().push(step);
            if self.fail_step == Some(step) {
                Err(AppError::Config("simulated step failure".into()))
            } else {
                Ok(ProvisionStepRunOutcome::Done)
            }
        }
    }

    fn step(id: ProvisionStepId, consent: Option<ProvisionConsent>) -> ProvisionStep {
        ProvisionStep {
            id,
            title: format!("{id:?}"),
            state: ProvisionStepState::Todo,
            detail: None,
            phone_action: None,
            consent,
        }
    }

    fn temp_root() -> PathBuf {
        static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "hacc-provision-tests-{}-{}",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        root
    }

    fn plan(steps: Vec<ProvisionStep>) -> ProvisionPlan {
        ProvisionPlan {
            serial: "192.0.2.10:5555".into(),
            device_id: "device-id".into(),
            recipe_id: "debian-official".into(),
            steps,
            current_step: None,
        }
    }

    #[tokio::test]
    async fn skips_completed_and_already_done_steps() {
        let first = ProvisionStepId::Preflight;
        let second = ProvisionStepId::InstallTermux;
        let executor = MockExecutor {
            checks: vec![
                (first, ProvisionCheck::Done),
                (second, ProvisionCheck::Todo),
            ],
            ..Default::default()
        };
        let mut plan = plan(vec![step(first, None), step(second, None)]);
        let mut progress = ProvisionProgress::new("device-id", "debian-official");
        let store = ProvisionProgressStore::new(temp_root());
        let (events, _receiver) = mpsc::channel(8);

        let outcome = run_plan(
            &executor,
            &mut plan,
            &mut progress,
            None,
            false,
            &[],
            CancellationToken::new(),
            events,
            &store,
        )
        .await
        .unwrap();

        assert_eq!(outcome, ProvisionRunOutcome::PlanDone);
        assert_eq!(*executor.runs.lock().unwrap(), vec![second]);
        assert_eq!(progress.completed_steps, vec![first, second]);
        assert!(plan
            .steps
            .iter()
            .all(|item| item.state == ProvisionStepState::Done));
    }

    #[tokio::test]
    async fn doctor_warnings_are_emitted_without_failing_the_completed_step() {
        let step_id = ProvisionStepId::ConfigureHermes;
        let executor = MockExecutor {
            checks: vec![(
                step_id,
                ProvisionCheck::DoneWithWarnings(vec![
                    "Warning: gateway service setup was skipped".into(),
                ]),
            )],
            ..Default::default()
        };
        let mut plan = plan(vec![step(step_id, None)]);
        let mut progress = ProvisionProgress::new("device-id", "debian-official");
        let store = ProvisionProgressStore::new(temp_root());
        let (events, mut received) = mpsc::channel(8);

        let outcome = run_plan(
            &executor,
            &mut plan,
            &mut progress,
            None,
            false,
            &[],
            CancellationToken::new(),
            events,
            &store,
        )
        .await
        .unwrap();

        assert_eq!(outcome, ProvisionRunOutcome::PlanDone);
        assert!(matches!(
            received.recv().await,
            Some(ProvisionEvent::Output {
                step: ProvisionStepId::ConfigureHermes,
                event: crate::transport::StreamEvent::Stderr { line },
            }) if line == "Hermes doctor warning: Warning: gateway service setup was skipped"
        ));
        assert!(matches!(
            received.recv().await,
            Some(ProvisionEvent::StepDone {
                step: ProvisionStepId::ConfigureHermes
            })
        ));
        assert!(matches!(
            received.recv().await,
            Some(ProvisionEvent::PlanDone)
        ));
    }

    #[tokio::test]
    async fn pauses_before_consent_and_resumes_when_approved() {
        let step_id = ProvisionStepId::InstallDistro;
        let consent = ProvisionConsent {
            title: "Install Debian packages".into(),
            commands: vec!["proot-distro install debian".into()],
            destructive: false,
        };
        let executor = MockExecutor::default();
        let mut plan = plan(vec![step(step_id, Some(consent))]);
        let mut progress = ProvisionProgress::new("device-id", "debian-official");
        let store = ProvisionProgressStore::new(temp_root());
        let (events, _receiver) = mpsc::channel(8);

        let outcome = run_plan(
            &executor,
            &mut plan,
            &mut progress,
            None,
            false,
            &[],
            CancellationToken::new(),
            events,
            &store,
        )
        .await
        .unwrap();
        assert_eq!(
            outcome,
            ProvisionRunOutcome::ConsentRequired { step: step_id }
        );
        assert!(executor.runs.lock().unwrap().is_empty());

        let (events, _receiver) = mpsc::channel(8);
        let outcome = run_plan(
            &executor,
            &mut plan,
            &mut progress,
            Some(step_id),
            false,
            &[step_id],
            CancellationToken::new(),
            events,
            &store,
        )
        .await
        .unwrap();
        assert_eq!(outcome, ProvisionRunOutcome::PlanDone);
        assert_eq!(*executor.runs.lock().unwrap(), vec![step_id]);
    }

    #[tokio::test]
    async fn stops_and_persists_failure_without_running_later_steps() {
        let failed = ProvisionStepId::BootstrapSsh;
        let later = ProvisionStepId::ConnectSsh;
        let executor = MockExecutor {
            fail_step: Some(failed),
            ..Default::default()
        };
        let mut plan = plan(vec![step(failed, None), step(later, None)]);
        let mut progress = ProvisionProgress::new("device-id", "debian-official");
        let store = ProvisionProgressStore::new(temp_root());
        let (events, _receiver) = mpsc::channel(8);

        let error = run_plan(
            &executor,
            &mut plan,
            &mut progress,
            None,
            false,
            &[],
            CancellationToken::new(),
            events,
            &store,
        )
        .await
        .unwrap_err();

        assert!(matches!(error, AppError::Config(_)));
        assert_eq!(*executor.runs.lock().unwrap(), vec![failed]);
        assert_eq!(plan.steps[0].state, ProvisionStepState::Failed);
        assert!(progress.last_error.is_some());
        assert_eq!(
            store.load("device-id", "debian-official").unwrap(),
            progress
        );
    }

    #[tokio::test]
    async fn check_failure_emits_failure_and_persists_without_running_steps() {
        let failed = ProvisionStepId::BootstrapSsh;
        let executor = MockExecutor {
            fail_check: Some(failed),
            ..Default::default()
        };
        let mut plan = plan(vec![
            step(failed, None),
            step(ProvisionStepId::ConnectSsh, None),
        ]);
        let mut progress = ProvisionProgress::new("device-id", "debian-official");
        let store = ProvisionProgressStore::new(temp_root());
        let (events, mut receiver) = mpsc::channel(8);
        assert!(run_plan(
            &executor,
            &mut plan,
            &mut progress,
            None,
            false,
            &[],
            CancellationToken::new(),
            events,
            &store,
        )
        .await
        .is_err());
        assert!(executor.runs.lock().unwrap().is_empty());
        assert_eq!(plan.steps[0].state, ProvisionStepState::Failed);
        assert_eq!(plan.current_step, None);
        assert!(
            matches!(receiver.recv().await, Some(ProvisionEvent::StepFailed { step, .. }) if step == failed)
        );
        assert_eq!(
            store.load("device-id", "debian-official").unwrap(),
            progress
        );
    }

    #[tokio::test]
    async fn cancellation_before_a_step_runs_nothing() {
        let step_id = ProvisionStepId::Preflight;
        let executor = MockExecutor::default();
        let mut plan = plan(vec![step(step_id, None)]);
        let mut progress = ProvisionProgress::new("device-id", "debian-official");
        let store = ProvisionProgressStore::new(temp_root());
        let (events, _receiver) = mpsc::channel(8);
        let cancel = CancellationToken::new();
        cancel.cancel();

        let outcome = run_plan(
            &executor,
            &mut plan,
            &mut progress,
            None,
            false,
            &[],
            cancel,
            events,
            &store,
        )
        .await
        .unwrap();

        assert_eq!(outcome, ProvisionRunOutcome::Cancelled);
        assert!(executor.runs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn single_step_run_does_not_execute_following_steps() {
        let first = ProvisionStepId::Preflight;
        let second = ProvisionStepId::InstallTermux;
        let executor = MockExecutor::default();
        let mut plan = plan(vec![step(first, None), step(second, None)]);
        let mut progress = ProvisionProgress::new("device-id", "debian-official");
        let store = ProvisionProgressStore::new(temp_root());
        let (events, _receiver) = mpsc::channel(8);

        let outcome = run_plan(
            &executor,
            &mut plan,
            &mut progress,
            Some(first),
            true,
            &[],
            CancellationToken::new(),
            events,
            &store,
        )
        .await
        .unwrap();

        assert_eq!(outcome, ProvisionRunOutcome::StepDone { step: first });
        assert_eq!(*executor.runs.lock().unwrap(), vec![first]);
        assert_eq!(plan.steps[1].state, ProvisionStepState::Todo);
    }

    #[test]
    fn progress_is_scoped_to_device_and_recipe_and_can_be_reset() {
        let store = ProvisionProgressStore::new(temp_root());
        let mut progress = ProvisionProgress::new("device-id", "debian-official");
        progress.mark_done(ProvisionStepId::Preflight);
        assert!(progress.completed_at_unix_ms[0].completed_at_unix_ms > 0);
        store.save(&progress).unwrap();
        assert_eq!(
            store.load("device-id", "debian-official").unwrap(),
            progress
        );
        assert!(store
            .load("other-device", "debian-official")
            .unwrap()
            .completed_steps
            .is_empty());
        assert!(store
            .load("device-id", "termux-native-apt")
            .unwrap()
            .completed_steps
            .is_empty());
        store.reset("device-id", "debian-official").unwrap();
        assert!(store
            .load("device-id", "debian-official")
            .unwrap()
            .completed_steps
            .is_empty());
    }
}
