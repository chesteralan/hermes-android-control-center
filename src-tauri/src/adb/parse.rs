//! Pure parsers for adb output. None of these may panic on any input.

use std::collections::HashMap;

use super::address::split_host_port;
use super::types::*;
use crate::error::AppError;

const MDNS_CONNECT: &str = "._adb-tls-connect._tcp";

pub fn connection_type(serial: &str) -> ConnectionType {
    if serial.contains("._adb-tls-connect._tcp")
        || serial.starts_with("adb-") && serial.contains("._tcp")
    {
        ConnectionType::WirelessMdns
    } else if split_host_port(serial).is_some_and(|(h, _)| h.contains('.') || h.contains(':')) {
        ConnectionType::WirelessIp
    } else {
        ConnectionType::Usb
    }
}

/// `adb-<serialno>-<suffix>._adb-tls-connect._tcp` → `<serialno>`; USB serial → itself.
pub fn device_id_from_serial(serial: &str) -> Option<String> {
    match connection_type(serial) {
        ConnectionType::Usb => Some(serial.to_string()),
        ConnectionType::WirelessIp => None,
        ConnectionType::WirelessMdns => mdns_instance_serialno(serial.split(MDNS_CONNECT).next()?),
    }
}

/// `adb-<serialno>-<suffix>` → `<serialno>`.
pub fn mdns_instance_serialno(instance: &str) -> Option<String> {
    let rest = instance.strip_prefix("adb-")?;
    let (id, _suffix) = rest.rsplit_once('-')?;
    (!id.is_empty()).then(|| id.to_string())
}

fn parse_device_line(line: &str) -> Option<AndroidDevice> {
    let mut tokens = line.split_whitespace();
    let serial = tokens.next()?.to_string();
    let rest: Vec<&str> = tokens.collect();
    if rest.is_empty() {
        return None;
    }
    let kv_start = rest
        .iter()
        .position(|t| t.contains(':') && !t.starts_with('(') && !t.ends_with(')'))
        .unwrap_or(rest.len());
    let raw_state = rest[..kv_start].join(" ");
    let mut props: HashMap<&str, &str> = HashMap::new();
    for t in &rest[kv_start..] {
        if let Some((k, v)) = t.split_once(':') {
            props.insert(k, v);
        }
    }
    let connection = connection_type(&serial);
    let ip_address = match connection {
        ConnectionType::WirelessIp => split_host_port(&serial).map(|(h, _)| h),
        _ => None,
    };
    Some(AndroidDevice {
        device_id: device_id_from_serial(&serial),
        state: DeviceState::from_adb(&raw_state),
        raw_state,
        model: props.get("model").map(|s| s.to_string()),
        product: props.get("product").map(|s| s.to_string()),
        device: props.get("device").map(|s| s.to_string()),
        transport_id: props.get("transport_id").map(|s| s.to_string()),
        ip_address,
        connection,
        serial,
    })
}

/// `adb devices -l` (also the payload of a `track-devices -l` frame).
pub fn parse_devices(text: &str) -> Vec<AndroidDevice> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("List of devices") && !l.starts_with('*'))
        .filter_map(parse_device_line)
        .collect()
}

/// Incremental decoder for `adb track-devices` framing: 4 hex digits of length, then payload.
#[derive(Default)]
pub struct TrackParser {
    buf: Vec<u8>,
}

