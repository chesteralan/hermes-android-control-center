# M5 — Hermes Management (Phase 3)

**Goal:** Hermes status + Start/Stop/Restart via configurable commands. **Depends on:** M4, M0-S2.

## Backend

### [x] M5-T1 `HermesConfig` finalization
- Fields: `environment`, `start_mode` (`Supervised` | `Detached`), `gateway_command`, `process_match`, `gateway_match`, `hermes_home`, `path_prepend`, optional start/stop/restart/status/log overrides, `version_command`, `doctor_command`, `update_command`.
- Commands are written as they would be typed **inside** the environment; the app adds the environment wrapper. Nothing Hermes-specific in code.
- Debian proot + official-installer defaults are prefilled from the verified spike; Detect Hermes can replace environment/PATH with a discovered candidate.

### [x] M5-T1a Hermes environment + command wrapper (`hermes/env.rs`)
```rust
pub enum HermesEnvironment { Termux, ProotDistro { distro: String } }
```
- `wrap(env, cmd)`:
  - `Termux` → `cmd`
  - `ProotDistro` → `proot-distro login <distro> -- bash -c '<export PATH=...; command>'`
- `path_prepend` comes from detection or Settings.
- Status/process/log state reads use the Termux-visible rootfs (`containers/<distro>/rootfs` or `installed-rootfs/<distro>`); command execution enters proot.
- `Supervised` starts the app-managed supervisor; `Detached` starts the gateway without it and uses a separate restart path.
- Distro names are validated against `[a-zA-Z0-9_-]+`; shell values use POSIX quoting.
- **Tests:** wrapper quoting, rootfs layout selection, process self-match protection, supervised and detached action generation.
- Implementation complete; automated tests run locally. Real-phone SSH/proot command verification pending.

### [x] M5-T1b Hermes installation detection
- `detect_hermes(serial) -> HermesInstallReport` (on demand + after Termux check passes):
  1. Termux: `command -v hermes`, `python3 --version`.
  2. proot-distro: search both `containers/<distro>/rootfs` and `installed-rootfs/<distro>` for Hermes symlinks, resolve the binary path, read venv Python metadata, and report whether Hermes home exists.
  3. If found: run configured `version_command` inside the detected environment with a bounded timeout; version is `Unknown` if unavailable.
```rust
pub struct HermesInstallReport {
    pub candidates: Vec<HermesCandidate>,    // one per environment where found
    pub searched: Vec<String>,               // "termux", "proot:ubuntu", ...
}
pub struct HermesCandidate {
    pub environment: HermesEnvironment,
    pub binary_path: String,
    pub version: Option<String>,
    pub python_version: Option<String>,
}
```
- 0 candidates → "Hermes not found in Termux or any proot-distro" with `searched` list; 1 → offer to use it; >1 → user picks.
- Detection only looks up the `hermes` command on PATH; if not found, user can set a custom binary/command (no assumed install paths).
- **Tests:** parser fixtures for Termux/proot/multiple/none and version output parsing.
- Implementation complete; actual Hermes install detection on device remains unverified while ADB is disconnected.

### [x] M5-T1c Gateway supervisor (Termux side, ADR-015)
Hermes expects a service manager: `hermes gateway restart`, chat `/restart`, `hermes update` and the event-loop watchdog (exit 75) all exit and wait to be relaunched. proot has no systemd, so the app installs a tiny supervisor.
- `~/.hacc/hermes-supervisor.sh` (written over SSH, versioned by hash, check-then-write): loop → run wrapped `hermes gateway run` appending to the gateway log → on exit, if `~/.hacc/hermes.stop` exists remove it and exit; otherwise restart with backoff (2 s, 5 s, 15 s, 30 s; reset after 10 min healthy) and log to `~/.hacc/supervisor.log`.
- Start = launch supervisor with `nohup setsid … &` (no-op if already running, checked via pidfile + `kill -0`).
- Stop = touch stop flag, `kill -TERM` the gateway, wait for supervisor exit (timeout → report, never `kill -9` silently).
- Restart (graceful) = `kill -USR1` the gateway (Hermes drains in-flight turns, then exits) → supervisor relaunches. "Restart now" = `kill -TERM` → supervisor relaunches.
- Status exposes both supervisor PID and gateway PID; crash-loop (≥ 5 restarts in 10 min) surfaces as a warning.
- **Tests:** script rendering, idempotent start and stop/restart command generation are covered. FakeTransport end-to-end action sequences and shellcheck in CI remain.
- Implementation complete; real gateway supervisor lifecycle/reboot behavior remains unverified on device.

