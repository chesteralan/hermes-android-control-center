# Open Work

Consolidated snapshot of unfinished task headings and milestone exit checks as of 2026-10-07. The linked milestone task files remain the source of truth; update this index when their status changes.

Status: `[ ]` open, `[~]` in progress, `[!]` blocked. Items marked blocked need new evidence or a supported environment before implementation can proceed.

## Recently Completed

- [x] M0-S3 verified the sanitized `track_frames.bin` header (`0074` = 116 payload bytes) and added an exact-length parser assertion. The broader capture ticket remains in progress.
- [x] [M13-T5 — Device-scoped restoration and documentation](tasks/M13-chat-sessions.md): persistence, unavailable-device behavior, environment separation, privacy, and profile limitations are tested/documented.
- [x] [M13-T2 — Typed Rust session access](tasks/M13-chat-sessions.md): scoped commands, bounded pagination/responses, token-verified loopback access, and malformed/truncated JSON errors are implemented and tested; one Hermes v0.21.4 live check is recorded.
- [x] M13 session API limits are documented and regression-tested: 20-session recent list, 1–500 messages per page, 2 MiB list response, and 8 MiB transcript response.
- [x] M13-T1 local startup test rejects an unrelated loopback HTTP service without spawning Hermes; on-device conflict behavior remains open.
- [x] [M10-T16 — Contribution guide, security reporting policy, and MIT license](tasks/M10-production.md).
- [x] M13-T6 local macOS validation passes (182 Rust tests, 92 frontend tests, lint, typecheck, formatting, and Clippy); platform/device QA remains open.
- [x] M10-T1 removed a guarded production chat `exit_code.unwrap()`; the wider panic/error-propagation audit remains in progress.
- [x] [M10-T14 — Synchronized application versions and Keep a Changelog](tasks/M10-production.md)
- [x] [M8-T1 — Hermes Control API service skeleton](tasks/M8-control-api.md): Python package, aiohttp entrypoint, config/install script, and loopback/token bind guard.
- [x] [M8-T2–T9 — Endpoints, schemas, supervision, desktop transports, app install, and Settings switch](tasks/M8-control-api.md)
- [x] [M14-T1–T2 — Tray menu, close/minimize-to-tray, restore, and explicit quit verified on macOS](tasks/M14-tray-lifecycle.md)

## In Progress

- [~] [M0-S3 — ADB output fixtures](tasks/M0-foundations.md)
- [~] [M0-S4 — Provisioning feasibility](tasks/M0-foundations.md)
- [~] [M10-T1 — Error audit](tasks/M10-production.md)
- [~] [M10-T2 — Security review and CI dependency gates](tasks/M10-production.md)
- [~] [M10-T4 — Keyboard command palette and shortcut guide](tasks/M10-production.md)
- [~] [M10-T6 — Consent-gated diagnostics ZIP](tasks/M10-production.md)
- [~] [M10-T7 — Existing setup path; automatic wizard awaits M11](tasks/M10-production.md)
- [~] [M10-T8 — About, version, and native app menu](tasks/M10-production.md)
- [~] [M10-T10–T13 — Signing, universal build, updater, and release workflow](tasks/M10-production.md)
- [~] [M10-T15 — Root README](tasks/M10-production.md)
- [~] [M13-T1 — Hermes session interface spike](tasks/M13-chat-sessions.md)
- [~] [M13-T6 — Cross-platform and performance QA](tasks/M13-chat-sessions.md)
- [~] [M14-T3–T4 — Windows/Linux/Wayland runtime and release QA](tasks/M14-tray-lifecycle.md)

## Blocked

- [!] [M4-T5 — Optional bootstrap via RUN_COMMAND](tasks/M4-termux-bridge.md): Android denied the intent because the ADB-shell sender lacks `com.termux.permission.RUN_COMMAND`; manual SSH setup remains supported.
- [!] M9 has no task breakdown: [the roadmap references `tasks/M9-multi-device.md`](MILESTONES.md), but that file is missing.

## Open Tickets

### M3 — Basic Terminal + Live Logs

- [ ] [M3-T10 — Manual QA](tasks/M3-basic-terminal-logs.md)

### M10 — Hardening & Production

- [ ] [M10-T3 — Performance budgets](tasks/M10-production.md)
- [ ] [M10-T5 — Resilience QA matrix](tasks/M10-production.md)
- [ ] [M10-T9 — Stretch tray status, crash notifications, and health timeline](tasks/M10-production.md)

### M11 — New Phone Provisioning

