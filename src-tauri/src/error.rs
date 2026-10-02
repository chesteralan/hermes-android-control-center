use serde::{Serialize, Serializer};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ErrorKind {
    AdbNotFound,
    AdbFailed,
    DeviceOffline,
    DeviceUnauthorized,
    DeviceNotFound,
    ConnectionRefused,
    WirelessDebuggingDisabled,
    PairingFailed,
    MdnsUnavailable,
    TermuxUnavailable,
    HermesNotFound,
    CommandFailed,
    Timeout,
    Cancelled,
    Config,
    Io,
}

/// Shape sent to the UI for every failed command.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ErrorPayload {
    pub kind: ErrorKind,
    pub message: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq)]
pub enum AppError {
    #[error("ADB was not found")]
    AdbNotFound { searched: Vec<String> },
    #[error("{message}")]
    AdbFailed {
        message: String,
        stderr: String,
        exit_code: Option<i32>,
    },
    #[error("Device {serial} is offline")]
    DeviceOffline { serial: String },
    #[error("Device {serial} is unauthorized")]
    DeviceUnauthorized { serial: String },
    #[error("Device {serial} was not found")]
    DeviceNotFound { serial: String },
    #[error("Connection refused by {address}")]
    ConnectionRefused { address: String, output: String },
    #[error("Wireless debugging appears to be disabled on {address}")]
    WirelessDebuggingDisabled { address: String, output: String },
    #[error("Pairing failed")]
    PairingFailed { output: String },
    #[error("mDNS discovery is unavailable")]
    MdnsUnavailable { output: String },
    #[error("Termux is unavailable: {reason}")]
    TermuxUnavailable { reason: String },
    #[error("Hermes process not found")]
    HermesNotFound,
    #[error("Command failed: {command}")]
    CommandFailed {
        command: String,
        exit_code: Option<i32>,
        stderr: String,
    },
    #[error("{operation} timed out after {after_ms} ms")]
    Timeout { operation: String, after_ms: u64 },
    #[error("Cancelled")]
    Cancelled,
    #[error("{0}")]
    Config(String),
    #[error("{0}")]
    Io(String),
}

