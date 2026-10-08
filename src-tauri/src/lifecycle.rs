use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::adb::{AndroidDevice, DeviceState};
use crate::state::AppState;

const MAIN_WINDOW: &str = "main";
const TRAY_ID: &str = "hacc-tray";
const OPEN_ID: &str = "open-main-window";
const QUIT_ID: &str = "quit-application";
const ABOUT_MENU_ID: &str = "about-app";
const SETTINGS_MENU_ID: &str = "open-settings";
const UPDATE_MENU_ID: &str = "check-for-updates";
pub const MENU_COMMAND_EVENT: &str = "hacc://menu-command";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrayDeviceStatus {
    NoDevices,
    Connected,
    Attention,
}

struct TrayIconPixels {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

fn aggregate_tray_status(devices: &[AndroidDevice]) -> TrayDeviceStatus {
    if devices.is_empty() {
        TrayDeviceStatus::NoDevices
    } else if devices
        .iter()
        .all(|device| device.state == DeviceState::Device)
    {
        TrayDeviceStatus::Connected
    } else {
        TrayDeviceStatus::Attention
    }
}

fn tray_tooltip(status: TrayDeviceStatus, count: usize) -> String {
    match status {
        TrayDeviceStatus::NoDevices => "Hermes Control Center: no Android devices".into(),
        TrayDeviceStatus::Connected => {
            let noun = if count == 1 { "device" } else { "devices" };
            format!("Hermes Control Center: {count} connected {noun}")
        }
        TrayDeviceStatus::Attention => {
            let noun = if count == 1 { "device" } else { "devices" };
            format!("Hermes Control Center: attention needed ({count} {noun})")
        }
    }
}

fn paint_disk(
    rgba: &mut [u8],
    width: u32,
    height: u32,
    center_x: i32,
    center_y: i32,
    radius: i32,
    color: [u8; 4],
) {
    let width = width as i32;
    let height = height as i32;
    for y in (center_y - radius).max(0)..=(center_y + radius).min(height - 1) {
        for x in (center_x - radius).max(0)..=(center_x + radius).min(width - 1) {
            if (x - center_x).pow(2) + (y - center_y).pow(2) > radius.pow(2) {
                continue;
            }
            let offset = ((y * width + x) * 4) as usize;
            rgba[offset..offset + 4].copy_from_slice(&color);
        }
    }
}

fn tray_status_icon(status: TrayDeviceStatus) -> Option<Image<'static>> {
    static BASE_ICON: OnceLock<Result<TrayIconPixels, String>> = OnceLock::new();
    let base = match BASE_ICON.get_or_init(|| {
        Image::from_bytes(include_bytes!("../icons/icon.png"))
            .map(|image| TrayIconPixels {
                rgba: image.rgba().to_vec(),
                width: image.width(),
                height: image.height(),
            })
            .map_err(|error| error.to_string())
    }) {
        Ok(icon) => icon,
        Err(error) => {
            tracing::warn!(%error, "failed to decode tray status icon");
            return None;
        }
    };

    let width = base.width;
    let height = base.height;
    let mut rgba = base.rgba.clone();
    let radius = (width.min(height) / 7).max(2) as i32;
    let center_x = width as i32 - radius - 2;
    let center_y = height as i32 - radius - 2;
    let color = match status {
        TrayDeviceStatus::NoDevices => [135, 145, 155, 255],
        TrayDeviceStatus::Connected => [46, 160, 67, 255],
        TrayDeviceStatus::Attention => [224, 112, 40, 255],
    };
    paint_disk(
        &mut rgba,
        width,
        height,
        center_x,
        center_y,
        radius + 1,
        [255, 255, 255, 255],
    );
    paint_disk(&mut rgba, width, height, center_x, center_y, radius, color);
    Some(Image::new_owned(rgba, width, height))
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
enum MenuCommand {
    About,
    Settings,
    CheckForUpdates,
}

pub struct AppLifecycle {
    tray_available: bool,
    quitting: AtomicBool,
}

impl AppLifecycle {
    pub fn new(tray_available: bool) -> Self {
        Self {
            tray_available,
            quitting: AtomicBool::new(false),
        }
    }

    pub fn should_hide_on_close(&self) -> bool {
        self.tray_available && !self.quitting.load(Ordering::SeqCst)
    }

    pub fn should_hide_on_minimize(&self, minimized: bool) -> bool {
        self.tray_available && minimized && !self.quitting.load(Ordering::SeqCst)
    }

