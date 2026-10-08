# Open Work

Consolidated snapshot of unfinished task headings and milestone exit checks as of 2026-10-08. The linked milestone task files remain the source of truth; update this index when their status changes.

Status: `[ ]` open, `[~]` in progress, `[!]` blocked. Items marked blocked need new evidence or a supported environment before implementation can proceed.

## Recently Completed

- [x] [M10-T1 — Recoverable SSH-key initialization failure](tasks/M10-production.md): poisoned initialization returns an error without creating a key; key reload/permissions behavior remains tested.
- [x] [M10-T1 — Observable logging setup and reload failures](tasks/M10-production.md): file/subscriber/reload errors are reported instead of silently ignored; console fallback is retained and broader startup QA remains open.
- [x] [M10-T1 — Consistent shell-exit and device-error classification](tasks/M10-production.md): command-not-found exits stay ordinary command results; missing-device streams report device errors.
- [x] [M10-T1 — Final streamed ADB stderr classification and details](tasks/M10-production.md): split-chunk errors without a final newline retain device-error classification and output; ordinary command exits remain unchanged.
- [x] [M10-T1 — Terminal input errors precede connection setup](tasks/M10-production.md): blank execute/stream requests are rejected before ADB/SSH/API setup; broader error audit remains open.
- [x] [M10-T1 — Provisioning-run lock poisoning returns an error](tasks/M10-production.md): IPC reports poisoned state and cleanup failures are logged; the wider error audit remains open.
- [x] [M10-T1 — Shared export destination validation and write-error coverage](tasks/M10-production.md): log, terminal, and diagnostics exports reject relative/empty/root paths; broader error audit and native-dialog QA remain open.
- [x] [M10-T6 — Quoted diagnostics secret redaction and synthetic ZIP regression coverage](tasks/M10-production.md): JSON/log secrets, escaped quoted values, Bearer headers, and both IP-consent settings verified; packaged-app archive QA remains open.
- [x] [M3-T10 — macOS live terminal and logcat QA](tasks/M3-basic-terminal-logs.md): stdout/stderr, exit status, streamed output, Cancel, and Logs Start/Stop/Clear verified on CPH2239.
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
- [x] [M11-T8 — Provisioning reuses the SSH bridge](tasks/M11-provisioning.md)
- [x] [M11-T16 — Form/raw-TOML recipe editor, validation, duplication, import/export](tasks/M11-provisioning.md)
- [x] [M12-T1 — Per-OS ADB candidates and install hints](tasks/M12-cross-platform.md)
- [x] [M12-T10 — Desktop setup, development, security, and troubleshooting guide](tasks/M12-cross-platform.md)

## In Progress

- [~] [M0-S3 — ADB output fixtures](tasks/M0-foundations.md)
- [~] [M0-S4 — Provisioning feasibility](tasks/M0-foundations.md)
- [~] [M10-T1 — Error audit](tasks/M10-production.md)
- [~] [M10-T2 — Isolated GLib backport passes optimized library tests and 241 Linux app tests/Clippy; strict dependency gate still blocked by six license rejections and one unmaintained advisory](tasks/M10-production.md)
- [~] [M10-T4 — Keyboard command palette and shortcut guide](tasks/M10-production.md)
- [~] [M10-T6 — Consent-gated diagnostics ZIP](tasks/M10-production.md)
- [~] [M10-T7 — Existing setup path; automatic wizard awaits M11](tasks/M10-production.md)
- [~] [M10-T8 — About, version, and native app menu](tasks/M10-production.md)
- [~] [M10-T9 — Tray status, named stop alerts, and persistent gateway timeline; packaged notification/runtime QA remains](tasks/M10-production.md)
- [~] [M10-T10–T13 — Signing, universal build, updater, and release workflow](tasks/M10-production.md)
- [~] [M10-T15 — Root README](tasks/M10-production.md)
- [~] [M13-T1 — Hermes session interface spike](tasks/M13-chat-sessions.md)
- [~] [M13-T6 — Cross-platform and performance QA](tasks/M13-chat-sessions.md)
- [~] [M14-T3–T4 — Windows/Linux/Wayland runtime and release QA](tasks/M14-tray-lifecycle.md)

## Blocked

- [!] [M4-T5 — Optional bootstrap via RUN_COMMAND](tasks/M4-termux-bridge.md): Android denied the intent because the ADB-shell sender lacks `com.termux.permission.RUN_COMMAND`; manual SSH setup remains supported.
- [!] M9 has no task breakdown: [the roadmap references `tasks/M9-multi-device.md`](MILESTONES.md), but that file is missing.

## Open Tickets

### M10 — Hardening & Production

- [ ] [M10-T3 — Performance budgets](tasks/M10-production.md)
- [ ] [M10-T5 — Resilience QA matrix](tasks/M10-production.md)

### M11 — New Phone Provisioning

Local foundations are implemented and tested (233 Rust, 101 frontend, 21 Python tests).
The task file lists remaining implementation details separately from phone-only QA;
this milestone is not complete.

