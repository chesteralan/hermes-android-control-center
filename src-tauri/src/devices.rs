//! In-memory view of all devices, keyed by serial. Multi-device by design.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

use serde::Serialize;
use ts_rs::TS;

use crate::adb::{AndroidDevice, DeviceState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ReconnectPhase {
    Waiting,
    Attempting,
    Connected,
    GaveUp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReconnectStatus {
    pub serial: String,
    pub device_id: Option<String>,
    pub phase: ReconnectPhase,
    pub attempt: u32,
    pub max_attempts: u32,
    #[ts(type = "number | null")]
    pub next_delay_ms: Option<u64>,
}

#[derive(Debug, Default, PartialEq)]
pub struct SnapshotDiff {
    /// Devices that were usable (`device` state) and no longer are.
    pub lost: Vec<AndroidDevice>,
    /// Devices that just became usable.
    pub ready: Vec<AndroidDevice>,
    pub changed: bool,
}

#[derive(Default)]
struct Inner {
    devices: BTreeMap<String, AndroidDevice>,
    ids: HashMap<String, String>,
    manual_disconnect: HashSet<String>,
    reconnect: HashMap<String, ReconnectStatus>,
    /// Last device seen per serial, kept so lost wireless devices stay visible while reconnecting.
    last_seen: HashMap<String, AndroidDevice>,
}

#[derive(Default)]
pub struct DeviceRegistry {
    inner: Mutex<Inner>,
}

impl DeviceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply_snapshot(&self, mut list: Vec<AndroidDevice>) -> SnapshotDiff {
        let mut g = self.inner.lock().unwrap();
        for d in &mut list {
            if d.device_id.is_none() {
                d.device_id = g.ids.get(&d.serial).cloned();
            }
        }
        let new: BTreeMap<String, AndroidDevice> =
            list.into_iter().map(|d| (d.serial.clone(), d)).collect();
        let mut diff = SnapshotDiff::default();
        for (serial, old) in &g.devices {
            let still_ready = new
                .get(serial)
                .is_some_and(|d| d.state == DeviceState::Device);
            if old.state == DeviceState::Device && !still_ready {
                diff.lost.push(old.clone());
            }
        }
        for (serial, d) in &new {
            let was_ready = g
                .devices
                .get(serial)
                .is_some_and(|o| o.state == DeviceState::Device);
            if d.state == DeviceState::Device && !was_ready {
                diff.ready.push(d.clone());
            }
        }
        diff.changed = g.devices != new;
        for d in new.values() {
            g.last_seen.insert(d.serial.clone(), d.clone());
            if d.state == DeviceState::Device {
                g.reconnect.remove(&d.serial);
            }
        }
        g.devices = new;
        diff
    }

    /// Devices for the UI, including lost wireless devices that are reconnecting or gave up.
    pub fn list(&self) -> Vec<AndroidDevice> {
        let g = self.inner.lock().unwrap();
        let mut out: Vec<AndroidDevice> = g.devices.values().cloned().collect();
        for (serial, status) in &g.reconnect {
            if g.devices.contains_key(serial) {
                continue;
            }
            if let Some(last) = g.last_seen.get(serial) {
                let mut d = last.clone();
                d.state = match status.phase {
                    ReconnectPhase::GaveUp => DeviceState::Disconnected,
                    _ => DeviceState::Reconnecting,
                };
                out.push(d);
            }
        }
        out
    }

    pub fn get(&self, serial: &str) -> Option<AndroidDevice> {
        self.inner.lock().unwrap().devices.get(serial).cloned()
    }

    pub fn set_device_id(&self, serial: &str, id: &str) -> bool {
        let mut g = self.inner.lock().unwrap();
        g.ids.insert(serial.to_string(), id.to_string());
        let mut changed = false;
        if let Some(d) = g.devices.get_mut(serial) {
            changed = d.device_id.as_deref() != Some(id);
            d.device_id = Some(id.to_string());
        }
        changed
    }

    pub fn mark_manual_disconnect(&self, serial: &str) {
        let mut g = self.inner.lock().unwrap();
        g.manual_disconnect.insert(serial.to_string());
        g.reconnect.remove(serial);
    }

    pub fn clear_manual_disconnect(&self, serial: &str) {
        self.inner.lock().unwrap().manual_disconnect.remove(serial);
    }

    pub fn is_manual_disconnect(&self, serial: &str) -> bool {
        self.inner
            .lock()
            .unwrap()
            .manual_disconnect
            .contains(serial)
    }

    pub fn set_reconnect(&self, status: ReconnectStatus) {
        self.inner
            .lock()
            .unwrap()
            .reconnect
            .insert(status.serial.clone(), status);
    }

    pub fn clear_reconnect(&self, serial: &str) {
        self.inner.lock().unwrap().reconnect.remove(serial);
    }

    pub fn reconnect_status(&self, serial: &str) -> Option<ReconnectStatus> {
        self.inner.lock().unwrap().reconnect.get(serial).cloned()
    }

    pub fn reconnect_statuses(&self) -> Vec<ReconnectStatus> {
        self.inner
            .lock()
            .unwrap()
            .reconnect
            .values()
            .cloned()
            .collect()
    }

    pub fn last_seen(&self, serial: &str) -> Option<AndroidDevice> {
        self.inner.lock().unwrap().last_seen.get(serial).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adb::parse::parse_devices;

    fn snap(text: &str) -> Vec<AndroidDevice> {
        parse_devices(text)
    }

    #[test]
    fn detects_ready_and_lost_across_devices() {
        let r = DeviceRegistry::new();
        let d = r.apply_snapshot(snap("A device model:X\n192.0.2.1:5555 device\n"));
        assert_eq!(d.ready.len(), 2);
        assert!(d.changed);

        let d = r.apply_snapshot(snap("A device model:X\n192.0.2.1:5555 offline\n"));
        assert_eq!(d.lost.len(), 1);
        assert_eq!(d.lost[0].serial, "192.0.2.1:5555");
        assert!(d.ready.is_empty());

        let d = r.apply_snapshot(snap("A device model:X\n192.0.2.1:5555 offline\n"));
        assert!(!d.changed && d.lost.is_empty());
    }

    #[test]
    fn device_id_cache_survives_snapshots() {
        let r = DeviceRegistry::new();
        r.apply_snapshot(snap("192.0.2.1:5555 device\n"));
        assert!(r.set_device_id("192.0.2.1:5555", "SER1"));
        r.apply_snapshot(snap("192.0.2.1:5555 device\n"));
        assert_eq!(
            r.get("192.0.2.1:5555").unwrap().device_id.as_deref(),
            Some("SER1")
        );
    }

    #[test]
    fn lost_device_stays_listed_while_reconnecting() {
        let r = DeviceRegistry::new();
        r.apply_snapshot(snap("192.0.2.1:5555 device model:P\n"));
        r.apply_snapshot(vec![]);
        r.set_reconnect(ReconnectStatus {
            serial: "192.0.2.1:5555".into(),
            device_id: None,
            phase: ReconnectPhase::Waiting,
            attempt: 1,
            max_attempts: 5,
            next_delay_ms: Some(1000),
        });
        let l = r.list();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].state, DeviceState::Reconnecting);
        r.apply_snapshot(snap("192.0.2.1:5555 device model:P\n"));
        assert!(r.reconnect_status("192.0.2.1:5555").is_none());
    }

    #[test]
    fn manual_disconnect_clears_reconnect() {
        let r = DeviceRegistry::new();
        r.set_reconnect(ReconnectStatus {
            serial: "s".into(),
            device_id: None,
            phase: ReconnectPhase::Waiting,
            attempt: 1,
            max_attempts: 2,
            next_delay_ms: None,
        });
        r.mark_manual_disconnect("s");
        assert!(r.is_manual_disconnect("s"));
        assert!(r.reconnect_status("s").is_none());
    }
}
