use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, AppResult};

pub const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ReconnectConfig {
    pub enabled: bool,
    /// Delays between attempts in milliseconds.
    #[ts(type = "Array<number>")]
    pub schedule_ms: Vec<u64>,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            schedule_ms: vec![1_000, 2_000, 5_000, 10_000, 30_000, 30_000, 30_000, 30_000],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct HermesConfig {
    pub start_command: String,
    pub stop_command: String,
    pub restart_command: String,
    pub status_command: String,
    pub log_command: String,
    /// Pattern matched against process command lines to detect Hermes.
    pub process_match: String,
}

impl Default for HermesConfig {
    fn default() -> Self {
        Self {
            start_command: String::new(),
            stop_command: String::new(),
            restart_command: String::new(),
            status_command: String::new(),
            log_command: String::new(),
            process_match: "hermes-agent/hermes".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct LogsConfig {
    pub auto_start: bool,
    /// Arguments appended to `adb logcat -v threadtime`.
    pub logcat_filter: String,
}

impl Default for LogsConfig {
    fn default() -> Self {
        Self {
            auto_start: false,
            logcat_filter: "*:I".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct KnownAddress {
    pub address: String,
    pub auto_connect: bool,
}

impl Default for KnownAddress {
    fn default() -> Self {
        Self {
            address: String::new(),
            auto_connect: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum LogLevelSetting {
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct AppConfig {
    pub version: u32,
    #[ts(type = "string | null")]
    pub adb_path: Option<PathBuf>,
    pub known_addresses: Vec<KnownAddress>,
    pub reconnect: ReconnectConfig,
    pub hermes: HermesConfig,
    pub logs: LogsConfig,
    pub api_port: u16,
    pub log_level: LogLevelSetting,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            adb_path: None,
            known_addresses: Vec::new(),
            reconnect: ReconnectConfig::default(),
            hermes: HermesConfig::default(),
            logs: LogsConfig::default(),
            api_port: 8765,
            log_level: LogLevelSetting::Info,
        }
    }
}

impl AppConfig {
    /// Load from persisted JSON, migrating older schemas. Unknown/invalid input falls back to defaults.
    pub fn from_json(value: serde_json::Value) -> Self {
        let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut value = value;
        if version == 0 {
            migrate_v0(&mut value);
        }
        let mut cfg: AppConfig = serde_json::from_value(value).unwrap_or_default();
        cfg.version = CONFIG_VERSION;
        cfg
    }

    pub fn validate(&self) -> AppResult<()> {
        if self.api_port == 0 {
            return Err(AppError::Config(
                "API port must be between 1 and 65535.".into(),
            ));
        }
        if let Some(p) = &self.adb_path {
            if !p.as_os_str().is_empty() && !p.is_file() {
                return Err(AppError::Config(format!(
                    "ADB path does not exist: {}",
                    p.display()
                )));
            }
        }
        if self.reconnect.schedule_ms.iter().any(|ms| *ms < 500) {
            return Err(AppError::Config(
                "Reconnect delays must be at least 500 ms.".into(),
            ));
        }
        for k in &self.known_addresses {
            crate::adb::address::validate_address(&k.address)?;
        }
        Ok(())
    }

    pub fn remember_address(&mut self, address: &str) {
        if !self.known_addresses.iter().any(|k| k.address == address) {
            self.known_addresses.push(KnownAddress {
                address: address.to_string(),
                auto_connect: true,
            });
        }
    }
}

/// v0 (pre-release drafts) stored a single `lastDevice` + `autoConnect`.
fn migrate_v0(value: &mut serde_json::Value) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    let last = obj
        .remove("lastDevice")
        .and_then(|v| v.as_str().map(str::to_string));
    let auto = obj
        .remove("autoConnect")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    if let Some(addr) = last {
        obj.insert(
            "knownAddresses".into(),
            serde_json::json!([{ "address": addr, "autoConnect": auto }]),
        );
    }
    obj.insert("version".into(), serde_json::json!(CONFIG_VERSION));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_round_trips() {
        let cfg = AppConfig::default();
        let json = serde_json::to_value(&cfg).unwrap();
        assert_eq!(AppConfig::from_json(json), cfg);
    }

    #[test]
    fn migrates_v0_last_device() {
        let v0 = serde_json::json!({ "lastDevice": "192.0.2.10:5555", "autoConnect": false });
        let cfg = AppConfig::from_json(v0);
        assert_eq!(cfg.version, CONFIG_VERSION);
        assert_eq!(cfg.known_addresses.len(), 1);
        assert_eq!(cfg.known_addresses[0].address, "192.0.2.10:5555");
        assert!(!cfg.known_addresses[0].auto_connect);
    }

    #[test]
    fn garbage_falls_back_to_defaults() {
        let cfg = AppConfig::from_json(serde_json::json!({ "version": 1, "apiPort": "nope" }));
        assert_eq!(cfg, AppConfig::default());
    }

    #[test]
    fn validation_errors() {
        let cfg = AppConfig {
            api_port: 0,
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
        let cfg = AppConfig {
            adb_path: Some("/definitely/missing/adb".into()),
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
        let mut cfg = AppConfig::default();
        cfg.reconnect.schedule_ms = vec![10];
        assert!(cfg.validate().is_err());
        assert!(AppConfig::default().validate().is_ok());
    }

    #[test]
    fn remember_address_dedupes() {
        let mut cfg = AppConfig::default();
        cfg.remember_address("192.0.2.1:5555");
        cfg.remember_address("192.0.2.1:5555");
        assert_eq!(cfg.known_addresses.len(), 1);
    }
}