- [ ] [M11-T1 — Provisioning engine](tasks/M11-provisioning.md)
- [ ] [M11-T2 — Recipes](tasks/M11-provisioning.md)
- [ ] [M11-T3 — Preflight](tasks/M11-provisioning.md)
- [ ] [M11-T4 — Install Termux and optional Termux:Boot](tasks/M11-provisioning.md)
- [ ] [M11-T5 — Android settings with consent](tasks/M11-provisioning.md)
- [ ] [M11-T6 — First Termux launch](tasks/M11-provisioning.md)
- [ ] [M11-T7 — Bootstrap SSH handoff](tasks/M11-provisioning.md)
- [ ] [M11-T8 — Connect over SSH](tasks/M11-provisioning.md)
- [ ] [M11-T9 — Termux packages and wake lock](tasks/M11-provisioning.md)
- [ ] [M11-T10 — Install proot-distro and distro](tasks/M11-provisioning.md)
- [ ] [M11-T11 — Install Hermes](tasks/M11-provisioning.md)
- [ ] [M11-T12 — Configure Hermes](tasks/M11-provisioning.md)
- [ ] [M11-T13 — Configure autostart](tasks/M11-provisioning.md)
- [ ] [M11-T14 — Verify and start](tasks/M11-provisioning.md)
- [ ] [M11-T15 — Setup wizard](tasks/M11-provisioning.md)
- [ ] [M11-T16 — Recipe editor](tasks/M11-provisioning.md)

### M12 — Cross-Platform

- [ ] [M12-T1 — ADB detection per OS](tasks/M12-cross-platform.md)
- [ ] [M12-T2 — Windows process spawning](tasks/M12-cross-platform.md)
- [ ] [M12-T3 — Secrets and key files](tasks/M12-cross-platform.md)
- [ ] [M12-T4 — UI and shortcuts](tasks/M12-cross-platform.md)
- [ ] [M12-T5 — Linux specifics](tasks/M12-cross-platform.md)
- [ ] [M12-T6 — Windows bundles](tasks/M12-cross-platform.md)
- [ ] [M12-T7 — Linux bundles](tasks/M12-cross-platform.md)
- [ ] [M12-T8 — Updater per platform](tasks/M12-cross-platform.md)
- [ ] [M12-T9 — Release workflow matrix](tasks/M12-cross-platform.md)
- [ ] [M12-T10 — Documentation](tasks/M12-cross-platform.md)
- [ ] [M12-T11 — QA matrix](tasks/M12-cross-platform.md)

## Open Milestone Exit Checks

### M1 — Desktop Shell

- [~] [CI run and `v0.1.0-alpha.1` tag](tasks/M1-desktop-shell.md)

### M2 — ADB Core

- [ ] [Exercise device, pairing, reconnect, and release checks on a real phone](tasks/M2-adb-core.md)

### M3 — Basic Terminal + Live Logs

- [ ] [Tag `v0.1.0` and attach the unsigned CI app](tasks/M3-basic-terminal-logs.md)

### M5 — Hermes Management

- [ ] [Verify Hermes detection and actions on a connected phone](tasks/M5-hermes.md)
- [ ] [Verify supervisor recovery after app close and gateway exit](tasks/M5-hermes.md)

### M7 — Logs Complete

- [ ] [Demonstrate brief §8 requirements on real Hermes logs](tasks/M7-logs.md)

### M8 — Hermes Control API

- [~] [Verify Hermes action state transitions; on-device API auth/status without fallback/log streaming and contract tests pass](tasks/M8-control-api.md)

### M10 — Hardening & Production

- [ ] [Fresh-Mac download, onboarding, and Hermes workflow QA](tasks/M10-production.md)
- [ ] [Tag `v1.0.0`](tasks/M10-production.md)

### M11 — New Phone Provisioning

- [ ] [Factory-fresh Android 13/14+ provisioning run](tasks/M11-provisioning.md)
- [ ] [Wizard rerun is idempotent](tasks/M11-provisioning.md)
- [ ] [Interrupted wizard resumes successfully](tasks/M11-provisioning.md)
- [ ] [Termux:Boot restores Hermes after reboot](tasks/M11-provisioning.md)
- [ ] [No bootstrap downloads or secrets remain](tasks/M11-provisioning.md)

### M12 — Cross-Platform

- [ ] [Windows and Linux install/update without warnings](tasks/M12-cross-platform.md)
- [ ] [Core workflows match across all three OSes](tasks/M12-cross-platform.md)
- [ ] [No orphan ADB processes on any OS](tasks/M12-cross-platform.md)
- [ ] [Tag `v1.1.0`](tasks/M12-cross-platform.md)

### M13 — Chat Session History

- [ ] [Browse app-created and gateway-created sessions](tasks/M13-chat-sessions.md)
- [x] [Document and test session API/page limits](tasks/M13-chat-sessions.md)
- [ ] [Establish and test the supported Hermes version range](tasks/M13-chat-sessions.md)
- [ ] [CI and connected-phone QA on all M12 platforms](tasks/M13-chat-sessions.md)

### M14 — Tray & Window Lifecycle
- [x] [macOS QA: close/minimize hides the window and preserves active work](tasks/M14-tray-lifecycle.md)
- [x] [macOS QA: tray restore and explicit quit work](tasks/M14-tray-lifecycle.md)
- [x] [macOS QA: explicit quit cancels streams and reaps the logcat child](tasks/M14-tray-lifecycle.md)
- [~] [Behavior/fallback documented; macOS, Windows, and Linux runtime QA pending](tasks/M14-tray-lifecycle.md)
- [ ] [Tag `v1.3.0`](tasks/M14-tray-lifecycle.md)
