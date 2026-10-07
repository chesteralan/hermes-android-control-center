use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::ErrorPayload;
use crate::transport::StreamEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ProvisionStepId {
    Preflight,
    InstallTermux,
    AndroidSettings,
    LaunchTermux,
    BootstrapSsh,
    ConnectSsh,
    TermuxPackages,
    InstallDistro,
    InstallHermes,
    ConfigureHermes,
    Autostart,
    VerifyAndStart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ProvisionStepState {
    Done,
    Todo,
    Running,
    PhoneActionNeeded,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProvisionConsent {
    pub title: String,
    pub commands: Vec<String>,
    pub destructive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProvisionStep {
    pub id: ProvisionStepId,
    pub title: String,
    pub state: ProvisionStepState,
    pub detail: Option<String>,
    pub phone_action: Option<String>,
    pub consent: Option<ProvisionConsent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProvisionPlan {
    pub serial: String,
    pub device_id: String,
    pub recipe_id: String,
    pub steps: Vec<ProvisionStep>,
    pub current_step: Option<ProvisionStepId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ProvisionEvent {
    StepStarted {
        step: ProvisionStepId,
    },
    Output {
        step: ProvisionStepId,
        event: StreamEvent,
    },
    PhoneActionNeeded {
        step: ProvisionStepId,
        message: String,
    },
    StepDone {
        step: ProvisionStepId,
    },
    StepBlocked {
        step: ProvisionStepId,
        reason: String,
    },
    ConsentRequired {
        step: ProvisionStepId,
        consent: ProvisionConsent,
    },
    StepFailed {
        step: ProvisionStepId,
        error: ErrorPayload,
    },
    PlanDone,
}