impl AppError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::AdbNotFound { .. } => ErrorKind::AdbNotFound,
            Self::AdbFailed { .. } => ErrorKind::AdbFailed,
            Self::DeviceOffline { .. } => ErrorKind::DeviceOffline,
            Self::DeviceUnauthorized { .. } => ErrorKind::DeviceUnauthorized,
            Self::DeviceNotFound { .. } => ErrorKind::DeviceNotFound,
            Self::ConnectionRefused { .. } => ErrorKind::ConnectionRefused,
            Self::WirelessDebuggingDisabled { .. } => ErrorKind::WirelessDebuggingDisabled,
            Self::PairingFailed { .. } => ErrorKind::PairingFailed,
            Self::MdnsUnavailable { .. } => ErrorKind::MdnsUnavailable,
            Self::TermuxUnavailable { .. } => ErrorKind::TermuxUnavailable,
            Self::HermesNotFound => ErrorKind::HermesNotFound,
            Self::CommandFailed { .. } => ErrorKind::CommandFailed,
            Self::Timeout { .. } => ErrorKind::Timeout,
            Self::Cancelled => ErrorKind::Cancelled,
            Self::Config(_) => ErrorKind::Config,
            Self::Io(_) => ErrorKind::Io,
        }
    }

    /// Human-readable headline for the UI.
    pub fn user_message(&self) -> String {
        match self {
            Self::AdbNotFound { .. } => {
                "ADB is not installed or could not be found. Install Android platform-tools or set the ADB path in Settings.".into()
            }
            Self::AdbFailed { message, .. } => format!("ADB command failed: {message}"),
            Self::DeviceOffline { serial } => {
                format!("Device {serial} is offline. Wake the phone and check Wi-Fi.")
            }
            Self::DeviceUnauthorized { serial } => format!(
                "Device {serial} is unauthorized. Unlock the phone and accept the \"Allow debugging\" prompt."
            ),
            Self::DeviceNotFound { serial } => format!("Device {serial} is not connected."),
            Self::ConnectionRefused { address, .. } => format!(
                "Unable to connect to {address}: connection refused. The wireless debugging port may have changed."
            ),
            Self::WirelessDebuggingDisabled { address, .. } => format!(
                "Unable to connect to {address}. Wireless debugging may be disabled on the phone."
            ),
            Self::PairingFailed { .. } => {
                "Pairing failed. Check the pairing port and code (the code expires quickly).".into()
            }
            Self::MdnsUnavailable { .. } => {
                "Device discovery (mDNS) is unavailable on this computer or network. Use pairing code or IP instead.".into()
            }
            Self::TermuxUnavailable { reason } => format!("Termux is unavailable: {reason}"),
            Self::HermesNotFound => "The Hermes process was not found.".into(),
            Self::CommandFailed { command, exit_code, .. } => match exit_code {
                Some(code) => format!("Command \"{command}\" failed with exit code {code}."),
                None => format!("Command \"{command}\" failed."),
            },
            Self::Timeout { operation, after_ms } => {
                format!("{operation} timed out after {:.1} s.", *after_ms as f64 / 1000.0)
            }
            Self::Cancelled => "The operation was cancelled.".into(),
            Self::Config(msg) => msg.clone(),
            Self::Io(msg) => format!("I/O error: {msg}"),
        }
    }

    /// Technical details for the expandable "Details" section.
    pub fn details(&self) -> Option<String> {
        match self {
            Self::AdbNotFound { searched } => Some(format!("Searched:\n{}", searched.join("\n"))),
            Self::AdbFailed {
                stderr, exit_code, ..
            } => Some(format!("exit code: {exit_code:?}\n{stderr}")),
            Self::ConnectionRefused { output, .. }
            | Self::WirelessDebuggingDisabled { output, .. }
            | Self::PairingFailed { output }
            | Self::MdnsUnavailable { output } => Some(format!("ADB returned:\n{output}")),
            Self::CommandFailed { stderr, .. } if !stderr.is_empty() => Some(stderr.clone()),
            _ => None,
        }
    }

    pub fn to_payload(&self) -> ErrorPayload {
        ErrorPayload {
            kind: self.kind(),
            message: self.user_message(),
            details: self.details(),
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_payload().serialize(serializer)
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    fn all_variants() -> Vec<AppError> {
        vec![
            AppError::AdbNotFound {
                searched: vec!["/x".into()],
            },
            AppError::AdbFailed {
                message: "m".into(),
                stderr: "e".into(),
                exit_code: Some(1),
            },
            AppError::DeviceOffline { serial: "s".into() },
            AppError::DeviceUnauthorized { serial: "s".into() },
            AppError::DeviceNotFound { serial: "s".into() },
            AppError::ConnectionRefused {
                address: "a".into(),
                output: "o".into(),
            },
            AppError::WirelessDebuggingDisabled {
                address: "a".into(),
                output: "o".into(),
            },
            AppError::PairingFailed { output: "o".into() },
            AppError::MdnsUnavailable { output: "o".into() },
            AppError::TermuxUnavailable { reason: "r".into() },
            AppError::HermesNotFound,
            AppError::CommandFailed {
                command: "c".into(),
                exit_code: Some(2),
                stderr: "e".into(),
            },
            AppError::Timeout {
                operation: "op".into(),
                after_ms: 1500,
            },
            AppError::Cancelled,
            AppError::Config("bad".into()),
            AppError::Io("io".into()),
        ]
    }

    #[test]
    fn every_variant_serializes_with_message() {
        for e in all_variants() {
            let v = serde_json::to_value(&e).unwrap();
            assert!(v["kind"].is_string(), "{e:?}");
            assert!(!v["message"].as_str().unwrap().is_empty(), "{e:?}");
        }
    }

    #[test]
    fn connection_refused_has_adb_output_in_details() {
        let e = AppError::ConnectionRefused {
            address: "1.2.3.4:5".into(),
            output: "refused".into(),
        };
        assert_eq!(e.kind(), ErrorKind::ConnectionRefused);
        assert!(e.details().unwrap().contains("refused"));
        assert!(e.user_message().contains("1.2.3.4:5"));
    }
}
