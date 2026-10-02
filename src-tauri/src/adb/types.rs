use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum DeviceState {
    Device,
    Offline,
    Unauthorized,
    Authorizing,
    Connecting,
    NoPermissions,
    Recovery,
    Bootloader,
    Sideload,
    Disconnected,
    Reconnecting,
    Unknown,
}

impl DeviceState {
    pub fn from_adb(raw: &str) -> Self {
        match raw {
            "device" => Self::Device,
            "offline" => Self::Offline,
            "unauthorized" => Self::Unauthorized,
            "authorizing" => Self::Authorizing,
            "connecting" => Self::Connecting,
            "recovery" => Self::Recovery,
            "bootloader" => Self::Bootloader,
            "sideload" => Self::Sideload,
            s if s.starts_with("no permissions") => Self::NoPermissions,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ConnectionType {
    Usb,
    /// `ip:port` serial from `adb connect`.
    WirelessIp,
    /// `adb-<serialno>-<suffix>._adb-tls-connect._tcp`, auto-connected by adb via mDNS.
    WirelessMdns,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AndroidDevice {
    pub serial: String,
    /// Stable hardware identity (`ro.serialno`), when known.
    pub device_id: Option<String>,
    pub model: Option<String>,
    pub product: Option<String>,
    pub device: Option<String>,
    pub transport_id: Option<String>,
    pub ip_address: Option<String>,
    pub state: DeviceState,
    pub raw_state: String,
    pub connection: ConnectionType,
}

impl AndroidDevice {
    pub fn is_wireless(&self) -> bool {
        self.connection != ConnectionType::Usb
    }

    /// Identity used by the UI: device_id when known, else serial.
    pub fn key(&self) -> &str {
        self.device_id.as_deref().unwrap_or(&self.serial)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdbInfo {
    pub path: String,
    pub version: String,
    pub revision: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MdnsServiceKind {
    Connect,
    Pairing,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MdnsService {
    pub name: String,
    pub service_type: String,
    pub kind: MdnsServiceKind,
    pub address: String,
    pub ip: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BatteryInfo {
    pub level: Option<u8>,
    pub charging: bool,
    pub status: String,
    pub plugged: Option<String>,
    pub temperature_c: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StorageInfo {
    #[ts(type = "number")]
    pub total_bytes: u64,
    #[ts(type = "number")]
    pub free_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MemoryInfo {
    #[ts(type = "number")]
    pub total_bytes: u64,
    #[ts(type = "number")]
    pub available_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CpuInfo {
    pub abi: Option<String>,
    pub cores: Option<u32>,
    pub hardware: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TermuxSource {
    PlayStore,
    FDroid,
    Sideloaded,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TermuxPackageInfo {
    pub installed: bool,
    pub version_name: Option<String>,
    #[ts(type = "number | null")]
    pub version_code: Option<u64>,
    pub installer: Option<String>,
    pub source: TermuxSource,
    pub companions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DeviceInfo {
    pub serial: String,
    pub device_id: Option<String>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub android_version: Option<String>,
    pub sdk: Option<u32>,
    pub ip_address: Option<String>,
    pub battery: Option<BatteryInfo>,
    pub storage: Option<StorageInfo>,
    pub memory: Option<MemoryInfo>,
    pub cpu: Option<CpuInfo>,
    pub termux: Option<TermuxPackageInfo>,
}