impl TrackParser {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(bytes);
        let mut frames = Vec::new();
        loop {
            if self.buf.len() < 4 {
                break;
            }
            let Some(len) = std::str::from_utf8(&self.buf[..4])
                .ok()
                .and_then(|h| usize::from_str_radix(h, 16).ok())
            else {
                // Not framed (e.g. an error message); surface it as-is and reset.
                frames.push(String::from_utf8_lossy(&self.buf).into_owned());
                self.buf.clear();
                break;
            };
            if self.buf.len() < 4 + len {
                break;
            }
            frames.push(String::from_utf8_lossy(&self.buf[4..4 + len]).into_owned());
            self.buf.drain(..4 + len);
        }
        frames
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectOutcome {
    Connected,
    AlreadyConnected,
}

pub fn parse_connect(address: &str, output: &str) -> Result<ConnectOutcome, AppError> {
    let lower = output.to_lowercase();
    let out = output.trim().to_string();
    if lower.contains("already connected") {
        Ok(ConnectOutcome::AlreadyConnected)
    } else if lower.contains("failed to authenticate") || lower.contains("unauthorized") {
        Err(AppError::DeviceUnauthorized {
            serial: address.to_string(),
        })
    } else if lower.contains("connection refused") {
        Err(AppError::ConnectionRefused {
            address: address.to_string(),
            output: out,
        })
    } else if lower.contains("timed out") || lower.contains("timeout") {
        Err(AppError::Timeout {
            operation: format!("Connecting to {address}"),
            after_ms: 0,
        })
    } else if lower.contains("no route to host")
        || lower.contains("unreachable")
        || lower.contains("host is down")
    {
        Err(AppError::WirelessDebuggingDisabled {
            address: address.to_string(),
            output: out,
        })
    } else if lower.contains("connected to")
        && !lower.contains("failed")
        && !lower.contains("cannot")
    {
        Ok(ConnectOutcome::Connected)
    } else {
        Err(AppError::AdbFailed {
            message: format!("could not connect to {address}"),
            stderr: out,
            exit_code: None,
        })
    }
}

pub fn parse_pair(output: &str) -> Result<(), AppError> {
    if output.to_lowercase().contains("successfully paired") {
        Ok(())
    } else {
        Err(AppError::PairingFailed {
            output: output.trim().to_string(),
        })
    }
}

pub fn parse_disconnect(target: &str, output: &str) -> Result<(), AppError> {
    let lower = output.to_lowercase();
    if lower.contains("no such device") || lower.contains("not found") {
        Err(AppError::DeviceNotFound {
            serial: target.to_string(),
        })
    } else if lower.contains("disconnected") || lower.trim().is_empty() {
        Ok(())
    } else {
        Err(AppError::AdbFailed {
            message: format!("could not disconnect {target}"),
            stderr: output.trim().to_string(),
            exit_code: None,
        })
    }
}

pub fn parse_mdns_check(output: &str) -> Result<(), AppError> {
    if output.contains("mdns daemon version") {
        Ok(())
    } else {
        Err(AppError::MdnsUnavailable {
            output: output.trim().to_string(),
        })
    }
}

pub fn parse_mdns_services(output: &str) -> Vec<MdnsService> {
    output
        .lines()
        .filter_map(|line| {
            let t: Vec<&str> = line.split_whitespace().collect();
            if t.len() < 3 || !t[1].starts_with("_adb") {
                return None;
            }
            let (ip, port) = split_host_port(t[2])?;
            let kind = match t[1].trim_end_matches('.') {
                "_adb-tls-connect._tcp" => MdnsServiceKind::Connect,
                "_adb-tls-pairing._tcp" => MdnsServiceKind::Pairing,
                _ => MdnsServiceKind::Other,
            };
            Some(MdnsService {
                name: t[0].to_string(),
                service_type: t[1].to_string(),
                kind,
                address: t[2].to_string(),
                ip,
                port,
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct VersionInfo {
    pub version: String,
    pub revision: Option<String>,
}

pub fn parse_version(output: &str) -> Option<VersionInfo> {
    let version = output
        .lines()
        .find_map(|l| l.trim().strip_prefix("Android Debug Bridge version "))?
        .trim()
        .to_string();
    let revision = output
        .lines()
        .find_map(|l| l.trim().strip_prefix("Version "))
        .map(|s| s.trim().to_string());
    Some(VersionInfo { version, revision })
}

/// Classify a failed device-scoped adb call.
pub fn map_device_error(serial: &str, stderr: &str, exit_code: Option<i32>) -> AppError {
    let lower = stderr.to_lowercase();
    if lower.contains("device offline") {
        AppError::DeviceOffline {
            serial: serial.to_string(),
        }
    } else if lower.contains("unauthorized") {
        AppError::DeviceUnauthorized {
            serial: serial.to_string(),
        }
    } else if lower.contains("not found") && lower.contains("device")
        || lower.contains("no devices/emulators found")
    {
        AppError::DeviceNotFound {
            serial: serial.to_string(),
        }
    } else {
        AppError::AdbFailed {
            message: "adb command failed".into(),
            stderr: strip_daemon_noise(stderr),
            exit_code,
        }
    }
}

/// Drop "* daemon not running; starting now" chatter adb prints on stderr.
pub fn strip_daemon_noise(s: &str) -> String {
    s.lines()
        .filter(|l| !l.trim_start().starts_with('*'))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

// ---------- device info (sectioned shell output) ----------

pub const DEVICE_INFO_SCRIPT: &str = "echo @@props; getprop 2>/dev/null; \
echo @@battery; dumpsys battery 2>/dev/null; \
echo @@df; df -k /data 2>/dev/null; \
echo @@mem; cat /proc/meminfo 2>/dev/null; \
echo @@cpu; cat /proc/cpuinfo 2>/dev/null; \
echo @@nproc; nproc 2>/dev/null; \
echo @@ip; ip -f inet addr show wlan0 2>/dev/null; \
echo @@aid; settings get secure android_id 2>/dev/null; \
echo @@termux; pm list packages -i --show-versioncode com.termux 2>/dev/null; \
echo @@termuxver; dumpsys package com.termux 2>/dev/null | grep -E 'versionName|firstInstallTime'; \
echo @@end";

pub fn split_sections(text: &str) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(name) = line.strip_prefix("@@") {
            current = Some(name.trim().to_string());
            out.entry(name.trim().to_string()).or_default();
        } else if let Some(c) = &current {
            let s = out.entry(c.clone()).or_default();
            s.push_str(line);
            s.push('\n');
        }
    }
    out
}

pub fn parse_getprop(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix('[')?;
            let (k, v) = rest.split_once("]: [")?;
            Some((k.to_string(), v.strip_suffix(']').unwrap_or(v).to_string()))
        })
        .collect()
}

fn kv_colon(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|l| {
            l.split_once(':')
                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        })
        .collect()
}

pub fn parse_battery(text: &str) -> Option<BatteryInfo> {
    // Some OEMs (e.g. OPPO) print a vendor block first; prefer the AOSP block.
    let body = text
        .split("Current Battery Service state:")
        .last()
        .unwrap_or(text);
    let kv = kv_colon(body);
    let level = kv.get("level").and_then(|v| v.parse::<u32>().ok());
    let scale = kv
        .get("scale")
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|s| *s > 0)
        .unwrap_or(100);
    let status_code = kv.get("status").and_then(|v| v.parse::<u8>().ok());
    if level.is_none() && status_code.is_none() {
        return None;
    }
    let status = match status_code {
        Some(2) => "Charging",
        Some(3) => "Discharging",
        Some(4) => "Not charging",
        Some(5) => "Full",
        _ => "Unknown",
    };
    let plugged = [
        ("AC powered", "AC"),
        ("USB powered", "USB"),
        ("Wireless powered", "Wireless"),
    ]
    .iter()
    .find(|(k, _)| kv.get(*k).map(String::as_str) == Some("true"))
    .map(|(_, name)| name.to_string());
    Some(BatteryInfo {
        level: level.map(|l| ((l * 100) / scale).min(100) as u8),
        charging: status_code == Some(2),
        status: status.to_string(),
        plugged,
        temperature_c: kv
            .get("temperature")
            .and_then(|v| v.parse::<f32>().ok())
            .map(|t| t / 10.0),
    })
}

pub fn parse_df_k(text: &str) -> Option<StorageInfo> {
    let row = text
        .lines()
        .skip_while(|l| !l.starts_with("Filesystem"))
        .nth(1)?;
    let t: Vec<&str> = row.split_whitespace().collect();
    if t.len() < 4 {
        return None;
    }
    let total: u64 = t[1].parse().ok()?;
    let free: u64 = t[3].parse().ok()?;
    Some(StorageInfo {
        total_bytes: total * 1024,
        free_bytes: free * 1024,
    })
}

pub fn parse_meminfo(text: &str) -> Option<MemoryInfo> {
    let kb = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key))
            .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
    };
    Some(MemoryInfo {
        total_bytes: kb("MemTotal:")? * 1024,
        available_bytes: kb("MemAvailable:")? * 1024,
    })
}

