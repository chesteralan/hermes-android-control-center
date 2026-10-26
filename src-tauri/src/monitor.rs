//! Connection monitoring glue: tracker → UI events, device-id learning, auto-reconnect.

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::adb::{reconnect, tracker, AndroidDevice, ConnectionType, DeviceState};
use crate::devices::{ReconnectStatus, SnapshotDiff};
use crate::state::AppState;

pub const EVT_DEVICES: &str = "device://changed";
pub const EVT_RECONNECT: &str = "device://reconnect";

pub fn emit_devices<R: Runtime>(app: &AppHandle<R>) {
    let list = app.state::<AppState>().devices.list();
    crate::lifecycle::update_tray_status(app, &list);
    // Best-effort: logged; the next registry change re-emits the device list.
    if let Err(e) = app.emit(EVT_DEVICES, list) {
        tracing::warn!(error = %e, "emit devices failed");
    }
}

pub fn snapshot_applied<R: Runtime>(app: &AppHandle<R>, diff: SnapshotDiff) {
    emit_devices(app);
    for serial in identity_lookup_candidates(app.state::<AppState>().devices.list()) {
        learn_device_id(app.clone(), serial);
    }
    for device in diff.lost {
        maybe_reconnect(app.clone(), device);
    }
}

fn identity_lookup_candidates(devices: Vec<AndroidDevice>) -> Vec<String> {
    devices
        .into_iter()
        .filter(|device| {
            device.state == DeviceState::Device
                && device.connection != ConnectionType::Usb
                && device.device_id.is_none()
        })
        .map(|device| device.serial)
        .collect()
}

pub fn start<R: Runtime>(app: AppHandle<R>) {
    let state = app.state::<AppState>();
    let registry = state.devices.clone();
    let cancel = state.shutdown.child_token();

    let client_app = app.clone();
    let snap_app = app.clone();
    let hooks = tracker::TrackerHooks {
        get_client: Arc::new(move || {
            let a = client_app.clone();
            Box::pin(async move { a.state::<AppState>().adb_client().await })
        }),
        on_snapshot: Arc::new(move |_list, diff| {
            snapshot_applied(&snap_app, diff);
        }),
    };
    tauri::async_runtime::spawn(tracker::run(registry, hooks, cancel));
    tauri::async_runtime::spawn(auto_connect(app));
}

async fn auto_connect<R: Runtime>(app: AppHandle<R>) {
    let state = app.state::<AppState>();
    let addresses: Vec<String> = state
        .config
        .read()
        .await
        .known_addresses
        .iter()
        .filter(|k| k.auto_connect)
        .map(|k| k.address.clone())
        .collect();
    if addresses.is_empty() {
        return;
    }
    let Ok(client) = state.adb_client().await else {
        return;
    };
    for addr in addresses {
        match client.connect(&addr).await {
            Ok(_) => tracing::info!("auto-connected"),
            Err(e) => tracing::info!(error = %e, "auto-connect failed"),
        }
    }
}

/// Wireless `ip:port` serials don't reveal the hardware serial; ask the device once.
fn learn_device_id<R: Runtime>(app: AppHandle<R>, serial: String) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let Ok(client) = state.adb_client().await else {
            return;
        };
        if let Ok(out) = client
            .shell(&serial, "getprop ro.serialno", Duration::from_secs(5))
            .await
        {
            let id = out.stdout.trim();
            if id.is_empty() || id == "unknown" {
                return;
            }
            let registry_changed = state.devices.set_device_id(&serial, id);
            let profile_changed = state.adopt_profile_identity(&serial, id).await;
            if profile_changed {
                let cfg = state.config.read().await;
                // Best-effort: logged; the in-memory profile already holds the id.
                if let Err(e) = crate::config_store::save(&app, &cfg) {
                    tracing::warn!(error = %e, "could not persist re-keyed device profile");
                }
            }
            if registry_changed || profile_changed {
                emit_devices(&app);
            }
        }
    });
}

