# M5 — Hermes Management (Phase 3)

**Goal:** Hermes status + Start/Stop/Restart via configurable commands. **Depends on:** M4, M0-S2.

## Backend

### [ ] M5-T1 `HermesConfig` finalization
- Fields: `transport`, `environment` (M5-T1a), `start_mode` (`Supervised` | `Detached` | `Foreground`), `start_command`, `stop_command`, `restart_command` (optional; if empty → stop then start), `status_command` (optional), `log_command`, `process_match`, `gateway_process_match` (optional), `python_version_command` (default `python3 --version`), `version_command` (optional, e.g. `hermes --version`), `state_file` (optional JSON status file), `doctor_command`, `update_command` (optional).
- Commands are written as they would be typed **inside** the environment; the app adds the environment wrapper. Nothing Hermes-specific in code.
- Settings UI: "Load suggested defaults" button that fills values from M0-S2 findings / M5-T1b detection, clearly marked as suggestions.

### [ ] M5-T1a Hermes environment + command wrapper (`hermes/env.rs`)
```rust
pub enum HermesEnvironment {
    Termux,
    ProotDistro { distro: String, user: Option<String>, extra_args: Vec<String>, path_prepend: Vec<String> },
    Custom { template: String },             // must contain {cmd}
}
```
- `wrap(env, cmd)`:
  - `Termux` → `cmd`
  - `ProotDistro` → `proot-distro login <distro> [--user <u>] [extra_args] -- bash -lc 'export PATH=<path_prepend>:$PATH; <escaped cmd>'`
  - `Custom` → template with `{cmd}` replaced by the shell-escaped command
- `path_prepend` is filled by detection (installers often add `hermes` to PATH only for interactive shells).
- `start_mode = Detached` wraps the start command as `nohup setsid <wrapped> > <log> 2>&1 < /dev/null &` (falls back to `nohup` if `setsid` missing) so Hermes survives the SSH session closing.
- `start_mode = Supervised` launches the M5-T1c supervisor instead (default for proot-distro).
- `guest_to_host_path(env, path)`: for proot-distro maps `/root/x` → `$PREFIX/var/lib/proot-distro/installed-rootfs/<distro>/root/x`, so logs are tailed from the Termux side without proot overhead.
- Process checks (`pgrep`, `ps`) run on the Termux side (proot guests are ordinary Termux-uid processes); fall back to running them inside the distro if M0-S2 shows guest argv isn't visible.
- Validate distro name `[a-z0-9_-]+`; reject templates without `{cmd}`.
- **Tests:** table tests for every wrapper incl. quotes/`$`/newlines in commands, detached mode, path mapping, invalid distro/template.

### [ ] M5-T1b Hermes installation detection
- `detect_hermes(serial) -> HermesInstallReport` (on demand + after Termux check passes):
  1. Termux: `command -v hermes`, `python3 --version`.
  2. proot-distro: list `$PREFIX/var/lib/proot-distro/installed-rootfs/*` (more stable than parsing `proot-distro list`); for each distro run `command -v hermes` via the wrapper, falling back to `bash -ic 'command -v hermes'` (installer PATH lives in `.bashrc`), plus `python3 --version` (sequential, 15 s timeout each).
  3. If found: run `version_command` if configured / `hermes --version` best-effort.
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
- **Tests:** FakeTransport: Termux-only, proot-only (debian), both, none, distro login failure, `.bashrc`-only PATH.

### [ ] M5-T1c Gateway supervisor (Termux side, ADR-015)
Hermes expects a service manager: `hermes gateway restart`, chat `/restart`, `hermes update` and the event-loop watchdog (exit 75) all exit and wait to be relaunched. proot has no systemd, so the app installs a tiny supervisor.
- `~/.hacc/hermes-supervisor.sh` (written over SSH, versioned by hash, check-then-write): loop → run wrapped `hermes gateway run` appending to the gateway log → on exit, if `~/.hacc/hermes.stop` exists remove it and exit; otherwise restart with backoff (2 s, 5 s, 15 s, 30 s; reset after 10 min healthy) and log to `~/.hacc/supervisor.log`.
- Start = launch supervisor with `nohup setsid … &` (no-op if already running, checked via pidfile + `kill -0`).
- Stop = touch stop flag, `kill -TERM` the gateway, wait for supervisor exit (timeout → report, never `kill -9` silently).
- Restart (graceful) = `kill -USR1` the gateway (Hermes drains in-flight turns, then exits) → supervisor relaunches. "Restart now" = `kill -TERM` → supervisor relaunches.
- Status exposes both supervisor PID and gateway PID; crash-loop (≥ 5 restarts in 10 min) surfaces as a warning.
- **Tests:** script rendered from template with escaped values; Rust side: start idempotent, stop/restart signal sequences via FakeTransport; shellcheck in CI.

