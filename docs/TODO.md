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

## In Progress

- [~] [M0-S3 — ADB output fixtures](tasks/M0-foundations.md)
- [~] [M0-S4 — Provisioning feasibility](tasks/M0-foundations.md)
- [~] [M10-T1 — Error audit](tasks/M10-production.md)
- [~] [M10-T15 — Root README](tasks/M10-production.md)
- [~] [M13-T1 — Hermes session interface spike](tasks/M13-chat-sessions.md)
- [~] [M13-T6 — Cross-platform and performance QA](tasks/M13-chat-sessions.md)

## Blocked

- [!] [M4-T5 — Optional bootstrap via RUN_COMMAND](tasks/M4-termux-bridge.md): Android denied the intent because the ADB-shell sender lacks `com.termux.permission.RUN_COMMAND`; manual SSH setup remains supported.
- [!] M9 has no task breakdown: [the roadmap references `tasks/M9-multi-device.md`](MILESTONES.md), but that file is missing.

## Open Tickets

### M3 — Basic Terminal + Live Logs
- [ ] [M3-T10 — Manual QA](tasks/M3-basic-terminal-logs.md)

### M8 — Hermes Control API
- [ ] [M8-T1 — Service skeleton](tasks/M8-control-api.md)
- [ ] [M8-T2 — Endpoints](tasks/M8-control-api.md)
- [ ] [M8-T3 — Service tests](tasks/M8-control-api.md)
- [ ] [M8-T4 — JSON schema contract](tasks/M8-control-api.md)
- [ ] [M8-T5 — Process supervision](tasks/M8-control-api.md)
- [ ] [M8-T6 — ApiTransport](tasks/M8-control-api.md)
- [ ] [M8-T7 — ApiWsLogSource](tasks/M8-control-api.md)
- [ ] [M8-T8 — Install/upgrade from app](tasks/M8-control-api.md)
- [ ] [M8-T9 — Transport switch in Settings](tasks/M8-control-api.md)

### M10 — Hardening & Production
- [ ] [M10-T2 — Security review](tasks/M10-production.md)
- [ ] [M10-T3 — Performance budgets](tasks/M10-production.md)
- [ ] [M10-T4 — Accessibility and keyboard](tasks/M10-production.md)
- [ ] [M10-T5 — Resilience QA matrix](tasks/M10-production.md)
- [ ] [M10-T6 — Diagnostics bundle](tasks/M10-production.md)
- [ ] [M10-T7 — First-run onboarding](tasks/M10-production.md)
- [ ] [M10-T8 — App icon, About, version, and menu items](tasks/M10-production.md)
- [ ] [M10-T9 — Stretch tray status, crash notifications, and health timeline](tasks/M10-production.md)
- [ ] [M10-T10 — Signing and notarization](tasks/M10-production.md)
- [ ] [M10-T11 — Universal build](tasks/M10-production.md)
- [ ] [M10-T12 — Auto-updater](tasks/M10-production.md)
- [ ] [M10-T13 — Release workflow](tasks/M10-production.md)
- [ ] [M10-T14 — Versioning and changelog](tasks/M10-production.md)

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

### M14 — Tray & Window Lifecycle
- [ ] [M14-T1 — Tray icon and menu](tasks/M14-tray-lifecycle.md)
- [ ] [M14-T2 — Close and minimize behavior](tasks/M14-tray-lifecycle.md)
- [ ] [M14-T3 — Platform support and fallback](tasks/M14-tray-lifecycle.md)
- [ ] [M14-T4 — Documentation and release QA](tasks/M14-tray-lifecycle.md)

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
- [ ] [Verify transport parity, contract tests, and loopback-only service](tasks/M8-control-api.md)

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
- [ ] [Closing/minimizing keeps work alive in the tray](tasks/M14-tray-lifecycle.md)
- [ ] [Tray restore and explicit quit work](tasks/M14-tray-lifecycle.md)
- [ ] [Quit cancels streams and managed child processes](tasks/M14-tray-lifecycle.md)
- [ ] [Document and verify behavior/fallback on macOS, Windows, and Linux](tasks/M14-tray-lifecycle.md)
- [ ] [Tag `v1.3.0`](tasks/M14-tray-lifecycle.md)
