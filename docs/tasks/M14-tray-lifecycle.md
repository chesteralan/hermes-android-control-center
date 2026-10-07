# M14 — Tray & Window Lifecycle (v1.3.0)

**Goal:** Keep Hermes Control Center available in the system tray when its main window is closed or minimized, while making explicit quit behavior reliable. **Depends on:** M13.

## Scope

- Closing the main window hides it to the tray instead of exiting the process.
- Minimizing the main window hides it to the tray instead of leaving a separate minimized taskbar/Dock window.
- Hiding the window does not stop device connections, background work, or active streams.
- The tray menu restores and focuses the existing main window, and provides an explicit Quit action.
- Quit performs normal application shutdown, including cancellation of streams and child processes; it does not leave `adb` or other managed processes running.
- Use the native menu-bar/tray conventions on macOS, Windows, and Linux. Detect unavailable tray support and provide a documented, usable fallback rather than silently hiding the only way to access or quit the app.

## Tasks

### [x] M14-T1 Tray icon and menu
- Add a bundled tray icon with accessible labels and menu items to open the main window and quit.
- Reuse and focus the existing window when opening from the tray; do not create duplicate windows.
- Keep tray status presentation consistent with the aggregate phone status if the M10 tray status icon is implemented.
- **Tests:** tray menu actions, repeated restore requests, window focus, and explicit quit.
- **Implementation note:** Added a bundled PNG tray icon with labeled Open and Quit items. Open reuses the `main` window, restores it from minimized/hidden state, and focuses it. macOS QA verified the tray menu restores the same window without a duplicate and exposes explicit Quit.

### [x] M14-T2 Close and minimize behavior
- Intercept close requests and minimize actions to hide the main window while keeping the application and its managed work alive.
- Ensure the window can be restored from the tray after close or minimize and that normal app shutdown still runs only on explicit Quit or OS shutdown.
- Handle repeated close/minimize/restore events without duplicate callbacks or lost window state.
- **Tests:** close-to-tray, minimize-to-tray, restore, active-stream continuity, and shutdown cleanup.
- **Implementation note:** Close requests are prevented and hidden only when tray initialization succeeds; focus-loss/resize events check native minimized state before hiding. Hiding does not cancel app state. Explicit Quit cancels the root token, closes PTYs, releases per-device SSH sessions/ADB forwards, waits up to three seconds for streams, then exits. macOS QA verified close/minimize hide, continued logcat streaming while hidden, restore, and explicit Quit reaping the `adb logcat` child.

### [~] M14-T3 Platform support and fallback
- Implement and verify native tray behavior on macOS, Windows, and Linux, including Wayland/X11 where applicable.
- Where a tray is unavailable, keep an accessible route to restore and quit, and document the platform limitation and fallback behavior.
- Verify keyboard and accessibility behavior for tray menu actions and window restoration.
- **Tests:** per-platform build/CI coverage plus a manual desktop-environment matrix.
- **Implementation note:** The existing CI matrix covers macOS, Windows, and Ubuntu builds. Runtime tray initialization is attempted on macOS/Windows/X11; Linux Wayland conservatively keeps normal window/taskbar behavior because a usable tray host cannot be reliably detected. Windows/Linux/Wayland runtime QA remains.

### [~] M14-T4 Documentation and release QA
- Document close/minimize behavior, how to restore the window, explicit quit, and tray-unavailable fallback.
- Verify that hiding the window preserves connected-device activity and that explicit quit cancels streams and reaps managed child processes.
- Run the normal milestone CI gates and smoke-test install, launch, hide, restore, and quit on all supported platforms.
- **Implementation note:** User behavior and the Wayland/tray-init fallback are documented in the root README. macOS install/launch/hide/restore/quit was smoke-tested; multi-platform release QA remains.

## Exit check

- [x] macOS QA: closing/minimizing hides the window while app work and the logcat stream continue.
- [x] macOS QA: the tray restores the existing window and explicitly quits the app.
- [x] macOS QA: explicit Quit cancels the logcat stream and reaps its managed child process.
- [~] Supported behavior and the Wayland/tray-init fallback are documented; macOS/Windows/Linux runtime QA remains.
- [ ] Tag `v1.3.0`.