//! Connection monitoring glue: tracker → UI events, device-id learning, auto-reconnect.

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::adb::{reconnect, tracker, AndroidDevice, ConnectionType};
use crate::devices::ReconnectStatus;
use crate::state::AppState;

pub const EVT_DEVICES: &str = "device://changed";
pub const EVT_RECONNECT: &str = "device://reconnect";

pub fn emit_devices<R: Runtime>(app: &AppHandle<R>) {
    let list = app.state::<AppState>().devices.list();
    if let Err(e) = app.emit(EVT_DEVICES, list) {
        tracing::warn!(error = %e, "emit devices failed");
    }
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
        on_snapshot: Arc::new(move |list, diff| {
            let _ = snap_app.emit(EVT_DEVICES, list);
            for d in diff.ready.into_iter().filter(|d| d.device_id.is_none()) {
                learn_device_id(snap_app.clone(), d.serial);
            }
            for d in diff.lost {
                maybe_reconnect(snap_app.clone(), d);
            }
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
            if !id.is_empty() && id != "unknown" && state.devices.set_device_id(&serial, id) {
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
        let mut map = state.reconnects.lock().unwrap();
        if let Some(old) = map.insert(device.serial.clone(), token.clone()) {
            old.cancel();
        }
    }
    tauri::async_runtime::spawn(async move {
        let mine = token.clone();
        let state = app.state::<AppState>();
        let reconnect_cfg = state.config.read().await.reconnect.clone();
        if !reconnect_cfg.enabled {
            return;
        }
        let Ok(client) = state.adb_client().await else {
            return;
        };
        let target = reconnect::ReconnectTarget::from_device(&device);
        let emit_app = app.clone();
        let ok = reconnect::run(
            &client,
            target,
            &reconnect_cfg.schedule_ms,
            token,
            move |s: ReconnectStatus| {
                let st = emit_app.state::<AppState>();
                st.devices.set_reconnect(s.clone());
                let _ = emit_app.emit(EVT_RECONNECT, s);
                emit_devices(&emit_app);
            },
        )
        .await;
        if ok {
            state.devices.clear_reconnect(&device.serial);
        }
        if !mine.is_cancelled() {
            state.reconnects.lock().unwrap().remove(&device.serial);
        }
        emit_devices(&app);
    });
}