pub fn parse_cpu(cpuinfo: &str, nproc: &str, props: &HashMap<String, String>) -> Option<CpuInfo> {
    let kv = kv_colon(cpuinfo);
    let counted = cpuinfo
        .lines()
        .filter(|l| l.starts_with("processor"))
        .count() as u32;
    let cores = nproc
        .trim()
        .parse::<u32>()
        .ok()
        .or((counted > 0).then_some(counted));
    let hardware = kv
        .get("Hardware")
        .cloned()
        .or_else(|| props.get("ro.soc.model").cloned())
        .or_else(|| props.get("ro.board.platform").cloned())
        .filter(|s| !s.is_empty());
    let abi = props
        .get("ro.product.cpu.abi")
        .cloned()
        .filter(|s| !s.is_empty());
    if cores.is_none() && hardware.is_none() && abi.is_none() {
        return None;
    }
    Some(CpuInfo {
        abi,
        cores,
        hardware,
    })
}

pub fn parse_ip(text: &str) -> Option<String> {
    text.split_whitespace()
        .skip_while(|t| *t != "inet")
        .nth(1)
        .and_then(|cidr| cidr.split('/').next())
        .map(str::to_string)
}

pub fn parse_termux(pm: &str, dumpsys: &str) -> TermuxPackageInfo {
    let mut main: Option<(Option<u64>, Option<String>)> = None;
    let mut companions = Vec::new();
    for line in pm.lines() {
        let Some(rest) = line.trim().strip_prefix("package:") else {
            continue;
        };
        let mut tokens = rest.split_whitespace();
        let Some(name) = tokens.next() else { continue };
        let mut code = None;
        let mut installer = None;
        for t in tokens {
            if let Some(v) = t.strip_prefix("versionCode:") {
                code = v.parse().ok();
            } else if let Some(v) = t.strip_prefix("installer=") {
                installer = Some(v.to_string()).filter(|s| s != "null");
            }
        }
        if name == "com.termux" {
            main = Some((code, installer));
        } else if name.starts_with("com.termux.") {
            companions.push(name.to_string());
        }
    }
    let version_name = dumpsys
        .lines()
        .find_map(|l| l.trim().strip_prefix("versionName="))
        .map(|s| s.trim().to_string());
    match main {
        None => TermuxPackageInfo {
            installed: false,
            version_name: None,
            version_code: None,
            installer: None,
            source: TermuxSource::Unknown,
            companions,
        },
        Some((code, installer)) => {
            let source = match installer.as_deref() {
                Some("com.android.vending") => TermuxSource::PlayStore,
                Some("org.fdroid.fdroid")
                | Some("org.fdroid.basic")
                | Some("com.looker.droidify") => TermuxSource::FDroid,
                Some(_) | None => TermuxSource::Sideloaded,
            };
            TermuxPackageInfo {
                installed: true,
                version_name,
                version_code: code,
                installer,
                source,
                companions,
            }
        }
    }
}

