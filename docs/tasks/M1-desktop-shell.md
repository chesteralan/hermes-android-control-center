# M1 — Desktop Shell (Phase 1)

**Goal:** Dark developer-tool shell, navigation, settings, Rust skeleton. **Depends on:** M0.

## Backend

### [x] M1-T1 Rust module skeleton
- Create modules per ARCHITECTURE §2 (`error`, `state`, `config`, `process`, `adb`, `transport`, `hermes`, `logs`, `streams`, `commands`) with `mod.rs` and TODO-free minimal content.
- `main.rs` → `lib::run()`; `lib.rs` registers plugins and an empty `invoke_handler`.
- **DoD:** `cargo build`, `cargo clippy -D warnings` pass.

### [x] M1-T2 `AppError`
- Enum per ARCHITECTURE §3 with `thiserror` + `Serialize` (`tag = "kind"`).
- `impl AppError { fn user_message(&self) -> String }` — human-readable headline.
- Serialize shape: `{ kind, data, message }`.
- **Tests:** each variant serializes with a non-empty `message`.

### [x] M1-T3 `ProcessRunner` + `TokioRunner` + `FakeRunner`
- `run` with timeout → `AppError::Timeout`; `spawn_stream` returns stdout/stderr line streams + exit future; `kill_on_drop(true)`; never uses a shell; on Windows sets `CREATE_NO_WINDOW` and splits lines on `\r?\n`.
- `FakeRunner`: map argv → `RawOutput` or scripted stream lines.
- **Tests:** timeout, non-zero exit, stderr capture, stream ordering (using `/bin/echo`, `/bin/sh` only in tests of the runner itself).

### [x] M1-T4 Tracing
- `tracing-subscriber` with `EnvFilter` (default `info`), stdout in dev, `tracing-appender` daily rolling file in `app_log_dir()`.
- Runtime level change command `set_log_level`.
- **DoD:** log file appears after launch.

### [x] M1-T5 Config + persistence
- `AppConfig`, `HermesConfig`, `ReconnectConfig`, `TermuxConfig`, `ApiConfig` with `Default` (Hermes commands empty strings + UI hints, not fake defaults).
- Load/save via `tauri-plugin-store`; `version: u32` + `migrate()`.
- Commands: `get_settings`, `update_settings` (validates: ports 1–65535, adb path exists if set).
- **Tests:** default round-trip, migration from v0, validation errors.

### [x] M1-T6 ts-rs pipeline
- `#[derive(TS)] #[ts(export, export_to = "../../src/types/generated/")]` on IPC types.
- `npm run gen:types` runs `cargo test export_bindings`; CI checks `git diff --exit-code src/types/generated`.

### [x] M1-T7 Capabilities
- `capabilities/default.json`: only `core:default`, `store:default`, `dialog` (open/save), `fs` scoped to user-chosen export paths, `clipboard-manager:allow-write-text`. No shell plugin.

## Frontend

### [x] M1-T8 App layout
- Header: "Hermes Control Center", device selector placeholder, Settings button.
- Sidebar: Dashboard, Device, Hermes, Terminal, Logs, Settings (keyboard: Mod+1–Mod+6, where Mod = ⌘ on macOS, Ctrl on Windows/Linux).
- All file locations via Tauri path API (`app_config_dir`, `app_data_dir`, `app_log_dir`) — no hard-coded `~/Library` paths.
- `routeStore` (zustand) with typed `Route` union.
- **Tests:** clicking nav switches view; active item highlighted.

### [x] M1-T9 UI primitives
- `Button` (variants: default, primary, danger, ghost; loading state), `Card`, `StatusDot` (connected/disconnected/warning/unknown), `KeyValue` rows, `ConfirmDialog`, `ErrorPanel` (message + collapsible Details with copy), `EmptyState`, `Spinner`, `Toast`.
- **Tests:** `ErrorPanel` shows headline and toggles details; `ConfirmDialog` calls onConfirm only on confirm.

### [x] M1-T10 Typed IPC layer
- `src/lib/ipc.ts`: one function per command, typed with generated types; normalizes rejections into `AppError`.
- Only file importing `invoke`.

### [x] M1-T11 Settings page
- Sections: ADB (path + Detect button — wired in M2), Connection (auto-connect, reconnect attempts), Hermes commands (textareas), Logs (log command, auto-start), API (port), App (log level).
- Dirty-state + Save/Revert; validation errors inline.
- **Tests:** edit + save calls `update_settings`; validation message rendered.

### [x] M1-T12 Placeholder views
- Dashboard/Device/Hermes/Terminal/Logs show `EmptyState` "No phone connected" (gains pair/connect actions in M2-T11b).

## Exit check
- [~] App launches, nav works, settings persist across restarts (verified locally); CI run + tag `v0.1.0-alpha.1` pending first push.