- [~] [M11-T1 — Provisioning engine; in-operation interruption QA remains](tasks/M11-provisioning.md)
- [~] [M11-T2 — Recipes; native repository and per-phone configuration remain](tasks/M11-provisioning.md)
- [~] [M11-T3 — Configurable threshold and typed Termux uninstall implemented; factory-phone QA remains](tasks/M11-provisioning.md)
- [~] [M11-T4 — Version checks, APK progress and common install-error guidance implemented; OEM/source and phone QA remain](tasks/M11-provisioning.md)
- [~] [M11-T5 — SDK-scoped settings commands implemented; phone permission and keyguard QA remain](tasks/M11-provisioning.md)
- [~] [M11-T6 — Wake/keyguard detection and bounded Termux focus wait implemented; phone QA remains](tasks/M11-provisioning.md)
- [~] [M11-T7 — All-outcome bootstrap cleanup added; retry/interruption phone QA remains](tasks/M11-provisioning.md)
- [~] [M11-T9 — Package/wake-lock output streams implemented; connected-phone QA remains](tasks/M11-provisioning.md)
- [~] [M11-T10 — Existing-distro reruns are idempotent; partial-rootfs repair and phone QA remain](tasks/M11-provisioning.md)
- [~] [M11-T11 — Reviewed installer can be explicitly run; PATH discovery and phone QA remain](tasks/M11-provisioning.md)
- [~] [M11-T12 — Proot and gateway safety hints added; portal/config fallback remain](tasks/M11-provisioning.md)
- [~] [M11-T13 — Autostart hook; phone activation/reboot QA remains](tasks/M11-provisioning.md)
- [~] [M11-T14 — Start/status verification; M9 profile persistence remains](tasks/M11-provisioning.md)
- [~] [M11-T15 — Device-page wizard; entry points/state coverage/runtime QA remain](tasks/M11-provisioning.md)

### M12 — Cross-Platform

Local implementation passes 237 Rust, 106 frontend, and 3 release-assembly tests.
Hosted macOS/Windows/Ubuntu build/test jobs now pass; dependency policy still blocks
on existing license/advisory findings. Interactive runtime, signing, and installed-update
acceptance remain open; no cross-platform release certification is claimed.

- [~] [M12-T2 — Job Objects/no-console clients; Windows runtime/orphan QA remains](tasks/M12-cross-platform.md)
- [~] [M12-T3 — Native stores/private keys/encrypted fallback; platform runtime QA remains](tasks/M12-cross-platform.md)
- [~] [M12-T4 — Modifier/font mapping and PTY copy; native visual QA remains](tasks/M12-cross-platform.md)
- [~] [M12-T5 — Wayland/portal/udev/DMABUF guidance; Linux runtime QA remains](tasks/M12-cross-platform.md)
- [!] [M12-T6 — NSIS/MSI configuration; Authenticode credentials and Windows install QA required](tasks/M12-cross-platform.md)
- [!] [M12-T7 — AppImage/deb/rpm configuration; GPG credentials and Linux install QA required](tasks/M12-cross-platform.md)
- [~] [M12-T8 — Consolidated metadata/package-manager guard; installed update QA remains](tasks/M12-cross-platform.md)
- [!] [M12-T9 — Three-OS build/test jobs pass; dependency-policy review and hosted signing remain](tasks/M12-cross-platform.md)
- [~] [M12-T11 — QA matrix documented; native runtime/performance cells pending](tasks/M12-cross-platform.md)

### M16 — Public Download Distribution

GitHub Releases is the recommended primary host; the existing tag workflow assembles signed bundles as a draft. Public release remains gated on dependency-policy remediation, signing credentials, and platform install/update QA. Cloudflare R2 is optional pending a concrete custom-domain, retention, or traffic requirement.

- [ ] [M16-T1 — Release publication prerequisites](tasks/M16-download-distribution.md)
- [ ] [M16-T2 — Verify the GitHub Release pipeline](tasks/M16-download-distribution.md)
- [ ] [M16-T3 — Public download path and instructions](tasks/M16-download-distribution.md)
- [ ] [M16-T4 — Download and installation acceptance](tasks/M16-download-distribution.md)
- [ ] [M16-T5 — Optional Cloudflare R2 mirror evaluation](tasks/M16-download-distribution.md)

## Open Milestone Exit Checks

### M1 — Desktop Shell

- [~] [CI run and `v0.1.0-alpha.1` tag](tasks/M1-desktop-shell.md)

### M2 — ADB Core

- [~] [Live device info, mDNS discovery, rebuilt-app mDNS/IP disconnect, and Retry verified; fresh-phone QR/code pairing, wireless-toggle recovery, and release gates remain](tasks/M2-adb-core.md)

### M3 — Basic Terminal + Live Logs

- [ ] [Tag `v0.1.0` and attach the unsigned CI app](tasks/M3-basic-terminal-logs.md)

### M5 — Hermes Management

- [~] [Action-sequence regressions covered; live Hermes detection/actions blocked until ADB and SSH are restored](tasks/M5-hermes.md)
- [!] [Verify supervisor recovery after app close and gateway exit; usable phone connection required](tasks/M5-hermes.md)

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

### M16 — Public Download Distribution

- [ ] [Public, verified downloads available for each supported platform](tasks/M16-download-distribution.md)
- [ ] [Clean install and upgrade acceptance completed](tasks/M16-download-distribution.md)