pub fn parse_device_info(serial: &str, text: &str) -> DeviceInfo {
    let s = split_sections(text);
    let get = |k: &str| s.get(k).map(String::as_str).unwrap_or("");
    let props = parse_getprop(get("props"));
    let prop = |k: &str| props.get(k).cloned().filter(|v| !v.is_empty());
    let android_id = get("aid").trim();
    let device_id = prop("ro.serialno")
        .or_else(|| prop("ro.boot.serialno"))
        .filter(|v| v != "unknown")
        .or_else(|| {
            (!android_id.is_empty() && android_id != "null").then(|| android_id.to_string())
        })
        .or_else(|| device_id_from_serial(serial));
    let ip_address = parse_ip(get("ip")).or_else(|| match connection_type(serial) {
        ConnectionType::WirelessIp => split_host_port(serial).map(|(h, _)| h),
        _ => None,
    });
    DeviceInfo {
        serial: serial.to_string(),
        device_id,
        manufacturer: prop("ro.product.manufacturer"),
        model: prop("ro.product.model"),
        android_version: prop("ro.build.version.release"),
        sdk: prop("ro.build.version.sdk").and_then(|v| v.parse().ok()),
        ip_address,
        battery: parse_battery(get("battery")),
        storage: parse_df_k(get("df")),
        memory: parse_meminfo(get("mem")),
        cpu: parse_cpu(get("cpu"), get("nproc"), &props),
        termux: s
            .contains_key("termux")
            .then(|| parse_termux(get("termux"), get("termuxver"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICES_MDNS: &str = include_str!("../../tests/fixtures/adb/devices_wireless_mdns.txt");
    const MDNS_SERVICES: &str = include_str!("../../tests/fixtures/adb/mdns_services.txt");
    const MDNS_CHECK: &str = include_str!("../../tests/fixtures/adb/mdns_check.txt");
    const VERSION: &str = include_str!("../../tests/fixtures/adb/version.txt");
    const TRACK: &[u8] = include_bytes!("../../tests/fixtures/adb/track_frames.bin");
    const INFO: &str = include_str!("../../tests/fixtures/adb/device_info_sectioned.txt");
    const MDNS_SERIAL: &str = "adb-TESTSERIAL0001-a00nZY._adb-tls-connect._tcp";

    #[test]
    fn parses_real_mdns_device() {
        let d = parse_devices(DEVICES_MDNS);
        assert_eq!(d.len(), 1);
        let d = &d[0];
        assert_eq!(d.serial, MDNS_SERIAL);
        assert_eq!(d.state, DeviceState::Device);
        assert_eq!(d.model.as_deref(), Some("CPH2239"));
        assert_eq!(d.transport_id.as_deref(), Some("7"));
        assert_eq!(d.connection, ConnectionType::WirelessMdns);
        assert_eq!(d.device_id.as_deref(), Some("TESTSERIAL0001"));
    }

    #[test]
    fn parses_usb_ip_unauthorized_offline_and_multiple() {
        let text = "List of devices attached\n\
            R58M123ABC             device usb:1-1 product:beyond1 model:SM_G973F device:beyond1 transport_id:3\n\
            192.0.2.10:37145       unauthorized transport_id:4\n\
            192.0.2.11:5555        offline transport_id:5\n\
            emulator-5554          no permissions (user in plugdev group; are your udev rules wrong?); see [http://developer.android.com/tools/device.html] usb:1-2 transport_id:6\n\n";
        let d = parse_devices(text);
        assert_eq!(d.len(), 4);
        assert_eq!(d[0].connection, ConnectionType::Usb);
        assert_eq!(d[0].device_id.as_deref(), Some("R58M123ABC"));
        assert_eq!(d[1].state, DeviceState::Unauthorized);
        assert_eq!(d[1].connection, ConnectionType::WirelessIp);
        assert_eq!(d[1].ip_address.as_deref(), Some("192.0.2.10"));
        assert_eq!(d[1].device_id, None);
        assert_eq!(d[2].state, DeviceState::Offline);
        assert_eq!(d[3].state, DeviceState::NoPermissions);
    }

    #[test]
    fn empty_and_garbage_device_lists() {
        assert!(parse_devices("List of devices attached\n\n").is_empty());
        assert!(parse_devices("* daemon not running; starting now at tcp:5037\n").is_empty());
        assert!(parse_devices("\u{0}\u{1}garbage").is_empty());
    }

    #[test]
    fn track_parser_handles_real_and_split_frames() {
        let mut p = TrackParser::default();
        let frames = p.push(TRACK);
        assert_eq!(frames.len(), 1);
        assert_eq!(parse_devices(&frames[0])[0].serial, MDNS_SERIAL);

        let mut p = TrackParser::default();
        let (a, b) = TRACK.split_at(10);
        assert!(p.push(a).is_empty());
        assert_eq!(p.push(b).len(), 1);

        let mut p = TrackParser::default();
        assert_eq!(p.push(b"0000"), vec![String::new()]);
        assert!(parse_devices("").is_empty());
    }

    #[test]
    fn connect_outputs() {
        let a = "192.0.2.10:5555";
        assert_eq!(
            parse_connect(a, "connected to 192.0.2.10:5555"),
            Ok(ConnectOutcome::Connected)
        );
        assert_eq!(
            parse_connect(a, "already connected to 192.0.2.10:5555"),
            Ok(ConnectOutcome::AlreadyConnected)
        );
        assert!(matches!(
            parse_connect(
                a,
                "failed to connect to '192.0.2.10:5555': Connection refused"
            ),
            Err(AppError::ConnectionRefused { .. })
        ));
        assert!(matches!(
            parse_connect(a, "cannot connect to 192.0.2.10:5555: Operation timed out"),
            Err(AppError::Timeout { .. })
        ));
        assert!(matches!(
            parse_connect(a, "failed to authenticate to 192.0.2.10:5555"),
            Err(AppError::DeviceUnauthorized { .. })
        ));
        assert!(matches!(
            parse_connect(a, "failed to connect to 192.0.2.10:5555: No route to host"),
            Err(AppError::WirelessDebuggingDisabled { .. })
        ));
        assert!(matches!(
            parse_connect(a, "something odd"),
            Err(AppError::AdbFailed { .. })
        ));
    }

    #[test]
    fn pair_and_disconnect_outputs() {
        assert!(parse_pair("Successfully paired to 192.0.2.10:40000 [guid=adb-X-Y]").is_ok());
        assert!(matches!(
            parse_pair("Failed: Wrong password or connection was dropped."),
            Err(AppError::PairingFailed { .. })
        ));
        assert!(parse_disconnect("x", "disconnected 192.0.2.10:5555").is_ok());
        assert!(matches!(
            parse_disconnect("x", "error: no such device '192.0.2.10:5555'"),
            Err(AppError::DeviceNotFound { .. })
        ));
    }

    #[test]
    fn mdns_outputs() {
        assert!(parse_mdns_check(MDNS_CHECK).is_ok());
        assert!(parse_mdns_check("ERROR: mdns daemon unavailable").is_err());
        let s = parse_mdns_services(MDNS_SERVICES);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].kind, MdnsServiceKind::Connect);
        assert_eq!(s[0].address, "192.0.2.25:44467");
        assert_eq!(s[0].port, 44467);
        assert_eq!(
            mdns_instance_serialno(&s[0].name).as_deref(),
            Some("TESTSERIAL0001")
        );
        let pairing = parse_mdns_services("hacc-abc123\t_adb-tls-pairing._tcp\t192.0.2.25:40111\n");
        assert_eq!(pairing[0].kind, MdnsServiceKind::Pairing);
    }

    #[test]
    fn version_output() {
        let v = parse_version(VERSION).unwrap();
        assert_eq!(v.version, "1.0.41");
        assert_eq!(v.revision.as_deref(), Some("37.0.1-15733141"));
        assert!(parse_version("nonsense").is_none());
    }

    #[test]
    fn device_error_mapping() {
        assert!(matches!(
            map_device_error("s", "error: device offline", Some(1)),
            AppError::DeviceOffline { .. }
        ));
        assert!(matches!(
            map_device_error(
                "s",
                "error: device unauthorized.\nThis adb server's $ADB_VENDOR_KEYS is not set",
                Some(1)
            ),
            AppError::DeviceUnauthorized { .. }
        ));
        assert!(matches!(
            map_device_error("s", "error: device 's' not found", Some(1)),
            AppError::DeviceNotFound { .. }
        ));
        assert!(matches!(
            map_device_error("s", "weird", Some(1)),
            AppError::AdbFailed { .. }
        ));
        assert_eq!(
            strip_daemon_noise("* daemon not running; starting now\nreal error"),
            "real error"
        );
    }

    #[test]
    fn real_device_info() {
        let i = parse_device_info(MDNS_SERIAL, INFO);
        assert_eq!(i.device_id.as_deref(), Some("TESTSERIAL0001"));
        assert_eq!(i.manufacturer.as_deref(), Some("OPPO"));
        assert_eq!(i.model.as_deref(), Some("CPH2239"));
        assert_eq!(i.android_version.as_deref(), Some("11"));
        assert_eq!(i.sdk, Some(30));
        assert_eq!(i.ip_address.as_deref(), Some("192.0.2.25"));
        let b = i.battery.unwrap();
        assert_eq!(b.level, Some(100));
        assert_eq!(b.status, "Full");
        assert_eq!(b.plugged.as_deref(), Some("AC"));
        assert_eq!(b.temperature_c, Some(34.3));
        let st = i.storage.unwrap();
        assert_eq!(st.total_bytes, 109_780_492 * 1024);
        assert_eq!(st.free_bytes, 84_112_064 * 1024);
        let m = i.memory.unwrap();
        assert_eq!(m.total_bytes, 3_893_508 * 1024);
        assert_eq!(m.available_bytes, 1_409_096 * 1024);
        let c = i.cpu.unwrap();
        assert_eq!(c.cores, Some(8));
        assert_eq!(c.abi.as_deref(), Some("arm64-v8a"));
        assert_eq!(c.hardware.as_deref(), Some("MT6765G"));
        let t = i.termux.unwrap();
        assert!(t.installed);
        assert_eq!(t.version_code, Some(1022));
        assert_eq!(t.version_name.as_deref(), Some("0.119.0-beta.3"));
        assert_eq!(t.source, TermuxSource::Sideloaded);
    }

    #[test]
    fn partial_device_info_is_none_not_fabricated() {
        let i = parse_device_info(
            "192.0.2.10:5555",
            "@@props\n[ro.product.model]: [Pixel 8]\n@@battery\n@@end\n",
        );
        assert_eq!(i.model.as_deref(), Some("Pixel 8"));
        assert_eq!(i.ip_address.as_deref(), Some("192.0.2.10"));
        assert!(
            i.battery.is_none() && i.storage.is_none() && i.memory.is_none() && i.cpu.is_none()
        );
        assert!(i.termux.is_none());
        assert!(i.device_id.is_none());
    }

    #[test]
    fn termux_sources() {
        let play = parse_termux(
            "package:com.termux versionCode:118 installer=com.android.vending\n",
            "",
        );
        assert_eq!(play.source, TermuxSource::PlayStore);
        let fd = parse_termux(
            "package:com.termux versionCode:1000 installer=org.fdroid.fdroid\npackage:com.termux.boot versionCode:7 installer=org.fdroid.fdroid\n",
            "    versionName=0.118.0\n",
        );
        assert_eq!(fd.source, TermuxSource::FDroid);
        assert_eq!(fd.companions, vec!["com.termux.boot"]);
        let none = parse_termux("", "");
        assert!(!none.installed);
    }

    #[test]
    fn parsers_never_panic_on_garbage() {
        let inputs = [
            "",
            "\n\n",
            "@@",
            "@@props\n[",
            "[a]: [",
            "Filesystem\n1 2",
            "level: x",
            "inet",
            "\u{feff}::::",
        ];
        for s in inputs {
            let _ = parse_devices(s);
            let _ = parse_device_info("s", s);
            let _ = parse_mdns_services(s);
            let _ = parse_connect("a:1", s);
            let _ = parse_version(s);
            let _ = TrackParser::default().push(s.as_bytes());
        }
    }
}
