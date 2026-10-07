use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::state::AppState;

const MAIN_WINDOW: &str = "main";
const OPEN_ID: &str = "open-main-window";
const QUIT_ID: &str = "quit-application";
const ABOUT_MENU_ID: &str = "about-app";
const SETTINGS_MENU_ID: &str = "open-settings";
const UPDATE_MENU_ID: &str = "check-for-updates";
pub const MENU_COMMAND_EVENT: &str = "hacc://menu-command";

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
        Ok(_) => true,
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

    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Hermes Control Center")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            OPEN_ID => restore_main_window(app),
            QUIT_ID => request_quit(app.clone()),
            _ => {}
        })
        .build(app)
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
