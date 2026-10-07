use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, Runtime, State};

use crate::adb::address::{split_host_port, validate_address};
use crate::adb::qr_pair::{self, QrPairEvent, QrSession};
use crate::adb::{AndroidDevice, ConnectionType, DeviceInfo, DeviceState, MdnsService};
use crate::config_store;
use crate::error::AppError;
use crate::monitor;
use crate::state::AppState;

async fn refresh<R: Runtime>(app: &AppHandle<R>) -> Result<Vec<AndroidDevice>, AppError> {
    let state = app.state::<AppState>();
    let client = state.adb_client().await?;
    let list = client.devices().await?;
    let diff = state.devices.apply_snapshot(list);
    monitor::snapshot_applied(app, diff);
    Ok(state.devices.list())
}

#[tauri::command]
pub async fn list_devices<R: Runtime>(app: AppHandle<R>) -> Result<Vec<AndroidDevice>, AppError> {
    refresh(&app).await
}

#[tauri::command]
pub async fn connect_device<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    address: String,
) -> Result<AndroidDevice, AppError> {
    let address = address.trim().to_string();
    validate_address(&address)?;
    state.devices.clear_manual_disconnect(&address);
    state.adb_client().await?.connect(&address).await?;

    {
        let mut cfg = state.config.write().await;
        cfg.remember_address(&address);
        if let Err(e) = config_store::save(&app, &cfg) {
            tracing::warn!(error = %e, "could not persist known address");
        }
    }

    let list = refresh(&app).await?;
    let ip = split_host_port(&address).map(|(h, _)| h);
    let found = list.iter().find(|d| d.serial == address).or_else(|| {
        list.iter()
            .find(|d| d.ip_address.is_some() && d.ip_address == ip)
    });
    Ok(found.cloned().unwrap_or_else(|| AndroidDevice {
        serial: address.clone(),
        device_id: None,
        model: None,
        product: None,
        device: None,
        transport_id: None,
        ip_address: ip,
        state: DeviceState::Connecting,
        raw_state: "connecting".into(),
        connection: ConnectionType::WirelessIp,
    }))
}

#[tauri::command]
pub async fn disconnect_device<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    serial: String,
) -> Result<(), AppError> {
    state.devices.mark_manual_disconnect(&serial);
    state.cancel_reconnect(&serial);
    state.streams.cancel_device(&serial);
    state.release_device(&serial).await;
    state.adb_client().await?.disconnect(&serial).await?;
    refresh(&app).await?;
    Ok(())
}

#[tauri::command]
pub async fn retry_connection<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    serial: String,
) -> Result<(), AppError> {
    let device = state
        .devices
        .last_seen(&serial)
        .ok_or_else(|| AppError::DeviceNotFound {
            serial: serial.clone(),
        })?;
    state.devices.clear_manual_disconnect(&serial);
    let devices = refresh(&app).await?;
    if devices.iter().any(|candidate| {
        candidate.state == DeviceState::Device
            && (candidate.serial == serial
                || device
                    .device_id
                    .as_deref()
                    .is_some_and(|id| candidate.device_id.as_deref() == Some(id)))
    }) {
        return Ok(());
    }
    monitor::spawn_reconnect(app, device);
    Ok(())
}

#[tauri::command]
pub async fn pair_device(
    state: State<'_, AppState>,
    address: String,
    code: String,
) -> Result<(), AppError> {
    state
        .adb_client()
        .await?
        .pair(address.trim(), code.trim())
        .await
}

#[tauri::command]
pub async fn discover_devices(state: State<'_, AppState>) -> Result<Vec<MdnsService>, AppError> {
    let client = state.adb_client().await?;
    client.mdns_check().await?;
    client.mdns_services().await
}

#[tauri::command]
pub async fn get_device_info<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    serial: String,
) -> Result<DeviceInfo, AppError> {
    let info = state.adb_client().await?.device_info(&serial).await?;
    if let Some(id) = &info.device_id {
        if state.devices.set_device_id(&serial, id) {
            monitor::emit_devices(&app);
        }
    }
    Ok(info)
}

#[tauri::command]
pub async fn start_qr_pairing(
    state: State<'_, AppState>,
    on_event: Channel<QrPairEvent>,
) -> Result<QrSession, AppError> {
    let client = state.adb_client().await?;
    client.mdns_check().await?;
    let creds = qr_pair::new_credentials();
    let svg = qr_pair::render_svg(&qr_pair::qr_payload(&creds))?;
    let session_id = creds.service_name.clone();
    let cancel = state.shutdown.child_token();
    state
        .qr_sessions
        .lock()
        .unwrap()
        .insert(session_id.clone(), cancel.clone());

    tauri::async_runtime::spawn(async move {
        qr_pair::run(&client, &creds, qr_pair::QR_TIMEOUT, cancel, move |ev| {
            let _ = on_event.send(ev);
        })
        .await;
    });

    Ok(QrSession {
        session_id,
        qr_svg: svg,
        expires_in_ms: qr_pair::QR_TIMEOUT.as_millis() as u64,
    })
}

#[tauri::command]
pub async fn cancel_qr_pairing(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), AppError> {
    if let Some(t) = state.qr_sessions.lock().unwrap().remove(&session_id) {
        t.cancel();
    }
    Ok(())
}