pub fn maybe_reconnect<R: Runtime>(app: AppHandle<R>, device: AndroidDevice) {
    let state = app.state::<AppState>();
    if device.connection == ConnectionType::Usb
        || state.devices.is_manual_disconnect(&device.serial)
    {
        return;
    }
    spawn_reconnect(app.clone(), device);
}

pub fn spawn_reconnect<R: Runtime>(app: AppHandle<R>, device: AndroidDevice) {
    let state = app.state::<AppState>();
    let token = state.shutdown.child_token();
    {
        if let Some(old) = state.track_reconnect(device.serial.clone(), token.clone()) {
            old.cancel();
        }
    }
    tauri::async_runtime::spawn(async move {
        let mine = token.clone();
        let state = app.state::<AppState>();
        let (reconnect_cfg, profiles) = {
            let cfg = state.config.read().await;
            (cfg.reconnect.clone(), cfg.devices.clone())
        };
        if !reconnect_cfg.enabled {
            return;
        }
        let Ok(client) = state.adb_client().await else {
            return;
        };
        let target = reconnect_target(&device, &profiles);
        let emit_app = app.clone();
        let ok = reconnect::run(
            &client,
            target,
            &reconnect_cfg.schedule_ms,
            token,
            move |s: ReconnectStatus| {
                let st = emit_app.state::<AppState>();
                st.devices.set_reconnect(s.clone());
                // Best-effort: no listener once the UI has closed.
                let _ = emit_app.emit(EVT_RECONNECT, s);
                emit_devices(&emit_app);
            },
        )
        .await;
        if ok {
            state.devices.clear_reconnect(&device.serial);
        }
        if !mine.is_cancelled() {
            state.remove_reconnect(&device.serial);
        }
        emit_devices(&app);
    });
}

/// Build the reconnect target, preferring the phone's remembered profile address
/// (M9-T7): a wireless port change must not strand a phone on its stale serial.
pub fn reconnect_target(
    device: &AndroidDevice,
    profiles: &std::collections::HashMap<String, crate::config::DeviceProfile>,
) -> reconnect::ReconnectTarget {
    let mut target = reconnect::ReconnectTarget::from_device(device);
    let remembered = target
        .device_id
        .as_deref()
        .and_then(|id| profiles.get(id))
        .and_then(|profile| profile.last_address.as_ref());
    if let Some(address) = remembered {
        target.address = Some(address.clone());
        if let Some((host, _)) = crate::adb::address::split_host_port(address) {
            target.ip = Some(host);
        }
    }
    target
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adb::parse::parse_devices;
    use crate::config::DeviceProfile;
    use std::collections::HashMap;

    #[test]
    fn identity_lookup_includes_already_registered_ip_aliases() {
        let devices = parse_devices("192.0.2.10:5555 device model:Phone\n");
        assert_eq!(identity_lookup_candidates(devices), ["192.0.2.10:5555"]);
    }

    #[test]
    fn reconnect_target_prefers_profile_last_address() {
        let device = AndroidDevice {
            serial: "192.0.2.10:5555".into(),
            device_id: Some("SER1".into()),
            model: None,
            product: None,
            device: None,
            transport_id: None,
            ip_address: None,
            state: DeviceState::Offline,
            raw_state: "offline".into(),
            connection: ConnectionType::WirelessIp,
        };
        let mut profiles = HashMap::new();
        profiles.insert(
            "SER1".to_string(),
            DeviceProfile {
                device_id: "SER1".into(),
                last_address: Some("192.0.2.99:41000".into()),
                ..Default::default()
            },
        );

        let target = reconnect_target(&device, &profiles);
        assert_eq!(target.address.as_deref(), Some("192.0.2.99:41000"));
        assert_eq!(target.ip.as_deref(), Some("192.0.2.99"));

        let fallback = reconnect_target(&device, &HashMap::new());
        assert_eq!(fallback.address.as_deref(), Some("192.0.2.10:5555"));
        assert_eq!(fallback.ip.as_deref(), Some("192.0.2.10"));
    }
}
