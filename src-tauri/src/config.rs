use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, AppResult};

pub const CONFIG_VERSION: u32 = 3;

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

/// Where Hermes is installed (ADR-013). Transport (how we reach Termux) is separate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum HermesEnvironment {
    Termux,
    ProotDistro { distro: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum StartMode {
    /// App-installed supervisor relaunches the gateway (ADR-015).
    Supervised,
    /// Fire-and-forget `nohup setsid`.
    Detached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum HermesTransportKind {
    TermuxSsh,
    ControlApi,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct HermesConfig {
    pub environment: HermesEnvironment,
    pub start_mode: StartMode,
    pub transport: HermesTransportKind,
    pub fallback_to_ssh: bool,
    /// Run inside the environment, e.g. `hermes gateway run`.
    pub gateway_command: String,
    /// Matches any Hermes process command line (gateway or CLI).
    pub process_match: String,
    /// Extra text identifying the gateway among Hermes processes.
    pub gateway_match: String,
    /// HERMES_HOME inside the environment.
    pub hermes_home: String,
    /// Log paths inside the Hermes environment; gateway first, tool calls second.
    pub log_files: Vec<String>,
    /// Prepended to PATH inside the environment (installers often only update interactive shells).
    pub path_prepend: Vec<String>,
    /// Optional overrides; when set they replace the built-in supervisor actions.
    pub start_command: String,
    pub stop_command: String,
    pub restart_command: String,
    pub status_command: String,
    pub log_command: String,
    pub version_command: String,
    pub doctor_command: String,
    pub update_command: String,
}

impl Default for HermesConfig {
    fn default() -> Self {
        Self {
            environment: HermesEnvironment::ProotDistro {
                distro: "debian".into(),
            },
            start_mode: StartMode::Supervised,
            transport: HermesTransportKind::TermuxSsh,
            fallback_to_ssh: false,
            gateway_command: "hermes gateway run".into(),
            process_match: "hermes-agent/venv/bin/python".into(),
            gateway_match: "gateway run".into(),
            hermes_home: "/root/.hermes".into(),
            log_files: vec![
                "/root/.hermes/logs/gateway.log".into(),
                "/root/.hermes/logs/tool_calls.log".into(),
            ],
            path_prepend: vec!["/root/.local/bin".into()],
            start_command: String::new(),
            stop_command: String::new(),
            restart_command: String::new(),
            status_command: String::new(),
            log_command: String::new(),
            version_command: "hermes --version".into(),
            doctor_command: "hermes doctor".into(),
            update_command: "hermes update".into(),
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
pub struct TermuxConfig {
    /// Termux sshd ignores the user name, but it must be non-empty.
    pub ssh_user: String,
    /// sshd port inside Termux (bound to 127.0.0.1, reached via adb forward).
    pub ssh_port: u16,
}

impl Default for TermuxConfig {
    fn default() -> Self {
        Self {
            ssh_user: "termux".into(),
            ssh_port: 8022,
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
pub struct DeviceProfile {
    pub device_id: String,
    pub alias: Option<String>,
    pub color: Option<String>,
    pub last_address: Option<String>,
    pub auto_connect: bool,
    pub hermes: Option<HermesConfig>,
    pub termux: Option<TermuxConfig>,
    pub log_command: Option<String>,
}

impl Default for DeviceProfile {
    fn default() -> Self {
        Self {
            device_id: String::new(),
            alias: None,
            color: None,
            last_address: None,
            auto_connect: true,
            hermes: None,
            termux: None,
            log_command: None,
        }
    }
}

impl DeviceProfile {
    pub fn apply_update(&mut self, update: DeviceProfileUpdate) {
        if let Some(alias) = update.alias {
            self.alias = alias;
        }
        if let Some(color) = update.color {
            self.color = color;
        }
        if let Some(last_address) = update.last_address {
            self.last_address = last_address;
        }
        if let Some(auto_connect) = update.auto_connect {
            self.auto_connect = auto_connect;
        }
        if let Some(hermes) = update.hermes {
            self.hermes = hermes;
        }
        if let Some(termux) = update.termux {
            self.termux = termux;
        }
        if let Some(log_command) = update.log_command {
            self.log_command = log_command;
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct DeviceProfileUpdate {
    #[serde(default, deserialize_with = "deserialize_double_option")]
    #[ts(type = "string | null | undefined")]
    pub alias: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    #[ts(type = "string | null | undefined")]
    pub color: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    #[ts(type = "string | null | undefined")]
    pub last_address: Option<Option<String>>,
    #[ts(type = "boolean | undefined")]
    pub auto_connect: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub hermes: Option<Option<HermesConfig>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub termux: Option<Option<TermuxConfig>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    #[ts(type = "string | null | undefined")]
    pub log_command: Option<Option<String>>,
}

fn deserialize_double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EffectiveDeviceConfig {
    pub device_id: String,
    #[ts(type = "string | null")]
    pub alias: Option<String>,
    #[ts(type = "string | null")]
    pub color: Option<String>,
    #[ts(type = "string | null")]
    pub last_address: Option<String>,
    pub auto_connect: bool,
    pub hermes: HermesConfig,
    pub termux: TermuxConfig,
    pub log_command: String,
    pub has_hermes_override: bool,
    pub has_termux_override: bool,
    pub has_log_command_override: bool,
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
    pub termux: TermuxConfig,
    pub api_port: u16,
    pub log_level: LogLevelSetting,
    pub devices: HashMap<String, DeviceProfile>,
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
            termux: TermuxConfig::default(),
            api_port: 8765,
            log_level: LogLevelSetting::Info,
            devices: HashMap::new(),
        }
    }
}

fn validate_hermes_config(hermes: &HermesConfig) -> AppResult<()> {
    if let HermesEnvironment::ProotDistro { distro } = &hermes.environment {
        let ok = !distro.is_empty()
            && distro
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !ok {
            return Err(AppError::Config(format!("Invalid distro name: {distro:?}")));
        }
    }
    if hermes.process_match.trim().is_empty() || hermes.gateway_command.trim().is_empty() {
        return Err(AppError::Config(
            "Hermes process match and gateway command are required.".into(),
        ));
    }
    Ok(())
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
        // v1 default didn't match Hermes' real argv (`.../venv/bin/python /root/.local/bin/hermes`).
        if version < 2 && cfg.hermes.process_match == "hermes-agent/hermes" {
            cfg.hermes.process_match = HermesConfig::default().process_match;
        }
        if version < 3 {
            migrate_v2_to_v3(&mut cfg);
        }
        cfg.version = CONFIG_VERSION;
        cfg
    }

    pub fn validate(&self) -> AppResult<()> {
        validate_hermes_config(&self.hermes)?;
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
        for (id, profile) in &self.devices {
            if id.trim().is_empty() {
                return Err(AppError::Config("Device profile ID cannot be empty.".into()));
            }
            if let Some(hermes) = &profile.hermes {
                validate_hermes_config(hermes)?;
            }
            if let Some(addr) = &profile.last_address {
                crate::adb::address::validate_address(addr)?;
            }
        }
        Ok(())
    }

    pub fn effective_config(&self, device_id: &str) -> EffectiveDeviceConfig {
        let profile = self.devices.get(device_id);
        let alias = profile.and_then(|p| p.alias.clone());
        let color = profile.and_then(|p| p.color.clone());
        let last_address = profile.and_then(|p| p.last_address.clone());
        let auto_connect = profile.map(|p| p.auto_connect).unwrap_or(true);
        let has_hermes_override = profile.and_then(|p| p.hermes.as_ref()).is_some();
        let has_termux_override = profile.and_then(|p| p.termux.as_ref()).is_some();
        let has_log_command_override = profile.and_then(|p| p.log_command.as_ref()).is_some();

        let hermes = profile
            .and_then(|p| p.hermes.clone())
            .unwrap_or_else(|| self.hermes.clone());
        let termux = profile
            .and_then(|p| p.termux.clone())
            .unwrap_or_else(|| self.termux.clone());
        let log_command = profile
            .and_then(|p| p.log_command.clone())
            .filter(|cmd| !cmd.trim().is_empty())
            .unwrap_or_else(|| self.hermes.log_command.clone());

        EffectiveDeviceConfig {
            device_id: device_id.to_string(),
            alias,
            color,
            last_address,
            auto_connect,
            hermes,
            termux,
            log_command,
            has_hermes_override,
            has_termux_override,
            has_log_command_override,
        }
    }

    pub fn effective_hermes_config(&self, device_id: &str) -> HermesConfig {
        self.devices
            .get(device_id)
            .and_then(|p| p.hermes.clone())
            .unwrap_or_else(|| self.hermes.clone())
    }

    pub fn effective_termux_config(&self, device_id: &str) -> TermuxConfig {
        self.devices
            .get(device_id)
            .and_then(|p| p.termux.clone())
            .unwrap_or_else(|| self.termux.clone())
    }

    pub fn remember_address(&mut self, address: &str) {
        if !self.known_addresses.iter().any(|k| k.address == address) {
            self.known_addresses.push(KnownAddress {
                address: address.to_string(),
                auto_connect: true,
            });
        }
        let device_id = crate::adb::parse::device_id_from_serial(address)
            .unwrap_or_else(|| address.to_string());
        let profile = self
            .devices
            .entry(device_id.clone())
            .or_insert_with(|| DeviceProfile {
                device_id,
                ..Default::default()
            });
        profile.last_address = Some(address.to_string());
    }
}

/// v2 migrated knownAddresses into per-device profiles in v3.
fn migrate_v2_to_v3(cfg: &mut AppConfig) {
    for known in &cfg.known_addresses {
        let device_id = crate::adb::parse::device_id_from_serial(&known.address)
            .unwrap_or_else(|| known.address.clone());
        if !cfg.devices.contains_key(&device_id) {
            cfg.devices.insert(
                device_id.clone(),
                DeviceProfile {
                    device_id,
                    alias: None,
                    color: None,
                    last_address: Some(known.address.clone()),
                    auto_connect: known.auto_connect,
                    hermes: None,
                    termux: None,
                    log_command: None,
                },
            );
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
    fn migrates_v1_hermes_defaults_and_preserves_custom_commands() {
        let v1 = serde_json::json!({
            "version": 1,
            "hermes": {
                "startCommand": "custom-start",
                "stopCommand": "custom-stop",
                "restartCommand": "",
                "statusCommand": "custom-status",
                "logCommand": "custom-log",
                "processMatch": "hermes-agent/hermes"
            }
        });
        let cfg = AppConfig::from_json(v1);
        assert_eq!(cfg.version, CONFIG_VERSION);
        assert_eq!(
            cfg.hermes.process_match,
            HermesConfig::default().process_match
        );
        assert_eq!(cfg.hermes.start_command, "custom-start");
        assert_eq!(cfg.hermes.stop_command, "custom-stop");
        assert_eq!(cfg.hermes.status_command, "custom-status");
        assert_eq!(cfg.hermes.log_files, HermesConfig::default().log_files);
        assert_eq!(
            cfg.hermes.environment,
            HermesEnvironment::ProotDistro {
                distro: "debian".into()
            }
        );
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
        assert_eq!(cfg.devices.len(), 1);
    }

    #[test]
    fn migrates_v2_known_addresses_to_device_profiles() {
        let v2 = serde_json::json!({
            "version": 2,
            "knownAddresses": [
                { "address": "192.0.2.10:5555", "autoConnect": true },
                { "address": "192.0.2.20:5555", "autoConnect": false },
            ]
        });
        let cfg = AppConfig::from_json(v2);
        assert_eq!(cfg.version, CONFIG_VERSION);
        // Both addresses must have become device profiles.
        assert_eq!(cfg.devices.len(), 2);
        let profile = cfg
            .devices
            .values()
            .find(|p| p.last_address.as_deref() == Some("192.0.2.20:5555"))
            .expect("missing profile for 192.0.2.20:5555");
        assert!(!profile.auto_connect);
        // knownAddresses are preserved (v3 keeps the list for ADB reconnect purposes).
        assert_eq!(cfg.known_addresses.len(), 2);
    }

    #[test]
    fn migrates_v2_does_not_overwrite_existing_profile() {
        // If a device profile already exists (e.g. from a partial migration), migration must
        // not silently replace it.
        let mut cfg = AppConfig {
            version: 2,
            known_addresses: vec![KnownAddress {
                address: "192.0.2.30:5555".into(),
                auto_connect: false,
            }],
            ..Default::default()
        };
        let device_id =
            crate::adb::parse::device_id_from_serial("192.0.2.30:5555").unwrap_or_else(|| "192.0.2.30:5555".into());
        cfg.devices.insert(
            device_id.clone(),
            DeviceProfile {
                device_id: device_id.clone(),
                alias: Some("pre-existing".into()),
                auto_connect: true,
                ..Default::default()
            },
        );
        migrate_v2_to_v3(&mut cfg);
        // The pre-existing profile must be unchanged.
        let profile = cfg.devices.get(&device_id).unwrap();
        assert_eq!(profile.alias.as_deref(), Some("pre-existing"));
        assert!(profile.auto_connect);
    }

    #[test]
    fn effective_config_inherits_globals_when_no_override() {
        let cfg = AppConfig::default();
        let eff = cfg.effective_config("unknown-device");
        assert_eq!(eff.hermes, cfg.hermes);
        assert_eq!(eff.termux, cfg.termux);
        assert_eq!(eff.log_command, cfg.hermes.log_command);
        assert!(!eff.has_hermes_override);
        assert!(!eff.has_termux_override);
        assert!(!eff.has_log_command_override);
        assert!(eff.auto_connect);
    }

    #[test]
    fn effective_config_prefers_per_device_override() {
        let mut cfg = AppConfig::default();
        let mut override_hermes = cfg.hermes.clone();
        override_hermes.gateway_command = "hermes gateway run --verbose".into();
        let mut override_termux = cfg.termux.clone();
        override_termux.ssh_port = 9022;
        cfg.devices.insert(
            "phone-a".into(),
            DeviceProfile {
                device_id: "phone-a".into(),
                alias: Some("My Phone".into()),
                color: Some("#ff0000".into()),
                auto_connect: false,
                hermes: Some(override_hermes.clone()),
                termux: Some(override_termux.clone()),
                log_command: Some("tail -n 100 /root/.hermes/logs/gateway.log".into()),
                ..Default::default()
            },
        );
        let eff = cfg.effective_config("phone-a");
        assert_eq!(eff.hermes, override_hermes);
        assert_eq!(eff.termux, override_termux);
        assert_eq!(eff.log_command, "tail -n 100 /root/.hermes/logs/gateway.log");
        assert!(eff.has_hermes_override);
        assert!(eff.has_termux_override);
        assert!(eff.has_log_command_override);
        assert!(!eff.auto_connect);
        assert_eq!(eff.alias.as_deref(), Some("My Phone"));
        assert_eq!(eff.color.as_deref(), Some("#ff0000"));
    }

    #[test]
    fn effective_config_falls_back_to_global_log_command() {
        let mut cfg = AppConfig::default();
        cfg.hermes.log_command = "tail -f /global.log".into();
        cfg.devices.insert(
            "phone-b".into(),
            DeviceProfile {
                device_id: "phone-b".into(),
                // log_command is None → fall back to global
                log_command: None,
                ..Default::default()
            },
        );
        let eff = cfg.effective_config("phone-b");
        assert_eq!(eff.log_command, "tail -f /global.log");
        assert!(!eff.has_log_command_override);
    }

    #[test]
    fn apply_update_partial_fields_preserved() {
        let mut profile = DeviceProfile {
            device_id: "phone-c".into(),
            alias: Some("Old name".into()),
            color: Some("#blue".into()),
            auto_connect: true,
            ..Default::default()
        };
        // Only update alias; all other fields must stay.
        profile.apply_update(DeviceProfileUpdate {
            alias: Some(Some("New name".into())),
            ..Default::default()
        });
        assert_eq!(profile.alias.as_deref(), Some("New name"));
        assert_eq!(profile.color.as_deref(), Some("#blue"));
        assert!(profile.auto_connect);
    }

    #[test]
    fn apply_update_can_clear_optional_fields() {
        let mut profile = DeviceProfile {
            device_id: "phone-d".into(),
            alias: Some("Will clear".into()),
            color: Some("#green".into()),
            ..Default::default()
        };
        profile.apply_update(DeviceProfileUpdate {
            alias: Some(None),  // explicitly set to null
            color: Some(None),
            ..Default::default()
        });
        assert!(profile.alias.is_none());
        assert!(profile.color.is_none());
    }

    #[test]
    fn validation_rejects_empty_device_profile_id() {
        let mut cfg = AppConfig::default();
        cfg.devices.insert(
            String::new(),
            DeviceProfile {
                device_id: String::new(),
                ..Default::default()
            },
        );
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validation_accepts_valid_device_profiles() {
        let mut cfg = AppConfig::default();
        cfg.devices.insert(
            "valid-id".into(),
            DeviceProfile {
                device_id: "valid-id".into(),
                alias: Some("Test Phone".into()),
                ..Default::default()
            },
        );
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn migrates_v2_known_addresses_to_device_profiles_with_auto_connect() {
        let v2 = serde_json::json!({
            "version": 2,
            "knownAddresses": [
                { "address": "192.168.1.50:5555", "autoConnect": true },
                { "address": "adb-R58M123456-suffix._adb-tls-connect._tcp", "autoConnect": false }
            ]
        });
        let cfg = AppConfig::from_json(v2);
        assert_eq!(cfg.version, CONFIG_VERSION);
        assert_eq!(cfg.devices.len(), 2);

        let p1 = cfg.devices.get("192.168.1.50:5555").unwrap();
        assert_eq!(p1.last_address.as_deref(), Some("192.168.1.50:5555"));
        assert!(p1.auto_connect);

        let p2 = cfg.devices.get("R58M123456").unwrap();
        assert_eq!(
            p2.last_address.as_deref(),
            Some("adb-R58M123456-suffix._adb-tls-connect._tcp")
        );
        assert!(!p2.auto_connect);
    }

    #[test]
    fn effective_config_merge_precedence() {
        let mut cfg = AppConfig::default();
        cfg.hermes.gateway_command = "global gateway".into();
        cfg.termux.ssh_port = 8022;
        cfg.hermes.log_command = "global log".into();

        // Unknown device gets global defaults
        let eff = cfg.effective_config("unknown-dev");
        assert_eq!(eff.device_id, "unknown-dev");
        assert_eq!(eff.hermes.gateway_command, "global gateway");
        assert_eq!(eff.termux.ssh_port, 8022);
        assert_eq!(eff.log_command, "global log");
        assert!(!eff.has_hermes_override);
        assert!(!eff.has_termux_override);
        assert!(!eff.has_log_command_override);

        // Device with overrides
        let mut profile = DeviceProfile {
            device_id: "phone-1".into(),
            alias: Some("Pixel 7".into()),
            color: Some("#ff0000".into()),
            last_address: Some("192.168.1.99:5555".into()),
            auto_connect: false,
            hermes: Some(HermesConfig {
                gateway_command: "override gateway".into(),
                ..Default::default()
            }),
            termux: Some(TermuxConfig {
                ssh_port: 9022,
                ..Default::default()
            }),
            log_command: Some("override log".into()),
        };
        cfg.devices.insert("phone-1".into(), profile.clone());

        let eff = cfg.effective_config("phone-1");
        assert_eq!(eff.alias.as_deref(), Some("Pixel 7"));
        assert_eq!(eff.color.as_deref(), Some("#ff0000"));
        assert_eq!(eff.last_address.as_deref(), Some("192.168.1.99:5555"));
        assert!(!eff.auto_connect);
        assert_eq!(eff.hermes.gateway_command, "override gateway");
        assert_eq!(eff.termux.ssh_port, 9022);
        assert_eq!(eff.log_command, "override log");
        assert!(eff.has_hermes_override);
        assert!(eff.has_termux_override);
        assert!(eff.has_log_command_override);

        // Clear overrides back to None
        profile.hermes = None;
        profile.termux = None;
        profile.log_command = None;
        cfg.devices.insert("phone-1".into(), profile);

        let eff = cfg.effective_config("phone-1");
        assert_eq!(eff.hermes.gateway_command, "global gateway");
        assert_eq!(eff.termux.ssh_port, 8022);
        assert_eq!(eff.log_command, "global log");
        assert!(!eff.has_hermes_override);
        assert!(!eff.has_termux_override);
        assert!(!eff.has_log_command_override);
    }

    #[test]
    fn partial_update_preserves_unrelated_fields() {
        let mut profile = DeviceProfile {
            device_id: "dev-abc".into(),
            alias: Some("Old Alias".into()),
            color: Some("#112233".into()),
            last_address: Some("192.168.1.2:5555".into()),
            auto_connect: true,
            hermes: Some(HermesConfig::default()),
            termux: None,
            log_command: Some("tail log".into()),
        };

        // Partial update: only change alias and color
        let update_json = serde_json::json!({
            "alias": "New Alias",
            "color": "#445566"
        });
        let update: DeviceProfileUpdate = serde_json::from_value(update_json).unwrap();
        profile.apply_update(update);

        assert_eq!(profile.alias.as_deref(), Some("New Alias"));
        assert_eq!(profile.color.as_deref(), Some("#445566"));
        // Unrelated fields preserved
        assert_eq!(profile.last_address.as_deref(), Some("192.168.1.2:5555"));
        assert!(profile.auto_connect);
        assert!(profile.hermes.is_some());
        assert_eq!(profile.log_command.as_deref(), Some("tail log"));

        // Partial update: explicitly clear alias and hermes override
        let clear_json = serde_json::json!({
            "alias": null,
            "hermes": null
        });
        let clear_update: DeviceProfileUpdate = serde_json::from_value(clear_json).unwrap();
        profile.apply_update(clear_update);

        assert_eq!(profile.alias, None);
        assert_eq!(profile.hermes, None);
        // Color and others still preserved
        assert_eq!(profile.color.as_deref(), Some("#445566"));
        assert_eq!(profile.log_command.as_deref(), Some("tail log"));
    }
}