    fn begin_quit(&self) -> bool {
        !self.quitting.swap(true, Ordering::SeqCst)
    }
}

fn wayland_session() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE")
                .is_ok_and(|session| session.eq_ignore_ascii_case("wayland"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

pub fn install_app_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let about = MenuItem::with_id(
        app,
        ABOUT_MENU_ID,
        "About Hermes Control Center",
        true,
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let settings = MenuItem::with_id(app, SETTINGS_MENU_ID, "Settings", true, Some("CmdOrCtrl+,"))?;
    let check_updates =
        MenuItem::with_id(app, UPDATE_MENU_ID, "Check for Updates", true, None::<&str>)?;
    let quit = PredefinedMenuItem::quit(app, Some("Quit Hermes Control Center"))?;
    let app_submenu = Submenu::with_items(
        app,
        "Hermes Control Center",
        true,
        &[
            &about,
            &separator,
            &settings,
            &check_updates,
            &separator,
            &quit,
        ],
    )?;
    let menu = Menu::with_items(app, &[&app_submenu])?;
    app.set_menu(menu)?;
    app.on_menu_event(|app, event| {
        let command = match event.id().as_ref() {
            ABOUT_MENU_ID => Some(MenuCommand::About),
            SETTINGS_MENU_ID => Some(MenuCommand::Settings),
            UPDATE_MENU_ID => Some(MenuCommand::CheckForUpdates),
            _ => None,
        };
        if let Some(command) = command {
            let _ = app.emit(MENU_COMMAND_EVENT, command);
        }
    });
    Ok(())
}

pub fn install_tray<R: Runtime>(app: &AppHandle<R>) -> bool {
    if wayland_session() {
        tracing::warn!("tray unavailable on Wayland; using normal window controls");
        return false;
    }

    match build_tray(app) {
        Ok(_) => {
            update_tray_status(app, &app.state::<AppState>().devices.list());
            true
        }
        Err(error) => {
            tracing::warn!(%error, "could not create tray icon; using normal window controls");
            false
        }
    }
}

fn build_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<TrayIcon<R>> {
    let open = MenuItem::with_id(
        app,
        OPEN_ID,
        "Open Hermes Control Center",
        true,
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(
        app,
        QUIT_ID,
        "Quit Hermes Control Center",
        true,
        None::<&str>,
    )?;
    let menu = Menu::with_items(app, &[&open, &separator, &quit])?;
    let icon = Image::from_bytes(include_bytes!("../icons/icon.png"))?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip(tray_tooltip(TrayDeviceStatus::NoDevices, 0))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            OPEN_ID => restore_main_window(app),
            QUIT_ID => request_quit(app.clone()),
            _ => {}
        })
        .build(app)
}

pub fn update_tray_status<R: Runtime>(app: &AppHandle<R>, devices: &[AndroidDevice]) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let status = aggregate_tray_status(devices);
    if let Err(error) = tray.set_tooltip(Some(tray_tooltip(status, devices.len()))) {
        tracing::warn!(%error, "failed to update tray status tooltip");
    }
    if let Some(icon) = tray_status_icon(status) {
        if let Err(error) = tray.set_icon(Some(icon)) {
            tracing::warn!(%error, "failed to update tray status icon");
        }
    }
}

fn restore_main_window<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    if window.is_minimized().unwrap_or(false) {
        let _ = window.unminimize();
    }
    let _ = window.show();
    let _ = window.set_focus();
}

fn request_quit<R: Runtime>(app: AppHandle<R>) {
    let lifecycle = app.state::<AppLifecycle>();
    if !lifecycle.begin_quit() {
        return;
    }

    let state = app.state::<AppState>();
    state.shutdown.cancel();
    let pty_sessions = state.pty_sessions.clone();
    let serials = state
        .devices
        .list()
        .into_iter()
        .map(|device| device.serial)
        .collect::<Vec<_>>();

    tauri::async_runtime::spawn(async move {
        pty_sessions.close_all().await;
        for serial in serials {
            app.state::<AppState>().release_device(&serial).await;
        }
        let streams = app.state::<AppState>().streams.clone();
        let _ = tokio::time::timeout(Duration::from_secs(3), async {
            while !streams.is_empty() {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await;
        app.exit(0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adb::parse::parse_devices;

    #[test]
    fn aggregate_tray_status_distinguishes_empty_connected_and_attention() {
        assert_eq!(aggregate_tray_status(&[]), TrayDeviceStatus::NoDevices);

        let connected = parse_devices("192.0.2.10:5555 device model:Phone\n");
        assert_eq!(
            aggregate_tray_status(&connected),
            TrayDeviceStatus::Connected
        );

        let mixed = parse_devices(
            "192.0.2.10:5555 device model:Phone\n192.0.2.11:5555 unauthorized model:Phone\n",
        );
        assert_eq!(aggregate_tray_status(&mixed), TrayDeviceStatus::Attention);
        assert_eq!(
            tray_tooltip(TrayDeviceStatus::Attention, mixed.len()),
            "Hermes Control Center: attention needed (2 devices)"
        );
    }

    #[test]
    fn tray_status_icons_have_distinct_indicators() {
        let connected = tray_status_icon(TrayDeviceStatus::Connected).unwrap();
        let attention = tray_status_icon(TrayDeviceStatus::Attention).unwrap();
        assert_ne!(connected.rgba(), attention.rgba());
    }

    #[test]
    fn closes_hide_only_when_a_tray_is_available() {
        assert!(AppLifecycle::new(true).should_hide_on_close());
        assert!(!AppLifecycle::new(false).should_hide_on_close());
    }

    #[test]
    fn only_minimization_hides_when_a_tray_is_available() {
        let lifecycle = AppLifecycle::new(true);
        assert!(lifecycle.should_hide_on_minimize(true));
        assert!(!lifecycle.should_hide_on_minimize(false));
        assert!(!AppLifecycle::new(false).should_hide_on_minimize(true));
    }

    #[test]
    fn explicit_quit_is_idempotent_and_disables_hide_behavior() {
        let lifecycle = AppLifecycle::new(true);
        assert!(lifecycle.begin_quit());
        assert!(!lifecycle.begin_quit());
        assert!(!lifecycle.should_hide_on_close());
        assert!(!lifecycle.should_hide_on_minimize(true));
    }
}