### [ ] M5-T2 `HermesStatus` model + parsing (`hermes/status.rs`)
```rust
pub struct HermesStatus {
    pub installed: InstallState,         // Installed { environment } | NotFound | Unknown
    pub agent: ComponentStatus,          // Running | Stopped | Unknown
    pub pid: Option<u32>,
    pub uptime_secs: Option<u64>,
    pub python_version: Option<String>,
    pub gateway: Option<ComponentStatus>,
    pub integrations: Vec<IntegrationStatus>, // e.g. Telegram — only if reported
    pub raw_status_output: Option<String>,
    pub checked_at: i64,
}
```
- Process detection: `pgrep -f <process_match>` → PIDs; uptime via `ps -o etimes= -p <pid>` (fallback `/proc/<pid>/stat` + `/proc/uptime`).
- If `status_command` set: run it, keep raw output, parse known `key: value` lines tolerantly; unknown → `Unknown`.
- If `state_file` set (Hermes writes `gateway_state.json` with `gateway_state`, `exit_reason`, `updated_at`): read it from the Termux side via the host path and map `degraded` / stale heartbeat (`updated_at` > 120 s old while the process is alive) to warnings.
- **Never fabricate**: missing data → `None`/`Unknown`.
- **Tests:** fixtures for running/stopped/multiple PIDs/garbage; etimes parsing.

### [ ] M5-T3 `HermesManager`
- One instance per device, holds `Arc<dyn DeviceTransport>`; `status`, `start`, `stop`, `restart`. Uses that device's effective `HermesConfig` (global defaults until M9 adds per-phone overrides).
- Serialize actions per device (mutex) — no concurrent start+stop.
- After action, poll status (1 s, 2 s, 5 s) to confirm transition; return `CommandResult` + new status.
- Errors: missing command → `Config("Start command is not configured")`; process not found on stop → `HermesNotFound`.
- **Tests:** FakeTransport sequences for each action, concurrency lock, missing config.

### [ ] M5-T4 Commands
- `get_hermes_status`, `start_hermes`, `stop_hermes`, `restart_hermes` (all `serial`).
- Status polling: frontend-driven while Hermes page/dashboard visible (every 10 s), not a backend global loop.

## Frontend

### [ ] M5-T5 Hermes status card
- Installed: ✓ in Termux / ✓ in proot-distro `<distro>` / Not found / Unknown, with version when known.
- Agent ● Running/Stopped/Unknown, PID, uptime (humanized), Python (of the Hermes environment); Gateway card; integrations list only when present; "Last checked" time; raw status output in Details.
- **Tests:** running, stopped, unknown, partial, not installed, proot environment label.

### [ ] M5-T6 Controls
- Start (disabled when running), Restart (graceful drain, with "Restart now" secondary), Stop; Stop & Restart open `ConfirmDialog` ("Hermes will stop processing messages").
- Extra actions: **Doctor** (`doctor_command`, streamed output) and **Update** (`update_command` in PTY, then supervised restart); hidden when not configured.
- Show action output in collapsible panel; errors via `ErrorPanel`.
- **Tests:** confirm required for Stop/Restart; disabled states; error rendering.

### [ ] M5-T7 Dashboard integration
- Hermes + Gateway cards with Restart/Stop buttons as in brief §14.

### [ ] M5-T8 Not-configured state
- If commands empty → card shows "Configure Hermes commands" CTA linking to Settings.

### [ ] M5-T9 Environment settings + detection UI
- Settings → Hermes: Environment selector (Termux / proot-distro + distro dropdown from detected list / Custom template), start mode, "Preview" showing the exact wrapped command that will run.
- "Detect Hermes" button runs `detect_hermes`, lists candidates, "Use this" fills environment + suggested commands.
- **Tests:** preview matches wrapper; candidate selection fills fields.

## Exit check
- [ ] Start/Stop/Restart work on the real phone with configured commands; status reflects reality within 5 s.
- [ ] Same with Hermes installed inside proot-distro; Hermes keeps running after the app disconnects.