### [x] M5-T2 `HermesStatus` model + parsing (`hermes/status.rs`)
```rust
pub struct HermesStatus {
  pub gateway: ComponentStatus,        // Running | Degraded | Stopped | Unknown
  pub gateway_pid: Option<u32>,
    pub uptime_secs: Option<u64>,
    pub python_version: Option<String>,
    pub processes: Vec<HermesProcess>,
    pub platforms: Vec<PlatformStatus>,  // only when reported by gateway_state.json
    pub supervisor: SupervisorStatus,
    pub warnings: Vec<String>,
    pub raw_status_output: Option<String>,
    pub source: String,                  // termuxSsh | adb (limited)
    pub checked_at: i64,
}
```
- Process detection: `pgrep -f <process_match>` → PIDs; uptime via `ps -o etimes= -p <pid>` (fallback `/proc/<pid>/stat` + `/proc/uptime`).
- If `status_command` set: run it and preserve output in `raw_status_output`; process/state-file data remains authoritative.
- If `state_file` set (Hermes writes `gateway_state.json` with `gateway_state`, `exit_reason`, `updated_at`): read it from the Termux side via the host path and map `degraded` / stale heartbeat (`updated_at` > 120 s old while the process is alive) to warnings.
- **Never fabricate**: missing data → `None`/`Unknown`.
- **Tests:** fixtures for running/stopped/multiple PIDs/garbage; etimes parsing.

### [x] M5-T3 `HermesManager`
- Per-device Tauri handlers use `TermuxSshTransport`; `ActionLocks` serializes actions by serial. M9 moves the key to stable `device_id` profiles.
- After actions, poll status using bounded backoff; a restart confirms only after gateway PID changes. Each command result includes an explicit `confirmed` flag.
- Actions are serialized per serial with `ActionLocks`; command failures retain stdout/stderr and exit status.
- **Tests:** command generation, detached restart behavior, PID transition; FakeTransport action-sequence tests remain.
- Implementation complete; device action verification is in the exit check below.

### [x] M5-T4 Commands
- `get_hermes_status`, `detect_hermes`, `hermes_action` (`start|stop|restart|restartNow`), `run_hermes_tool` (`doctor|update`) (all take `serial`).
- Status polling: frontend-driven while Hermes page/dashboard visible (every 10 s), not a backend global loop.

## Frontend

### [x] M5-T5 Hermes status card
- Installed: ✓ in Termux / ✓ in proot-distro `<distro>` / Not found / Unknown, with version when known.
- Agent ● Running/Stopped/Unknown, PID, uptime (humanized), Python (of the Hermes environment); Gateway card; integrations list only when present; "Last checked" time; raw status output in Details.
- **Tests:** running, stopped, unknown, partial, not installed, proot environment label.

### [x] M5-T6 Controls
- Start (disabled when running), Restart (graceful drain, with "Restart now" secondary), Stop; Stop & Restart open `ConfirmDialog` ("Hermes will stop processing messages").
- Extra actions: **Doctor** and **Update** use the configured commands; Update restarts after success (supervisor handles Supervised mode; Detached mode is restarted by the app).
- Show action output and Doctor/Update output; errors via `ErrorPanel`.
- **Tests:** confirm required for Stop/Restart; disabled states; error rendering.

### [x] M5-T7 Dashboard integration
- Hermes + Gateway cards with Restart/Stop buttons as in brief §14.

### [x] M5-T8 Not-configured state
- Settings retains editable command fields and the Hermes card links to them; absent command overrides use the built-in configured gateway defaults.

### [x] M5-T9 Environment settings + detection UI
- Settings → Hermes: Environment selector (Termux / proot-distro + distro dropdown), start mode, configurable process/gateway match, HOME, PATH, gateway and status commands, version/Doctor/Update commands, "Preview" showing the wrapped gateway command.
- "Detect Hermes" button runs `detect_hermes`, lists candidates, "Use this" fills environment + suggested commands.
- **Tests:** preview matches wrapper; candidate selection fills fields.

## Exit check
- [ ] Verify on a connected phone: detect Hermes version/environment, Start/Stop/graceful Restart/Restart now/Doctor/Update, including status refresh and supervisor recovery.
- [ ] Confirm Hermes remains supervised after closing the app and restarts after a gateway exit/watchdog event.
